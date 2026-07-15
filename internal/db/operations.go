package db

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"
)

// OperationStatus is the durable lifecycle state of a long-running operation.
type OperationStatus string

const (
	OperationStatusQueued    OperationStatus = "queued"
	OperationStatusRunning   OperationStatus = "running"
	OperationStatusComplete  OperationStatus = "complete"
	OperationStatusFailed    OperationStatus = "failed"
	OperationStatusCancelled OperationStatus = "cancelled"
	OperationStatusAbandoned OperationStatus = "abandoned"
)

// OperationKind identifies the work represented by an operation. The store
// accepts additional nonempty kinds so the operation model remains transport-neutral.
type OperationKind string

const (
	OperationKindReview       OperationKind = "review"
	OperationKindVerification OperationKind = "verification"
	OperationKindChat         OperationKind = "chat"
	OperationKindAnalyzer     OperationKind = "analyzer"
	OperationKindExport       OperationKind = "export"
)

const (
	// DefaultOperationLeaseDuration bounds how long an owner may be silent
	// before another process can recover the operation.
	DefaultOperationLeaseDuration = 30 * time.Second
	// MaxOperationLeaseDuration prevents a configuration from creating an
	// effectively unbounded lease.
	MaxOperationLeaseDuration = 24 * time.Hour
)

var (
	// ErrOperationNotFound is returned when an operation ID is not persisted.
	ErrOperationNotFound = errors.New("operation not found")
	// ErrRoundNotFound is returned when a round ID is not persisted.
	ErrRoundNotFound = errors.New("round not found")
	// ErrOperationActive is returned when a session already has queued or
	// running state-changing/model work.
	ErrOperationActive = errors.New("session already has an active operation")
	// ErrOperationNotAcquirable is returned when an operation cannot be leased.
	ErrOperationNotAcquirable = errors.New("operation cannot be acquired")
	// ErrOperationAlreadyOwned is returned when another process owns a lease.
	ErrOperationAlreadyOwned = errors.New("operation is owned by another process")
	// ErrOperationNotOwned is returned when the current process does not own a
	// running operation lease.
	ErrOperationNotOwned = errors.New("operation lease is not owned by this process")
	// ErrOperationLeaseExpired is returned when an owner tries to use an
	// expired lease before recovery can make it abandoned.
	ErrOperationLeaseExpired = errors.New("operation lease has expired")
	// ErrInvalidOperationTransition is returned for an unsupported lifecycle
	// transition.
	ErrInvalidOperationTransition = errors.New("invalid operation state transition")
)

// RoundStatus is the durable lifecycle state of a review round.
type RoundStatus string

const (
	RoundStatusPending    RoundStatus = "pending"
	RoundStatusRunning    RoundStatus = "running"
	RoundStatusComplete   RoundStatus = "complete"
	RoundStatusIncomplete RoundStatus = "incomplete"
	RoundStatusCancelled  RoundStatus = "cancelled"
)

// Round is the durable unit of one captured review attempt.
type Round struct {
	ID                 string
	SessionID          string
	RepositoryID       string
	SnapshotID         string
	PredecessorRoundID string
	Number             int
	Status             RoundStatus
	CreatedAt          time.Time
	UpdatedAt          time.Time
}

// Operation is the durable state and lease metadata for long-running work.
// Streamed progress is deliberately absent; persisted state is authoritative.
type Operation struct {
	ID             string
	SessionID      string
	RepositoryID   string
	RoundID        string
	Kind           OperationKind
	Status         OperationStatus
	OwnerID        string
	HeartbeatAt    time.Time
	LeaseExpiresAt time.Time
	Failure        string
	CreatedAt      time.Time
	UpdatedAt      time.Time
	StartedAt      time.Time
	FinishedAt     time.Time
}

// Activity is an append-only durable event. IDs are allocated by SQLite and
// are suitable for ordered SSE replay and export.
type Activity struct {
	ID           int64
	SessionID    string
	RepositoryID string
	RoundID      string
	OperationID  string
	Kind         string
	Status       string
	Message      string
	CreatedAt    time.Time
}

// Valid reports whether status is one of the persisted operation states.
func (status OperationStatus) Valid() bool {
	switch status {
	case OperationStatusQueued, OperationStatusRunning, OperationStatusComplete,
		OperationStatusFailed, OperationStatusCancelled, OperationStatusAbandoned:
		return true
	default:
		return false
	}
}

// CanTransitionTo reports whether the lifecycle permits a direct transition.
// Abandonment is only performed by lease recovery, not by ordinary callers.
func (status OperationStatus) CanTransitionTo(next OperationStatus) bool {
	switch status {
	case OperationStatusQueued:
		return next == OperationStatusRunning || next == OperationStatusCancelled
	case OperationStatusRunning:
		return next == OperationStatusComplete || next == OperationStatusFailed ||
			next == OperationStatusCancelled || next == OperationStatusAbandoned
	default:
		return false
	}
}

// ValidateOperationTransition validates one operation state transition.
func ValidateOperationTransition(from, to OperationStatus) error {
	if !from.Valid() {
		return fmt.Errorf("%w: unknown source state %q", ErrInvalidOperationTransition, from)
	}
	if !to.Valid() {
		return fmt.Errorf("%w: unknown target state %q", ErrInvalidOperationTransition, to)
	}
	if !from.CanTransitionTo(to) {
		return fmt.Errorf("%w: %s to %s", ErrInvalidOperationTransition, from, to)
	}
	return nil
}

// Valid reports whether status is one of the persisted round states.
func (status RoundStatus) Valid() bool {
	switch status {
	case RoundStatusPending, RoundStatusRunning, RoundStatusComplete,
		RoundStatusIncomplete, RoundStatusCancelled:
		return true
	default:
		return false
	}
}

// CreateRound creates the next numbered round for a session and makes it the
// session's current round in the same transaction as its activity entry.
func (store *RepositoryStore) CreateRound(ctx context.Context, sessionID string) (Round, error) {
	if err := store.validate(); err != nil {
		return Round{}, err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return Round{}, fmt.Errorf("create round: session ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("create round")
	if err != nil {
		return Round{}, err
	}

	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Round{}, fmt.Errorf("begin round transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	if _, err := recoverExpiredOperationsTx(ctx, tx, now); err != nil {
		return Round{}, err
	}
	repositoryID, err := sessionRepositoryIDTx(ctx, tx, sessionID)
	if err != nil {
		return Round{}, err
	}
	var predecessorRoundID string
	if err := tx.QueryRowContext(ctx,
		`SELECT COALESCE(current_round_id, '') FROM sessions WHERE id = ?`, sessionID,
	).Scan(&predecessorRoundID); err != nil {
		return Round{}, fmt.Errorf("read current round: %w", err)
	}
	var number int
	if err := tx.QueryRowContext(ctx,
		`SELECT COALESCE(MAX(number), 0) + 1 FROM rounds WHERE session_id = ?`, sessionID,
	).Scan(&number); err != nil {
		return Round{}, fmt.Errorf("choose round number: %w", err)
	}
	roundID, err := store.newID()
	if err != nil {
		return Round{}, fmt.Errorf("create round ID: %w", err)
	}
	round := Round{
		ID:                 roundID,
		SessionID:          sessionID,
		RepositoryID:       repositoryID,
		PredecessorRoundID: predecessorRoundID,
		Number:             number,
		Status:             RoundStatusPending,
		CreatedAt:          now,
		UpdatedAt:          now,
	}
	if _, err := tx.ExecContext(ctx, `
	INSERT INTO rounds (id, session_id, repository_id, predecessor_round_id, number, status, created_at, updated_at)
	VALUES (?, ?, ?, NULLIF(?, ''), ?, ?, ?, ?)`,
		round.ID, round.SessionID, round.RepositoryID, round.PredecessorRoundID,
		round.Number, round.Status,
		timestampString(round.CreatedAt), timestampString(round.UpdatedAt),
	); err != nil {
		return Round{}, fmt.Errorf("insert round: %w", err)
	}
	if _, err := tx.ExecContext(ctx,
		`UPDATE sessions SET current_round_id = ? WHERE id = ?`, round.ID, sessionID,
	); err != nil {
		return Round{}, fmt.Errorf("set current round: %w", err)
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID:    round.SessionID,
		RepositoryID: round.RepositoryID,
		RoundID:      round.ID,
		Kind:         "round.created",
		Status:       string(round.Status),
		Message:      "Round created.",
		CreatedAt:    now,
	}); err != nil {
		return Round{}, err
	}
	if err := tx.Commit(); err != nil {
		return Round{}, fmt.Errorf("commit round: %w", err)
	}
	return round, nil
}

// GetRound returns one persisted round.
func (store *RepositoryStore) GetRound(ctx context.Context, roundID string) (Round, error) {
	if err := store.validate(); err != nil {
		return Round{}, err
	}
	roundID = strings.TrimSpace(roundID)
	if roundID == "" {
		return Round{}, fmt.Errorf("get round: round ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	row := store.database.QueryRowContext(ctx, roundQuery+` WHERE id = ?`, roundID)
	round, err := scanRound(row)
	if errors.Is(err, sql.ErrNoRows) {
		return Round{}, fmt.Errorf("%w: %q", ErrRoundNotFound, roundID)
	}
	if err != nil {
		return Round{}, fmt.Errorf("get round %q: %w", roundID, err)
	}
	return round, nil
}

// ListRounds returns the complete immutable round history for a session in
// ascending round-number order.
func (store *RepositoryStore) ListRounds(ctx context.Context, sessionID string) ([]Round, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return nil, fmt.Errorf("list rounds: session ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, roundQuery+` WHERE session_id = ? ORDER BY number ASC`, sessionID)
	if err != nil {
		return nil, fmt.Errorf("list rounds: %w", err)
	}
	defer rows.Close()
	rounds := make([]Round, 0)
	for rows.Next() {
		round, err := scanRound(rows)
		if err != nil {
			return nil, fmt.Errorf("list rounds: %w", err)
		}
		rounds = append(rounds, round)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list rounds: %w", err)
	}
	return rounds, nil
}

// CreateOperation queues one operation. The partial unique index and this
// transaction together prevent competing queued/running operations per
// session, including callers in separate processes.
func (store *RepositoryStore) CreateOperation(ctx context.Context, sessionID, roundID string, kind OperationKind) (Operation, error) {
	if err := store.validateOperationStore(); err != nil {
		return Operation{}, err
	}
	sessionID = strings.TrimSpace(sessionID)
	roundID = strings.TrimSpace(roundID)
	kind = OperationKind(strings.TrimSpace(string(kind)))
	if sessionID == "" {
		return Operation{}, fmt.Errorf("create operation: session ID is empty")
	}
	if kind == "" {
		return Operation{}, fmt.Errorf("create operation: kind is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("create operation")
	if err != nil {
		return Operation{}, err
	}
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Operation{}, fmt.Errorf("begin operation transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	if _, err := recoverExpiredOperationsTx(ctx, tx, now); err != nil {
		return Operation{}, err
	}
	repositoryID, err := sessionRepositoryIDTx(ctx, tx, sessionID)
	if err != nil {
		return Operation{}, err
	}
	if roundID != "" {
		var roundRepositoryID string
		err := tx.QueryRowContext(ctx,
			`SELECT repository_id FROM rounds WHERE id = ? AND session_id = ?`, roundID, sessionID,
		).Scan(&roundRepositoryID)
		if errors.Is(err, sql.ErrNoRows) {
			return Operation{}, fmt.Errorf("%w: %q", ErrRoundNotFound, roundID)
		}
		if err != nil {
			return Operation{}, fmt.Errorf("validate operation round: %w", err)
		}
		if roundRepositoryID != repositoryID {
			return Operation{}, fmt.Errorf("%w: round belongs to another repository", ErrRoundNotFound)
		}
	}
	var activeID string
	err = tx.QueryRowContext(ctx, `
SELECT id FROM operations
WHERE session_id = ? AND status IN (?, ?)
ORDER BY created_at ASC, id ASC
LIMIT 1`, sessionID, OperationStatusQueued, OperationStatusRunning).Scan(&activeID)
	if err == nil {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationActive, activeID)
	}
	if !errors.Is(err, sql.ErrNoRows) {
		return Operation{}, fmt.Errorf("check active operation: %w", err)
	}
	operationID, err := store.newID()
	if err != nil {
		return Operation{}, fmt.Errorf("create operation ID: %w", err)
	}
	operation := Operation{
		ID:           operationID,
		SessionID:    sessionID,
		RepositoryID: repositoryID,
		RoundID:      roundID,
		Kind:         kind,
		Status:       OperationStatusQueued,
		Failure:      "",
		CreatedAt:    now,
		UpdatedAt:    now,
	}
	if _, err := tx.ExecContext(ctx, `
INSERT INTO operations (id, session_id, repository_id, round_id, kind, status, failure, created_at, updated_at)
VALUES (?, ?, ?, NULLIF(?, ''), ?, ?, ?, ?, ?)`,
		operation.ID, operation.SessionID, operation.RepositoryID, operation.RoundID, operation.Kind,
		operation.Status, operation.Failure, timestampString(operation.CreatedAt), timestampString(operation.UpdatedAt),
	); err != nil {
		if isActiveOperationConstraint(err) {
			return Operation{}, fmt.Errorf("%w: session %q", ErrOperationActive, sessionID)
		}
		return Operation{}, fmt.Errorf("insert operation: %w", err)
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID:    operation.SessionID,
		RepositoryID: operation.RepositoryID,
		RoundID:      operation.RoundID,
		OperationID:  operation.ID,
		Kind:         "operation.created",
		Status:       string(operation.Status),
		Message:      "Operation queued.",
		CreatedAt:    now,
	}); err != nil {
		return Operation{}, err
	}
	if err := tx.Commit(); err != nil {
		if isActiveOperationConstraint(err) {
			return Operation{}, fmt.Errorf("%w: session %q", ErrOperationActive, sessionID)
		}
		return Operation{}, fmt.Errorf("commit operation: %w", err)
	}
	return operation, nil
}

// GetOperation returns the authoritative persisted operation state.
func (store *RepositoryStore) GetOperation(ctx context.Context, operationID string) (Operation, error) {
	if err := store.validate(); err != nil {
		return Operation{}, err
	}
	operationID = strings.TrimSpace(operationID)
	if operationID == "" {
		return Operation{}, fmt.Errorf("get operation: operation ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	row := store.database.QueryRowContext(ctx, operationQuery+` WHERE id = ?`, operationID)
	operation, err := scanOperation(row)
	if errors.Is(err, sql.ErrNoRows) {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotFound, operationID)
	}
	if err != nil {
		return Operation{}, fmt.Errorf("get operation %q: %w", operationID, err)
	}
	return operation, nil
}

// ListOperations returns a session's operations in durable creation order.
func (store *RepositoryStore) ListOperations(ctx context.Context, sessionID string) ([]Operation, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return nil, fmt.Errorf("list operations: session ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, operationQuery+` WHERE session_id = ? ORDER BY created_at ASC, id ASC`, sessionID)
	if err != nil {
		return nil, fmt.Errorf("list operations: %w", err)
	}
	defer rows.Close()
	operations := make([]Operation, 0)
	for rows.Next() {
		operation, err := scanOperation(rows)
		if err != nil {
			return nil, fmt.Errorf("list operations: %w", err)
		}
		operations = append(operations, operation)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list operations: %w", err)
	}
	return operations, nil
}

// AcquireOperation takes the current process's lease on a queued operation.
// Recovery runs in this same transaction so an expired owner cannot block a
// new acquisition.
func (store *RepositoryStore) AcquireOperation(ctx context.Context, operationID string) (Operation, error) {
	if err := store.validateOperationStore(); err != nil {
		return Operation{}, err
	}
	operationID = strings.TrimSpace(operationID)
	if operationID == "" {
		return Operation{}, fmt.Errorf("acquire operation: operation ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("acquire operation")
	if err != nil {
		return Operation{}, err
	}
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Operation{}, fmt.Errorf("begin acquire transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := recoverExpiredOperationsTx(ctx, tx, now); err != nil {
		return Operation{}, err
	}
	operation, err := getOperationTx(ctx, tx, operationID)
	if errors.Is(err, sql.ErrNoRows) {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotFound, operationID)
	}
	if err != nil {
		return Operation{}, fmt.Errorf("read operation %q: %w", operationID, err)
	}
	if operation.Status == OperationStatusRunning {
		if operation.OwnerID == store.processID && operation.LeaseExpiresAt.After(now) {
			return operation, nil
		}
		if operation.OwnerID != "" && operation.OwnerID != store.processID {
			return Operation{}, fmt.Errorf("%w: %q", ErrOperationAlreadyOwned, operationID)
		}
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationLeaseExpired, operationID)
	}
	if operation.Status != OperationStatusQueued {
		return Operation{}, fmt.Errorf("%w: %s is %s", ErrOperationNotAcquirable, operationID, operation.Status)
	}
	leaseExpiresAt := now.Add(store.leaseDuration)
	result, err := tx.ExecContext(ctx, `
UPDATE operations
SET status = ?, owner_id = ?, heartbeat_at = ?, lease_expires_at = ?,
    updated_at = ?, started_at = COALESCE(started_at, ?)
WHERE id = ? AND status = ?`,
		OperationStatusRunning, store.processID, timestampString(now), timestampString(leaseExpiresAt),
		timestampString(now), timestampString(now), operationID, OperationStatusQueued,
	)
	if err != nil {
		return Operation{}, fmt.Errorf("acquire operation %q: %w", operationID, err)
	}
	updated, err := result.RowsAffected()
	if err != nil {
		return Operation{}, fmt.Errorf("check acquired operation %q: %w", operationID, err)
	}
	if updated != 1 {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotAcquirable, operationID)
	}
	if err := markRoundStatusTx(ctx, tx, operation, RoundStatusRunning, now); err != nil {
		return Operation{}, err
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID:    operation.SessionID,
		RepositoryID: operation.RepositoryID,
		RoundID:      operation.RoundID,
		OperationID:  operation.ID,
		Kind:         "operation.running",
		Status:       string(OperationStatusRunning),
		Message:      "Operation acquired.",
		CreatedAt:    now,
	}); err != nil {
		return Operation{}, err
	}
	operation, err = getOperationTx(ctx, tx, operationID)
	if err != nil {
		return Operation{}, fmt.Errorf("read acquired operation %q: %w", operationID, err)
	}
	if err := tx.Commit(); err != nil {
		return Operation{}, fmt.Errorf("commit acquired operation %q: %w", operationID, err)
	}
	return operation, nil
}

// RenewOperation extends the current owner's bounded lease and records the
// heartbeat durably. An expired lease is first recovered rather than revived.
func (store *RepositoryStore) RenewOperation(ctx context.Context, operationID string) (Operation, error) {
	if err := store.validateOperationStore(); err != nil {
		return Operation{}, err
	}
	operationID = strings.TrimSpace(operationID)
	if operationID == "" {
		return Operation{}, fmt.Errorf("renew operation: operation ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("renew operation")
	if err != nil {
		return Operation{}, err
	}
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Operation{}, fmt.Errorf("begin renew transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := recoverExpiredOperationsTx(ctx, tx, now); err != nil {
		return Operation{}, err
	}
	operation, err := getOperationTx(ctx, tx, operationID)
	if errors.Is(err, sql.ErrNoRows) {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotFound, operationID)
	}
	if err != nil {
		return Operation{}, fmt.Errorf("read operation %q: %w", operationID, err)
	}
	if operation.Status != OperationStatusRunning {
		return Operation{}, fmt.Errorf("%w: %s is %s", ErrOperationNotAcquirable, operationID, operation.Status)
	}
	if operation.OwnerID != store.processID {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotOwned, operationID)
	}
	if !operation.LeaseExpiresAt.After(now) {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationLeaseExpired, operationID)
	}
	leaseExpiresAt := now.Add(store.leaseDuration)
	if _, err := tx.ExecContext(ctx, `
UPDATE operations
SET heartbeat_at = ?, lease_expires_at = ?, updated_at = ?
WHERE id = ? AND status = ? AND owner_id = ?`,
		timestampString(now), timestampString(leaseExpiresAt), timestampString(now),
		operationID, OperationStatusRunning, store.processID,
	); err != nil {
		return Operation{}, fmt.Errorf("renew operation %q: %w", operationID, err)
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID:    operation.SessionID,
		RepositoryID: operation.RepositoryID,
		RoundID:      operation.RoundID,
		OperationID:  operation.ID,
		Kind:         "operation.heartbeat",
		Status:       string(OperationStatusRunning),
		Message:      "Operation heartbeat renewed.",
		CreatedAt:    now,
	}); err != nil {
		return Operation{}, err
	}
	operation, err = getOperationTx(ctx, tx, operationID)
	if err != nil {
		return Operation{}, fmt.Errorf("read renewed operation %q: %w", operationID, err)
	}
	if err := tx.Commit(); err != nil {
		return Operation{}, fmt.Errorf("commit renewed operation %q: %w", operationID, err)
	}
	return operation, nil
}

// CompleteOperation durably completes a running operation owned by this
// process. No transient output can cause this transition.
func (store *RepositoryStore) CompleteOperation(ctx context.Context, operationID string) (Operation, error) {
	return store.finishOperation(ctx, operationID, OperationStatusComplete, "")
}

// FailOperation durably fails a running operation owned by this process.
func (store *RepositoryStore) FailOperation(ctx context.Context, operationID, failure string) (Operation, error) {
	return store.finishOperation(ctx, operationID, OperationStatusFailed, failure)
}

func (store *RepositoryStore) finishOperation(ctx context.Context, operationID string, target OperationStatus, failure string) (Operation, error) {
	if target != OperationStatusComplete && target != OperationStatusFailed {
		return Operation{}, fmt.Errorf("%w: finish target is %s", ErrInvalidOperationTransition, target)
	}
	if err := store.validateOperationStore(); err != nil {
		return Operation{}, err
	}
	operationID = strings.TrimSpace(operationID)
	if operationID == "" {
		return Operation{}, fmt.Errorf("finish operation: operation ID is empty")
	}
	if target == OperationStatusFailed {
		failure = strings.TrimSpace(failure)
		if failure == "" {
			failure = "Operation failed."
		}
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("finish operation")
	if err != nil {
		return Operation{}, err
	}
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Operation{}, fmt.Errorf("begin finish transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := recoverExpiredOperationsTx(ctx, tx, now); err != nil {
		return Operation{}, err
	}
	operation, err := getOperationTx(ctx, tx, operationID)
	if errors.Is(err, sql.ErrNoRows) {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotFound, operationID)
	}
	if err != nil {
		return Operation{}, fmt.Errorf("read operation %q: %w", operationID, err)
	}
	if err := validateOwnedRunningOperation(operation, store.processID, now, target); err != nil {
		return Operation{}, err
	}
	if _, err := tx.ExecContext(ctx, `
UPDATE operations
SET status = ?, failure = ?, updated_at = ?, finished_at = ?
WHERE id = ? AND status = ? AND owner_id = ?`,
		target, failure, timestampString(now), timestampString(now), operationID,
		OperationStatusRunning, store.processID,
	); err != nil {
		return Operation{}, fmt.Errorf("finish operation %q: %w", operationID, err)
	}
	if operation.RoundID != "" {
		roundStatus := RoundStatusIncomplete
		if target == OperationStatusComplete {
			roundStatus = RoundStatusComplete
		}
		if err := markRoundStatusTx(ctx, tx, operation, roundStatus, now); err != nil {
			return Operation{}, err
		}
	}
	message := failure
	kind := "operation.failed"
	if target == OperationStatusComplete {
		message = "Operation completed."
		kind = "operation.completed"
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID:    operation.SessionID,
		RepositoryID: operation.RepositoryID,
		RoundID:      operation.RoundID,
		OperationID:  operation.ID,
		Kind:         kind,
		Status:       string(target),
		Message:      message,
		CreatedAt:    now,
	}); err != nil {
		return Operation{}, err
	}
	operation, err = getOperationTx(ctx, tx, operationID)
	if err != nil {
		return Operation{}, fmt.Errorf("read finished operation %q: %w", operationID, err)
	}
	if err := tx.Commit(); err != nil {
		return Operation{}, fmt.Errorf("commit finished operation %q: %w", operationID, err)
	}
	return operation, nil
}

// CancelOperation durably cancels a queued or running operation. Repeating a
// cancellation is a successful no-op and returns the already-cancelled state.
func (store *RepositoryStore) CancelOperation(ctx context.Context, operationID string) (Operation, error) {
	if err := store.validateOperationStore(); err != nil {
		return Operation{}, err
	}
	operationID = strings.TrimSpace(operationID)
	if operationID == "" {
		return Operation{}, fmt.Errorf("cancel operation: operation ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("cancel operation")
	if err != nil {
		return Operation{}, err
	}
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Operation{}, fmt.Errorf("begin cancel transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := recoverExpiredOperationsTx(ctx, tx, now); err != nil {
		return Operation{}, err
	}
	operation, err := getOperationTx(ctx, tx, operationID)
	if errors.Is(err, sql.ErrNoRows) {
		return Operation{}, fmt.Errorf("%w: %q", ErrOperationNotFound, operationID)
	}
	if err != nil {
		return Operation{}, fmt.Errorf("read operation %q: %w", operationID, err)
	}
	if operation.Status == OperationStatusCancelled {
		return operation, nil
	}
	if operation.Status != OperationStatusQueued && operation.Status != OperationStatusRunning {
		return Operation{}, fmt.Errorf("%w: %s to %s", ErrInvalidOperationTransition, operation.Status, OperationStatusCancelled)
	}
	if _, err := tx.ExecContext(ctx, `
UPDATE operations
SET status = ?, updated_at = ?, finished_at = ?
WHERE id = ? AND status IN (?, ?)`,
		OperationStatusCancelled, timestampString(now), timestampString(now), operationID,
		OperationStatusQueued, OperationStatusRunning,
	); err != nil {
		return Operation{}, fmt.Errorf("cancel operation %q: %w", operationID, err)
	}
	if err := markRoundStatusTx(ctx, tx, operation, RoundStatusCancelled, now); err != nil {
		return Operation{}, err
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID:    operation.SessionID,
		RepositoryID: operation.RepositoryID,
		RoundID:      operation.RoundID,
		OperationID:  operation.ID,
		Kind:         "operation.cancelled",
		Status:       string(OperationStatusCancelled),
		Message:      "Operation cancelled.",
		CreatedAt:    now,
	}); err != nil {
		return Operation{}, err
	}
	operation, err = getOperationTx(ctx, tx, operationID)
	if err != nil {
		return Operation{}, fmt.Errorf("read cancelled operation %q: %w", operationID, err)
	}
	if err := tx.Commit(); err != nil {
		return Operation{}, fmt.Errorf("commit cancelled operation %q: %w", operationID, err)
	}
	return operation, nil
}

// RecoverExpiredOperations abandons every running operation whose bounded
// lease has expired. The operation and its round/activity records commit as a
// single transaction.
func (store *RepositoryStore) RecoverExpiredOperations(ctx context.Context) ([]Operation, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now, err := store.currentTime("recover expired operations")
	if err != nil {
		return nil, err
	}
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return nil, fmt.Errorf("begin operation recovery transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	recovered, err := recoverExpiredOperationsTx(ctx, tx, now)
	if err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, fmt.Errorf("commit operation recovery: %w", err)
	}
	return recovered, nil
}

// ListActivity returns committed activity after afterID in monotonic ID order.
func (store *RepositoryStore) ListActivity(ctx context.Context, sessionID string, afterID int64) ([]Activity, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return nil, fmt.Errorf("list activity: session ID is empty")
	}
	if afterID < 0 {
		return nil, fmt.Errorf("list activity: activity ID cannot be negative")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, activityQuery+` WHERE session_id = ? AND id > ? ORDER BY id ASC`, sessionID, afterID)
	if err != nil {
		return nil, fmt.Errorf("list activity: %w", err)
	}
	defer rows.Close()
	activities := make([]Activity, 0)
	for rows.Next() {
		activity, err := scanActivity(rows)
		if err != nil {
			return nil, fmt.Errorf("list activity: %w", err)
		}
		activities = append(activities, activity)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list activity: %w", err)
	}
	return activities, nil
}

func (store *RepositoryStore) validateOperationStore() error {
	if err := store.validate(); err != nil {
		return err
	}
	if store.processIDErr != nil {
		return fmt.Errorf("create process instance ID: %w", store.processIDErr)
	}
	if strings.TrimSpace(store.processID) == "" {
		return fmt.Errorf("repository store process instance ID is empty")
	}
	if store.leaseDuration <= 0 || store.leaseDuration > MaxOperationLeaseDuration {
		return fmt.Errorf("operation lease duration %s is outside the bounded range", store.leaseDuration)
	}
	return nil
}

func (store *RepositoryStore) currentTime(operation string) (time.Time, error) {
	now := store.now().UTC()
	if now.IsZero() {
		return time.Time{}, fmt.Errorf("%s: clock returned zero time", operation)
	}
	return now, nil
}

func validateOwnedRunningOperation(operation Operation, processID string, now time.Time, target OperationStatus) error {
	if err := ValidateOperationTransition(operation.Status, target); err != nil {
		return err
	}
	if operation.OwnerID != processID {
		return fmt.Errorf("%w: %q", ErrOperationNotOwned, operation.ID)
	}
	if !operation.LeaseExpiresAt.After(now) {
		return fmt.Errorf("%w: %q", ErrOperationLeaseExpired, operation.ID)
	}
	return nil
}

type expiredOperation struct {
	ID           string
	SessionID    string
	RepositoryID string
	RoundID      string
}

func recoverExpiredOperationsTx(ctx context.Context, tx *sql.Tx, now time.Time) ([]Operation, error) {
	rows, err := tx.QueryContext(ctx, `
SELECT id, session_id, repository_id, COALESCE(round_id, '')
FROM operations
WHERE status = ? AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?
ORDER BY id ASC`, OperationStatusRunning, timestampString(now))
	if err != nil {
		return nil, fmt.Errorf("find expired operations: %w", err)
	}
	candidates := make([]expiredOperation, 0)
	for rows.Next() {
		var operation expiredOperation
		if err := rows.Scan(&operation.ID, &operation.SessionID, &operation.RepositoryID, &operation.RoundID); err != nil {
			_ = rows.Close()
			return nil, fmt.Errorf("read expired operation: %w", err)
		}
		candidates = append(candidates, operation)
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return nil, fmt.Errorf("read expired operations: %w", err)
	}
	if err := rows.Close(); err != nil {
		return nil, fmt.Errorf("close expired operations: %w", err)
	}

	recovered := make([]Operation, 0, len(candidates))
	for _, candidate := range candidates {
		result, err := tx.ExecContext(ctx, `
UPDATE operations
SET status = ?, failure = ?, updated_at = ?, finished_at = ?
WHERE id = ? AND status = ? AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?`,
			OperationStatusAbandoned,
			"Operation lease expired before completion.",
			timestampString(now), timestampString(now), candidate.ID, OperationStatusRunning, timestampString(now),
		)
		if err != nil {
			return nil, fmt.Errorf("abandon operation %q: %w", candidate.ID, err)
		}
		updated, err := result.RowsAffected()
		if err != nil {
			return nil, fmt.Errorf("check abandoned operation %q: %w", candidate.ID, err)
		}
		if updated != 1 {
			continue
		}
		operation, err := getOperationTx(ctx, tx, candidate.ID)
		if err != nil {
			return nil, fmt.Errorf("read abandoned operation %q: %w", candidate.ID, err)
		}
		if err := markRoundStatusTx(ctx, tx, operation, RoundStatusIncomplete, now); err != nil {
			return nil, err
		}
		if err := insertActivityTx(ctx, tx, Activity{
			SessionID:    operation.SessionID,
			RepositoryID: operation.RepositoryID,
			RoundID:      operation.RoundID,
			OperationID:  operation.ID,
			Kind:         "operation.abandoned",
			Status:       string(OperationStatusAbandoned),
			Message:      operation.Failure,
			CreatedAt:    now,
		}); err != nil {
			return nil, err
		}
		recovered = append(recovered, operation)
	}
	return recovered, nil
}

func sessionRepositoryIDTx(ctx context.Context, tx *sql.Tx, sessionID string) (string, error) {
	var repositoryID string
	err := tx.QueryRowContext(ctx, `SELECT repository_id FROM sessions WHERE id = ?`, sessionID).Scan(&repositoryID)
	if errors.Is(err, sql.ErrNoRows) {
		return "", fmt.Errorf("%w: %q", ErrSessionNotFound, sessionID)
	}
	if err != nil {
		return "", fmt.Errorf("read session %q: %w", sessionID, err)
	}
	return repositoryID, nil
}

func markRoundStatusTx(ctx context.Context, tx *sql.Tx, operation Operation, status RoundStatus, now time.Time) error {
	if operation.RoundID == "" {
		return nil
	}
	if !status.Valid() {
		return fmt.Errorf("mark round: invalid status %q", status)
	}
	result, err := tx.ExecContext(ctx, `
UPDATE rounds
SET status = ?, updated_at = ?
WHERE id = ? AND session_id = ? AND repository_id = ?
  AND status IN (?, ?)`,
		status, timestampString(now), operation.RoundID, operation.SessionID, operation.RepositoryID,
		RoundStatusPending, RoundStatusRunning,
	)
	if err != nil {
		return fmt.Errorf("update round %q to %s: %w", operation.RoundID, status, err)
	}
	updated, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check round %q status: %w", operation.RoundID, err)
	}
	if updated == 1 {
		if status == RoundStatusIncomplete || status == RoundStatusCancelled {
			if _, err := tx.ExecContext(ctx, `
UPDATE sessions
SET current_round_id = (
    SELECT NULLIF(predecessor_round_id, '') FROM rounds WHERE id = ?
)
WHERE id = ? AND current_round_id = ?`, operation.RoundID, operation.SessionID, operation.RoundID); err != nil {
				return fmt.Errorf("restore prior current round after %s: %w", status, err)
			}
		}
		if err := insertActivityTx(ctx, tx, Activity{
			SessionID:    operation.SessionID,
			RepositoryID: operation.RepositoryID,
			RoundID:      operation.RoundID,
			OperationID:  operation.ID,
			Kind:         "round." + string(status),
			Status:       string(status),
			Message:      "Round status changed to " + string(status) + ".",
			CreatedAt:    now,
		}); err != nil {
			return err
		}
	}
	return nil
}

func insertActivityTx(ctx context.Context, tx *sql.Tx, activity Activity) error {
	if activity.SessionID == "" || activity.RepositoryID == "" {
		return fmt.Errorf("insert activity: session and repository IDs are required")
	}
	if activity.Kind == "" || activity.Status == "" {
		return fmt.Errorf("insert activity: kind and status are required")
	}
	if activity.CreatedAt.IsZero() {
		return fmt.Errorf("insert activity: creation time is zero")
	}
	var roundID any
	if activity.RoundID != "" {
		roundID = activity.RoundID
	}
	var operationID any
	if activity.OperationID != "" {
		operationID = activity.OperationID
	}
	if _, err := tx.ExecContext(ctx, `
INSERT INTO activity (session_id, repository_id, round_id, operation_id, kind, status, message, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		activity.SessionID, activity.RepositoryID, roundID, operationID, activity.Kind,
		activity.Status, activity.Message, timestampString(activity.CreatedAt),
	); err != nil {
		return fmt.Errorf("insert activity: %w", err)
	}
	return nil
}

const roundQuery = `
SELECT id, session_id, repository_id, COALESCE(snapshot_id, ''),
       COALESCE(predecessor_round_id, ''), number, status, created_at, updated_at
FROM rounds`

const operationQuery = `
SELECT id, session_id, repository_id, COALESCE(round_id, ''), kind, status,
       COALESCE(owner_id, ''), COALESCE(heartbeat_at, ''),
       COALESCE(lease_expires_at, ''), failure, created_at, updated_at,
       COALESCE(started_at, ''), COALESCE(finished_at, '')
FROM operations`

const activityQuery = `
SELECT id, session_id, repository_id, COALESCE(round_id, ''),
       COALESCE(operation_id, ''), kind, status, message, created_at
FROM activity`

func getOperationTx(ctx context.Context, tx *sql.Tx, operationID string) (Operation, error) {
	return scanOperation(tx.QueryRowContext(ctx, operationQuery+` WHERE id = ?`, operationID))
}

func scanRound(row scanner) (Round, error) {
	var round Round
	var createdAtRaw, updatedAtRaw string
	if err := row.Scan(
		&round.ID, &round.SessionID, &round.RepositoryID, &round.SnapshotID,
		&round.PredecessorRoundID, &round.Number, &round.Status,
		&createdAtRaw, &updatedAtRaw,
	); err != nil {
		return Round{}, err
	}
	var err error
	round.CreatedAt, err = parseTimestamp(createdAtRaw)
	if err != nil {
		return Round{}, fmt.Errorf("parse round creation time: %w", err)
	}
	round.UpdatedAt, err = parseTimestamp(updatedAtRaw)
	if err != nil {
		return Round{}, fmt.Errorf("parse round update time: %w", err)
	}
	return round, nil
}

func scanOperation(row scanner) (Operation, error) {
	var operation Operation
	var heartbeatRaw, expiresRaw, startedRaw, finishedRaw string
	var createdRaw, updatedRaw string
	if err := row.Scan(
		&operation.ID, &operation.SessionID, &operation.RepositoryID, &operation.RoundID,
		&operation.Kind, &operation.Status, &operation.OwnerID, &heartbeatRaw, &expiresRaw,
		&operation.Failure, &createdRaw, &updatedRaw, &startedRaw, &finishedRaw,
	); err != nil {
		return Operation{}, err
	}
	var err error
	operation.CreatedAt, err = parseTimestamp(createdRaw)
	if err != nil {
		return Operation{}, fmt.Errorf("parse operation creation time: %w", err)
	}
	operation.UpdatedAt, err = parseTimestamp(updatedRaw)
	if err != nil {
		return Operation{}, fmt.Errorf("parse operation update time: %w", err)
	}
	if heartbeatRaw != "" {
		operation.HeartbeatAt, err = parseTimestamp(heartbeatRaw)
		if err != nil {
			return Operation{}, fmt.Errorf("parse operation heartbeat time: %w", err)
		}
	}
	if expiresRaw != "" {
		operation.LeaseExpiresAt, err = parseTimestamp(expiresRaw)
		if err != nil {
			return Operation{}, fmt.Errorf("parse operation lease time: %w", err)
		}
	}
	if startedRaw != "" {
		operation.StartedAt, err = parseTimestamp(startedRaw)
		if err != nil {
			return Operation{}, fmt.Errorf("parse operation start time: %w", err)
		}
	}
	if finishedRaw != "" {
		operation.FinishedAt, err = parseTimestamp(finishedRaw)
		if err != nil {
			return Operation{}, fmt.Errorf("parse operation finish time: %w", err)
		}
	}
	return operation, nil
}

func scanActivity(row scanner) (Activity, error) {
	var activity Activity
	var createdRaw string
	if err := row.Scan(
		&activity.ID, &activity.SessionID, &activity.RepositoryID, &activity.RoundID,
		&activity.OperationID, &activity.Kind, &activity.Status, &activity.Message, &createdRaw,
	); err != nil {
		return Activity{}, err
	}
	var err error
	activity.CreatedAt, err = parseTimestamp(createdRaw)
	if err != nil {
		return Activity{}, fmt.Errorf("parse activity creation time: %w", err)
	}
	return activity, nil
}

func isActiveOperationConstraint(err error) bool {
	message := strings.ToLower(err.Error())
	return strings.Contains(message, "operations_active_session_idx") ||
		strings.Contains(message, "unique constraint failed: operations.session_id")
}
