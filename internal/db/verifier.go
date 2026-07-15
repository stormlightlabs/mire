package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/shared"
)

var _ review.VerificationStore = (*RepositoryStore)(nil)

var (
	// ErrVerificationRunNotFound indicates that a verifier run is not persisted.
	ErrVerificationRunNotFound = errors.New("verification run not found")
	// ErrVerificationNotFound indicates that an immutable verification record is
	// not persisted.
	ErrVerificationNotFound = errors.New("verification not found")
)

// CreateVerificationRun persists one queued verifier run for a retained
// candidate. The candidate and run are bound to the same session, round, and
// snapshot before any model work starts.
func (store *RepositoryStore) CreateVerificationRun(ctx context.Context, run review.VerificationRunRecord) (review.VerificationRunRecord, error) {
	if err := store.validate(); err != nil {
		return review.VerificationRunRecord{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if strings.TrimSpace(run.CandidateID) == "" {
		return review.VerificationRunRecord{}, errors.New("create verification run: candidate ID is empty")
	}
	if run.RunRecord.Role != review.ModelRoleVerifier || run.RunRecord.Status != review.RunStatusQueued {
		return review.VerificationRunRecord{}, errors.New("create verification run: run must be a queued verifier run")
	}
	if run.MaxAttempts < 1 {
		return review.VerificationRunRecord{}, errors.New("create verification run: max attempts must be positive")
	}
	if _, err := store.GetSession(ctx, run.SessionID); err != nil {
		return review.VerificationRunRecord{}, err
	}
	var candidateSession, candidateRound, candidateSnapshot string
	err := store.database.QueryRowContext(ctx, `
SELECT session_id, round_id, snapshot_id
FROM review_candidates WHERE id = ?`, run.CandidateID).Scan(&candidateSession, &candidateRound, &candidateSnapshot)
	if errors.Is(err, sql.ErrNoRows) {
		return review.VerificationRunRecord{}, fmt.Errorf("create verification run: %w: %q", ErrReviewCandidateNotFound, run.CandidateID)
	}
	if err != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("create verification run: read candidate: %w", err)
	}
	if candidateSession != run.SessionID || (run.RoundID != "" && candidateRound != run.RoundID) || (run.SnapshotID != "" && candidateSnapshot != run.SnapshotID) {
		return review.VerificationRunRecord{}, errors.New("create verification run: candidate provenance does not match run")
	}
	if run.RoundID == "" {
		run.RoundID = candidateRound
	}
	if run.SnapshotID == "" {
		run.SnapshotID = candidateSnapshot
	}
	if run.PassName == "" {
		run.PassName = "candidate-verification"
	}
	if run.ID == "" {
		var idErr error
		run.ID, idErr = store.newID()
		if idErr != nil {
			return review.VerificationRunRecord{}, fmt.Errorf("create verification run ID: %w", idErr)
		}
	}
	now := run.CreatedAt.UTC()
	if now.IsZero() {
		now = store.now().UTC()
	}
	if run.UpdatedAt.IsZero() {
		run.UpdatedAt = now
	}
	parameters, usage, redactions, err := encodeRunFields(run.RunRecord)
	if err != nil {
		return review.VerificationRunRecord{}, err
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO verification_runs (
    id, session_id, round_id, snapshot_id, candidate_id, status, attempt,
    max_attempts, error, adapter, protocol, prompt_template_version, model,
    parameters_json, input_manifest_digest, input_digest, output_digest,
    usage_json, finish_reason, redactions_json, termination_cause,
    retained_output, created_at, updated_at, started_at, finished_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULLIF(?, ''), NULLIF(?, ''))`,
		run.ID, run.SessionID, run.RoundID, run.SnapshotID, run.CandidateID, run.Status,
		run.Attempt, run.MaxAttempts, run.Error, run.Provenance.Adapter, run.Provenance.Protocol,
		run.Provenance.PromptTemplateVersion, run.Provenance.Model, parameters,
		run.Provenance.InputManifestDigest, run.Provenance.InputDigest, run.Provenance.OutputDigest,
		usage, run.Provenance.FinishReason, redactions, run.Provenance.TerminationCause,
		run.RetainedOutput, shared.TimestampString(now), shared.TimestampString(run.UpdatedAt),
		shared.TimestampString(run.StartedAt), shared.TimestampString(run.FinishedAt))
	if err != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("insert verification run: %w", err)
	}
	run.CreatedAt = now
	return run, nil
}

// UpdateVerificationRun persists one valid verifier run transition.
func (store *RepositoryStore) UpdateVerificationRun(ctx context.Context, run review.VerificationRunRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if run.Role != review.ModelRoleVerifier || !run.Status.Valid() || strings.TrimSpace(run.ID) == "" || strings.TrimSpace(run.CandidateID) == "" {
		return errors.New("update verification run: run identity or status is invalid")
	}
	parameters, usage, redactions, err := encodeRunFields(run.RunRecord)
	if err != nil {
		return err
	}
	var current, candidateID, sessionID, roundID, snapshotID string
	var adapter, protocol, promptTemplateVersion, model string
	var existingParameters, inputManifestDigest, inputDigest, existingRedactions string
	err = store.database.QueryRowContext(ctx, `
SELECT status, candidate_id, session_id, round_id, snapshot_id, adapter, protocol,
       prompt_template_version, model, parameters_json, input_manifest_digest,
       input_digest, redactions_json
FROM verification_runs WHERE id = ?`, run.ID).Scan(&current, &candidateID, &sessionID, &roundID,
		&snapshotID, &adapter, &protocol, &promptTemplateVersion, &model, &existingParameters,
		&inputManifestDigest, &inputDigest, &existingRedactions)
	if errors.Is(err, sql.ErrNoRows) {
		return fmt.Errorf("%w: %q", ErrVerificationRunNotFound, run.ID)
	}
	if err != nil {
		return fmt.Errorf("read verification run %q: %w", run.ID, err)
	}
	if candidateID != run.CandidateID || sessionID != run.SessionID || roundID != run.RoundID || snapshotID != run.SnapshotID {
		return errors.New("update verification run: immutable candidate provenance changed")
	}
	inputDigestChanged := current != string(review.RunStatusQueued) && inputDigest != run.Provenance.InputDigest
	if adapter != run.Provenance.Adapter || protocol != run.Provenance.Protocol || promptTemplateVersion != run.Provenance.PromptTemplateVersion || model != run.Provenance.Model || existingParameters != parameters || inputManifestDigest != run.Provenance.InputManifestDigest || inputDigestChanged || existingRedactions != redactions {
		return errors.New("update verification run: immutable run provenance changed")
	}
	if !review.RunStatus(current).CanTransitionTo(run.Status) {
		return fmt.Errorf("update verification run: invalid status transition %q -> %q", current, run.Status)
	}
	_, err = store.database.ExecContext(ctx, `
UPDATE verification_runs SET status = ?, attempt = ?, max_attempts = ?, error = ?,
    adapter = ?, protocol = ?, prompt_template_version = ?, model = ?, parameters_json = ?,
    input_manifest_digest = ?, input_digest = ?, output_digest = ?, usage_json = ?,
    finish_reason = ?, redactions_json = ?, termination_cause = ?, retained_output = ?,
    updated_at = ?, started_at = NULLIF(?, ''), finished_at = NULLIF(?, '')
WHERE id = ?`,
		run.Status, run.Attempt, run.MaxAttempts, run.Error, run.Provenance.Adapter,
		run.Provenance.Protocol, run.Provenance.PromptTemplateVersion, run.Provenance.Model,
		parameters, run.Provenance.InputManifestDigest, run.Provenance.InputDigest,
		run.Provenance.OutputDigest, usage, run.Provenance.FinishReason, redactions,
		run.Provenance.TerminationCause, run.RetainedOutput, shared.TimestampString(run.UpdatedAt),
		shared.TimestampString(run.StartedAt), shared.TimestampString(run.FinishedAt), run.ID)
	if err != nil {
		return fmt.Errorf("update verification run %q: %w", run.ID, err)
	}
	return nil
}

// GetVerificationRun returns the authoritative persisted verifier run.
func (store *RepositoryStore) GetVerificationRun(ctx context.Context, runID string) (review.VerificationRunRecord, error) {
	if err := store.validate(); err != nil {
		return review.VerificationRunRecord{}, err
	}
	runID = strings.TrimSpace(runID)
	if runID == "" {
		return review.VerificationRunRecord{}, errors.New("get verification run: run ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	run, err := scanVerificationRun(store.database.QueryRowContext(ctx, verificationRunQuery+` WHERE id = ?`, runID))
	if errors.Is(err, sql.ErrNoRows) {
		return review.VerificationRunRecord{}, fmt.Errorf("%w: %q", ErrVerificationRunNotFound, runID)
	}
	if err != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("get verification run %q: %w", runID, err)
	}
	return run, nil
}

// ListVerificationRuns returns all verifier provenance records for a round in
// stable creation order.
func (store *RepositoryStore) ListVerificationRuns(ctx context.Context, roundID string) ([]review.VerificationRunRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, verificationRunQuery+` WHERE round_id = ? ORDER BY created_at ASC, id ASC`, strings.TrimSpace(roundID))
	if err != nil {
		return nil, fmt.Errorf("list verification runs: %w", err)
	}
	defer rows.Close()
	result := make([]review.VerificationRunRecord, 0)
	for rows.Next() {
		run, scanErr := scanVerificationRun(rows)
		if scanErr != nil {
			return nil, fmt.Errorf("list verification runs: %w", scanErr)
		}
		result = append(result, run)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list verification runs: %w", err)
	}
	return result, nil
}

// SaveVerification inserts one immutable verification record. It stores no
// lane because verified, candidate, and refuted are derived views.
func (store *RepositoryStore) SaveVerification(ctx context.Context, verification review.VerificationRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if strings.TrimSpace(verification.ID) == "" || strings.TrimSpace(verification.CandidateID) == "" || strings.TrimSpace(verification.RunID) == "" {
		return errors.New("save verification: IDs are required")
	}
	if !verification.State.Valid() || verification.State == review.VerificationNotRun {
		return errors.New("save verification: terminal state is required")
	}
	if verification.Digest == "" || verification.Digest != review.VerificationDigest(verification) {
		return errors.New("save verification: digest does not match record")
	}
	var runSession, runRound, runSnapshot, runCandidate, runStatus string
	err := store.database.QueryRowContext(ctx, `
SELECT session_id, round_id, snapshot_id, candidate_id, status
FROM verification_runs WHERE id = ?`, verification.RunID).Scan(&runSession, &runRound, &runSnapshot, &runCandidate, &runStatus)
	if errors.Is(err, sql.ErrNoRows) {
		return fmt.Errorf("save verification: %w: %q", ErrVerificationRunNotFound, verification.RunID)
	}
	if err != nil {
		return fmt.Errorf("save verification: read run: %w", err)
	}
	if runStatus == string(review.RunStatusQueued) || runStatus == string(review.RunStatusRunning) {
		return errors.New("save verification: run is not terminal")
	}
	if runSession != verification.SessionID || runRound != verification.RoundID || runSnapshot != verification.SnapshotID || runCandidate != verification.CandidateID {
		return errors.New("save verification: record provenance does not match run")
	}
	data, err := json.Marshal(verification)
	if err != nil {
		return fmt.Errorf("encode verification: %w", err)
	}
	created := verification.CreatedAt.UTC()
	if created.IsZero() {
		created = store.now().UTC()
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO verifications (
    id, session_id, round_id, snapshot_id, candidate_id, run_id, state,
    digest, verification_json, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, verification.ID, verification.SessionID,
		verification.RoundID, verification.SnapshotID, verification.CandidateID, verification.RunID,
		verification.State, verification.Digest, data, shared.TimestampString(created))
	if err != nil {
		return fmt.Errorf("insert verification %q: %w", verification.ID, err)
	}
	return nil
}

// GetVerification returns one immutable verification record.
func (store *RepositoryStore) GetVerification(ctx context.Context, verificationID string) (review.VerificationRecord, error) {
	if err := store.validate(); err != nil {
		return review.VerificationRecord{}, err
	}
	verificationID = strings.TrimSpace(verificationID)
	if verificationID == "" {
		return review.VerificationRecord{}, errors.New("get verification: ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return scanVerification(store.database.QueryRowContext(ctx, `SELECT verification_json FROM verifications WHERE id = ?`, verificationID), verificationID)
}

// ListVerifications returns immutable verification records for a round in
// creation order.
func (store *RepositoryStore) ListVerifications(ctx context.Context, roundID string) ([]review.VerificationRecord, error) {
	return store.listVerifications(ctx, `round_id = ?`, roundID)
}

// ListCandidateVerifications returns all verification attempts for a candidate.
func (store *RepositoryStore) ListCandidateVerifications(ctx context.Context, candidateID string) ([]review.VerificationRecord, error) {
	return store.listVerifications(ctx, `candidate_id = ?`, candidateID)
}

// GetLatestVerification returns the newest immutable attempt for a candidate.
func (store *RepositoryStore) GetLatestVerification(ctx context.Context, candidateID string) (review.VerificationRecord, error) {
	if err := store.validate(); err != nil {
		return review.VerificationRecord{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var data string
	err := store.database.QueryRowContext(ctx, `
SELECT verification_json FROM verifications
WHERE candidate_id = ? ORDER BY created_at DESC, id DESC LIMIT 1`, candidateID).Scan(&data)
	if errors.Is(err, sql.ErrNoRows) {
		return review.VerificationRecord{}, fmt.Errorf("%w for candidate: %q", ErrVerificationNotFound, candidateID)
	}
	if err != nil {
		return review.VerificationRecord{}, fmt.Errorf("get latest verification: %w", err)
	}
	return decodeVerificationData(data, "latest verification")
}

func (store *RepositoryStore) listVerifications(ctx context.Context, predicate, value string) ([]review.VerificationRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `SELECT verification_json FROM verifications WHERE `+predicate+` ORDER BY created_at ASC, id ASC`, value)
	if err != nil {
		return nil, fmt.Errorf("list verifications: %w", err)
	}
	defer rows.Close()
	verifications := make([]review.VerificationRecord, 0)
	for rows.Next() {
		var data string
		if err := rows.Scan(&data); err != nil {
			return nil, fmt.Errorf("list verifications: %w", err)
		}
		verification, err := decodeVerificationData(data, "verification")
		if err != nil {
			return nil, err
		}
		verifications = append(verifications, verification)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list verifications: %w", err)
	}
	return verifications, nil
}

const verificationRunQuery = `
SELECT id, session_id, round_id, snapshot_id, candidate_id, status, attempt,
       max_attempts, error, adapter, protocol, prompt_template_version, model,
       parameters_json, input_manifest_digest, input_digest, output_digest,
       usage_json, finish_reason, redactions_json, termination_cause,
       retained_output, created_at, updated_at, started_at, finished_at
FROM verification_runs`

func scanVerificationRun(row scanner) (review.VerificationRunRecord, error) {
	var run review.VerificationRunRecord
	var status, parameters, usage, redactions string
	var created, updated, started, finished string
	err := row.Scan(&run.ID, &run.SessionID, &run.RoundID, &run.SnapshotID, &run.CandidateID,
		&status, &run.Attempt, &run.MaxAttempts, &run.Error, &run.Provenance.Adapter,
		&run.Provenance.Protocol, &run.Provenance.PromptTemplateVersion, &run.Provenance.Model,
		&parameters, &run.Provenance.InputManifestDigest, &run.Provenance.InputDigest,
		&run.Provenance.OutputDigest, &usage, &run.Provenance.FinishReason,
		&redactions, &run.Provenance.TerminationCause, &run.RetainedOutput, &created,
		&updated, &started, &finished)
	if err != nil {
		return review.VerificationRunRecord{}, err
	}
	run.Role = review.ModelRoleVerifier
	run.PassName = "candidate-verification"
	run.Status = review.RunStatus(status)
	if err := json.Unmarshal([]byte(parameters), &run.Provenance.Parameters); err != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("decode verification parameters: %w", err)
	}
	if err := json.Unmarshal([]byte(usage), &run.Provenance.Usage); err != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("decode verification usage: %w", err)
	}
	if err := json.Unmarshal([]byte(redactions), &run.Provenance.Redactions); err != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("decode verification redactions: %w", err)
	}
	var parseErr error
	if run.CreatedAt, parseErr = time.Parse(time.RFC3339Nano, created); parseErr != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("parse verification created time: %w", parseErr)
	}
	if run.UpdatedAt, parseErr = time.Parse(time.RFC3339Nano, updated); parseErr != nil {
		return review.VerificationRunRecord{}, fmt.Errorf("parse verification updated time: %w", parseErr)
	}
	if started != "" {
		if run.StartedAt, parseErr = time.Parse(time.RFC3339Nano, started); parseErr != nil {
			return review.VerificationRunRecord{}, fmt.Errorf("parse verification started time: %w", parseErr)
		}
	}
	if finished != "" {
		if run.FinishedAt, parseErr = time.Parse(time.RFC3339Nano, finished); parseErr != nil {
			return review.VerificationRunRecord{}, fmt.Errorf("parse verification finished time: %w", parseErr)
		}
	}
	return run, nil
}

func scanVerification(row scanner, verificationID string) (review.VerificationRecord, error) {
	var data string
	if err := row.Scan(&data); errors.Is(err, sql.ErrNoRows) {
		return review.VerificationRecord{}, fmt.Errorf("%w: %q", ErrVerificationNotFound, verificationID)
	} else if err != nil {
		return review.VerificationRecord{}, fmt.Errorf("get verification %q: %w", verificationID, err)
	}
	return decodeVerificationData(data, fmt.Sprintf("verification %q", verificationID))
}

func decodeVerificationData(data, label string) (review.VerificationRecord, error) {
	var verification review.VerificationRecord
	if err := json.Unmarshal([]byte(data), &verification); err != nil {
		return review.VerificationRecord{}, fmt.Errorf("decode %s: %w", label, err)
	}
	if verification.Digest != review.VerificationDigest(verification) {
		return review.VerificationRecord{}, fmt.Errorf("%s: digest mismatch", label)
	}
	return verification, nil
}
