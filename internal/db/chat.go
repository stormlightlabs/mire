package db

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

var _ review.ChatStore = (*RepositoryStore)(nil)

var (
	// ErrChatMessageNotFound indicates that a chat message is absent.
	ErrChatMessageNotFound = errors.New("chat message not found")
	// ErrChatRunNotFound indicates that a chat run is absent.
	ErrChatRunNotFound = errors.New("chat run not found")
	// ErrChatAnchorNotFound indicates that a diff hunk was not registered for
	// the active immutable round.
	ErrChatAnchorNotFound = errors.New("chat diff anchor not found")
)

// RegisterDiffAnchors records the exact hunk inventory produced from a frozen
// change model. Later chat requests must match one of these immutable anchors.
func (store *RepositoryStore) RegisterDiffAnchors(ctx context.Context, roundID, snapshotID string, anchors []review.Anchor) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	round, err := store.GetRound(ctx, roundID)
	if err != nil {
		return fmt.Errorf("register diff anchors: %w", err)
	}
	snapshotID = strings.TrimSpace(snapshotID)
	if snapshotID == "" {
		snapshotID = round.SnapshotID
	}
	if round.SnapshotID != snapshotID || snapshotID == "" {
		return errors.New("register diff anchors: round and snapshot do not match")
	}
	frozen, err := store.GetSnapshot(ctx, snapshotID)
	if err != nil {
		return fmt.Errorf("register diff anchors: %w", err)
	}
	now := store.now().UTC()
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin register diff anchors transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	seen := make(map[string]struct{}, len(anchors))
	for index, anchor := range anchors {
		anchor, err = normalizeRegisteredAnchor(anchor, snapshotID)
		if err != nil {
			return fmt.Errorf("register diff anchor %d: %w", index, err)
		}
		if err := validateSnapshotAnchorTx(ctx, tx, frozen, anchor); err != nil {
			return fmt.Errorf("register diff anchor %q: %w", anchor.HunkID, err)
		}
		key := anchorKey(anchor)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		data, err := json.Marshal(anchor)
		if err != nil {
			return fmt.Errorf("encode diff anchor %q: %w", anchor.HunkID, err)
		}
		digest := digestJSON(data)
		var existing string
		queryErr := tx.QueryRowContext(ctx, `
SELECT anchor_json FROM round_diff_anchors
WHERE round_id = ? AND side = ? AND path = ? AND hunk_id = ?`, roundID, anchor.Side, anchor.Path, anchor.HunkID).Scan(&existing)
		switch {
		case errors.Is(queryErr, sql.ErrNoRows):
			_, err = tx.ExecContext(ctx, `
INSERT INTO round_diff_anchors (round_id, snapshot_id, side, path, hunk_id, digest, anchor_json, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)`, roundID, snapshotID, anchor.Side, anchor.Path, anchor.HunkID, digest, data, timestampString(now))
			if err != nil {
				return fmt.Errorf("insert diff anchor %q: %w", anchor.HunkID, err)
			}
		case queryErr != nil:
			return fmt.Errorf("read diff anchor %q: %w", anchor.HunkID, queryErr)
		case existing != string(data):
			return fmt.Errorf("diff anchor %q is already registered with different identity", anchor.HunkID)
		}
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit diff anchors: %w", err)
	}
	return nil
}

// RegisterChangeModelAnchors registers every hunk from a canonical change
// model, making the model's exact diff selections available to chat.
func (store *RepositoryStore) RegisterChangeModelAnchors(ctx context.Context, change review.ChangeModel, roundID string) error {
	anchors := make([]review.Anchor, 0)
	for _, file := range change.Files {
		for _, hunk := range file.Hunks {
			if file.TargetPath != "" {
				anchors = append(anchors, review.Anchor{SnapshotID: change.SnapshotID, Side: snapshot.TreeSideTarget,
					Layer: snapshot.TreeSideTarget, Path: file.TargetPath, BlobDigest: file.TargetDigest,
					HunkID: hunk.ID, HunkDigest: hunk.Digest, ContextDigest: contextDigestFromHunk(hunk)})
			}
			if file.BasePath != "" && file.BasePath != file.TargetPath {
				anchors = append(anchors, review.Anchor{SnapshotID: change.SnapshotID, Side: snapshot.TreeSideBase,
					Layer: snapshot.TreeSideBase, Path: file.BasePath, BlobDigest: file.BaseDigest,
					HunkID: hunk.ID, HunkDigest: hunk.Digest, ContextDigest: contextDigestFromHunk(hunk)})
			}
		}
	}
	return store.RegisterDiffAnchors(ctx, roundID, change.SnapshotID, anchors)
}

// ListDiffAnchors returns the registered exact hunk inventory for a round.
func (store *RepositoryStore) ListDiffAnchors(ctx context.Context, roundID string) ([]review.Anchor, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT anchor_json FROM round_diff_anchors WHERE round_id = ?
ORDER BY side ASC, path ASC, hunk_id ASC`, strings.TrimSpace(roundID))
	if err != nil {
		return nil, fmt.Errorf("list diff anchors: %w", err)
	}
	defer rows.Close()
	anchors := make([]review.Anchor, 0)
	for rows.Next() {
		var data string
		if err := rows.Scan(&data); err != nil {
			return nil, fmt.Errorf("list diff anchors: %w", err)
		}
		var anchor review.Anchor
		if err := json.Unmarshal([]byte(data), &anchor); err != nil {
			return nil, fmt.Errorf("decode diff anchor: %w", err)
		}
		if err := review.ValidateDiffAnchor(anchor); err != nil {
			return nil, fmt.Errorf("validate diff anchor: %w", err)
		}
		anchors = append(anchors, anchor)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list diff anchors: %w", err)
	}
	return anchors, nil
}

// ValidateChatBinding resolves a user binding against the session's current
// round, finding revisions, and registered immutable diff anchors.
func (store *RepositoryStore) ValidateChatBinding(ctx context.Context, binding review.ChatBinding) (review.ChatBinding, error) {
	if err := store.validate(); err != nil {
		return review.ChatBinding{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	binding.SessionID = strings.TrimSpace(binding.SessionID)
	binding.RoundID = strings.TrimSpace(binding.RoundID)
	binding.SnapshotID = strings.TrimSpace(binding.SnapshotID)
	binding.Digest = ""
	if binding.SessionID == "" || binding.RoundID == "" {
		return review.ChatBinding{}, errors.New("validate chat binding: session and round are required")
	}
	session, err := store.GetSession(ctx, binding.SessionID)
	if err != nil {
		return review.ChatBinding{}, fmt.Errorf("validate chat binding: %w", err)
	}
	if session.CurrentRoundID != binding.RoundID {
		return review.ChatBinding{}, errors.New("validate chat binding: context must belong to the session's active round")
	}
	round, err := store.GetRound(ctx, binding.RoundID)
	if err != nil {
		return review.ChatBinding{}, fmt.Errorf("validate chat binding: %w", err)
	}
	if round.SessionID != binding.SessionID || round.SnapshotID == "" {
		return review.ChatBinding{}, errors.New("validate chat binding: round provenance does not match session")
	}
	if binding.SnapshotID == "" {
		binding.SnapshotID = round.SnapshotID
	}
	if binding.SnapshotID != round.SnapshotID {
		return review.ChatBinding{}, errors.New("validate chat binding: snapshot does not match active round")
	}
	frozen, err := store.GetSnapshot(ctx, binding.SnapshotID)
	if err != nil {
		return review.ChatBinding{}, fmt.Errorf("validate chat binding: %w", err)
	}
	binding.SnapshotDigest = frozen.ManifestDigest
	canonical, err := review.NormalizeChatBinding(binding)
	if err != nil {
		return review.ChatBinding{}, fmt.Errorf("validate chat binding: %w", err)
	}
	if err := store.validateChatReferences(ctx, canonical); err != nil {
		return review.ChatBinding{}, err
	}
	return canonical, nil
}

func (store *RepositoryStore) validateStoredChatBinding(ctx context.Context, binding review.ChatBinding) (review.ChatBinding, error) {
	binding.Digest = ""
	canonical, err := review.NormalizeChatBinding(binding)
	if err != nil {
		return review.ChatBinding{}, err
	}
	round, err := store.GetRound(ctx, canonical.RoundID)
	if err != nil {
		return review.ChatBinding{}, err
	}
	if round.SessionID != canonical.SessionID || round.SnapshotID != canonical.SnapshotID {
		return review.ChatBinding{}, errors.New("stored chat binding does not match round")
	}
	frozen, err := store.GetSnapshot(ctx, canonical.SnapshotID)
	if err != nil {
		return review.ChatBinding{}, err
	}
	if canonical.SnapshotDigest != "" && canonical.SnapshotDigest != frozen.ManifestDigest {
		return review.ChatBinding{}, errors.New("stored chat binding snapshot digest does not match snapshot")
	}
	canonical.SnapshotDigest = frozen.ManifestDigest
	canonical.Digest = review.ChatBindingDigest(canonical)
	if err := store.validateChatReferences(ctx, canonical); err != nil {
		return review.ChatBinding{}, err
	}
	return canonical, nil
}

func (store *RepositoryStore) validateChatReferences(ctx context.Context, binding review.ChatBinding) error {
	for _, reference := range binding.Context.References {
		switch reference.Kind {
		case review.ChatReferenceFindingRevision:
			finding, err := store.GetFindingRevision(ctx, reference.FindingRevision.FindingID, reference.FindingRevision.Revision)
			if err != nil {
				return fmt.Errorf("validate chat finding context: %w", err)
			}
			if finding.SessionID != binding.SessionID || finding.RoundID != binding.RoundID || finding.SnapshotID != binding.SnapshotID {
				return errors.New("chat finding reference does not belong to the active round and snapshot")
			}
		case review.ChatReferenceDiffAnchor:
			if err := store.validateRegisteredAnchor(ctx, binding.RoundID, binding.SnapshotID, *reference.DiffAnchor); err != nil {
				return err
			}
		default:
			return fmt.Errorf("validate chat context: reference kind %q is unsupported", reference.Kind)
		}
	}
	return nil
}

func (store *RepositoryStore) validateRegisteredAnchor(ctx context.Context, roundID, snapshotID string, anchor review.Anchor) error {
	data, err := json.Marshal(anchor)
	if err != nil {
		return fmt.Errorf("validate chat diff anchor: encode: %w", err)
	}
	var expected string
	err = store.database.QueryRowContext(ctx, `
SELECT anchor_json FROM round_diff_anchors
WHERE round_id = ? AND snapshot_id = ? AND side = ? AND path = ? AND hunk_id = ?`,
		roundID, snapshotID, anchor.Side, anchor.Path, anchor.HunkID).Scan(&expected)
	if errors.Is(err, sql.ErrNoRows) {
		return fmt.Errorf("%w: %s", ErrChatAnchorNotFound, anchor.HunkID)
	}
	if err != nil {
		return fmt.Errorf("validate chat diff anchor: %w", err)
	}
	if expected != string(data) {
		return errors.New("validate chat diff anchor: identity does not match registered snapshot hunk")
	}
	return nil
}

// SaveChatMessage appends one immutable chat message after validating its
// stored round binding. It never rewrites an existing message ID.
func (store *RepositoryStore) SaveChatMessage(ctx context.Context, message review.ChatMessage) (review.ChatMessage, error) {
	if err := store.validate(); err != nil {
		return review.ChatMessage{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if message.ID == "" {
		message.ID, _ = store.newID()
	}
	if message.SchemaVersion == "" {
		message.SchemaVersion = review.ChatSchemaVersion
	}
	if message.CreatedAt.IsZero() {
		message.CreatedAt = store.now().UTC()
	}
	canonicalBinding, err := store.validateStoredChatBinding(ctx, review.ChatBinding{SessionID: message.SessionID, RoundID: message.RoundID, SnapshotID: message.SnapshotID, Context: message.Context})
	if err != nil {
		return review.ChatMessage{}, fmt.Errorf("save chat message: %w", err)
	}
	message.Context = canonicalBinding.Context
	message.Digest = review.ChatMessageDigest(message)
	if err := review.ValidateChatMessage(message); err != nil {
		return review.ChatMessage{}, fmt.Errorf("save chat message: %w", err)
	}
	data, err := json.Marshal(message)
	if err != nil {
		return review.ChatMessage{}, fmt.Errorf("encode chat message: %w", err)
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO chat_messages (id, session_id, round_id, snapshot_id, role, digest, message_json, producer_run_id, reply_to, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, message.ID, message.SessionID, message.RoundID, message.SnapshotID,
		message.Role, message.Digest, data, message.ProducerRunID, message.ReplyTo, timestampString(message.CreatedAt.UTC()))
	if err != nil {
		return review.ChatMessage{}, fmt.Errorf("insert chat message %q: %w", message.ID, err)
	}
	return message, nil
}

// ListChatMessages returns the single session timeline in immutable creation
// order. It includes only persisted user and assistant messages.
func (store *RepositoryStore) ListChatMessages(ctx context.Context, sessionID string) ([]review.ChatMessage, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT message_json, digest FROM chat_messages
WHERE session_id = ? ORDER BY created_at ASC, rowid ASC`, strings.TrimSpace(sessionID))
	if err != nil {
		return nil, fmt.Errorf("list chat messages: %w", err)
	}
	defer rows.Close()
	messages := make([]review.ChatMessage, 0)
	for rows.Next() {
		var data, expected string
		if err := rows.Scan(&data, &expected); err != nil {
			return nil, fmt.Errorf("list chat messages: %w", err)
		}
		message, err := decodeChatMessage(data, expected)
		if err != nil {
			return nil, err
		}
		messages = append(messages, message)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list chat messages: %w", err)
	}
	return messages, nil
}

// GetChatMessage returns one immutable message by ID.
func (store *RepositoryStore) GetChatMessage(ctx context.Context, messageID string) (review.ChatMessage, error) {
	if err := store.validate(); err != nil {
		return review.ChatMessage{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var data, expected string
	err := store.database.QueryRowContext(ctx, `SELECT message_json, digest FROM chat_messages WHERE id = ?`, strings.TrimSpace(messageID)).Scan(&data, &expected)
	if errors.Is(err, sql.ErrNoRows) {
		return review.ChatMessage{}, fmt.Errorf("%w: %q", ErrChatMessageNotFound, messageID)
	}
	if err != nil {
		return review.ChatMessage{}, fmt.Errorf("get chat message: %w", err)
	}
	return decodeChatMessage(data, expected)
}

// CreateChatRun persists a queued run with immutable binding and input.
func (store *RepositoryStore) CreateChatRun(ctx context.Context, record review.ChatRunRecord) (review.ChatRunRecord, error) {
	if err := store.validate(); err != nil {
		return review.ChatRunRecord{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if record.Run.CreatedAt.IsZero() {
		record.Run.CreatedAt = store.now().UTC()
	}
	if record.Run.UpdatedAt.IsZero() {
		record.Run.UpdatedAt = record.Run.CreatedAt
	}
	if err := review.ValidateChatRunRecord(record); err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("create chat run: %w", err)
	}
	if _, err := store.GetSession(ctx, record.Run.SessionID); err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("create chat run: %w", err)
	}
	round, err := store.GetRound(ctx, record.Run.RoundID)
	if err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("create chat run: %w", err)
	}
	if round.SessionID != record.Run.SessionID || round.SnapshotID != record.Run.SnapshotID {
		return review.ChatRunRecord{}, errors.New("create chat run: run provenance does not match round")
	}
	message, err := store.GetChatMessage(ctx, record.UserMessageID)
	if err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("create chat run: %w", err)
	}
	if message.Role != review.MessageRoleUser || message.SessionID != record.Run.SessionID || message.RoundID != record.Run.RoundID || message.SnapshotID != record.Run.SnapshotID || !sameJSON(message.Context.Primary, record.Binding.Context.Primary) {
		return review.ChatRunRecord{}, errors.New("create chat run: user message does not match run binding")
	}
	runJSON, inputJSON, bindingJSON, responseJSON, err := encodeChatRun(record)
	if err != nil {
		return review.ChatRunRecord{}, err
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO chat_runs (id, session_id, round_id, snapshot_id, user_message_id, status, run_json, binding_json, input_json, response_json, retained_output, created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, record.Run.ID, record.Run.SessionID, record.Run.RoundID, record.Run.SnapshotID,
		record.UserMessageID, record.Run.Status, runJSON, bindingJSON, inputJSON, responseJSON, record.RetainedOutput,
		timestampString(record.Run.CreatedAt.UTC()), timestampString(record.Run.UpdatedAt.UTC()))
	if err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("insert chat run %q: %w", record.Run.ID, err)
	}
	return record, nil
}

// UpdateChatRun changes only the durable lifecycle/output projection. Binding,
// input, and user-message identity are immutable after creation.
func (store *RepositoryStore) UpdateChatRun(ctx context.Context, record review.ChatRunRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	existing, err := store.GetChatRun(ctx, record.Run.ID)
	if err != nil {
		return err
	}
	if existing.UserMessageID != record.UserMessageID || !sameJSON(existing.Binding, record.Binding) || !sameJSON(existing.Input, record.Input) || existing.Run.SessionID != record.Run.SessionID || existing.Run.RoundID != record.Run.RoundID || existing.Run.SnapshotID != record.Run.SnapshotID {
		return errors.New("update chat run: immutable binding or input changed")
	}
	if existing.Run.Status != record.Run.Status {
		if !existing.Run.Status.CanTransitionTo(record.Run.Status) {
			return fmt.Errorf("update chat run: invalid status transition %s -> %s", existing.Run.Status, record.Run.Status)
		}
	} else if record.Run.Status != review.RunStatusRunning {
		return errors.New("update chat run: terminal status cannot be rewritten")
	}
	if err := review.ValidateChatRunRecord(record); err != nil {
		return fmt.Errorf("update chat run: %w", err)
	}
	runJSON, _, _, responseJSON, err := encodeChatRun(record)
	if err != nil {
		return err
	}
	result, err := store.database.ExecContext(ctx, `
UPDATE chat_runs SET status = ?, run_json = ?, response_json = ?, retained_output = ?, updated_at = ?
WHERE id = ?`, record.Run.Status, runJSON, responseJSON, record.RetainedOutput,
		timestampString(record.Run.UpdatedAt.UTC()), record.Run.ID)
	if err != nil {
		return fmt.Errorf("update chat run %q: %w", record.Run.ID, err)
	}
	updated, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check chat run %q update: %w", record.Run.ID, err)
	}
	if updated != 1 {
		return fmt.Errorf("%w: %q", ErrChatRunNotFound, record.Run.ID)
	}
	return nil
}

// GetChatRun returns the authoritative persisted run and immutable input.
func (store *RepositoryStore) GetChatRun(ctx context.Context, runID string) (review.ChatRunRecord, error) {
	if err := store.validate(); err != nil {
		return review.ChatRunRecord{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var userMessageID, runJSON, bindingJSON, inputJSON, responseJSON, retained string
	err := store.database.QueryRowContext(ctx, `
SELECT user_message_id, run_json, binding_json, input_json, response_json, retained_output
FROM chat_runs WHERE id = ?`, strings.TrimSpace(runID)).Scan(&userMessageID, &runJSON, &bindingJSON, &inputJSON, &responseJSON, &retained)
	if errors.Is(err, sql.ErrNoRows) {
		return review.ChatRunRecord{}, fmt.Errorf("%w: %q", ErrChatRunNotFound, runID)
	}
	if err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("get chat run: %w", err)
	}
	var record review.ChatRunRecord
	record.UserMessageID = userMessageID
	if err := json.Unmarshal([]byte(runJSON), &record.Run); err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("decode chat run: %w", err)
	}
	if err := json.Unmarshal([]byte(bindingJSON), &record.Binding); err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("decode chat binding: %w", err)
	}
	if err := json.Unmarshal([]byte(inputJSON), &record.Input); err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("decode chat input: %w", err)
	}
	if responseJSON != "" {
		var response review.ChatResponse
		if err := json.Unmarshal([]byte(responseJSON), &response); err != nil {
			return review.ChatRunRecord{}, fmt.Errorf("decode chat response: %w", err)
		}
		record.Response = &response
	}
	record.RetainedOutput = retained
	if err := review.ValidateChatRunRecord(record); err != nil {
		return review.ChatRunRecord{}, fmt.Errorf("validate chat run: %w", err)
	}
	return record, nil
}

// ListChatRuns returns all model attempts in durable creation order.
func (store *RepositoryStore) ListChatRuns(ctx context.Context, sessionID string) ([]review.ChatRunRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
	SELECT id FROM chat_runs WHERE session_id = ? ORDER BY created_at ASC, rowid ASC`, strings.TrimSpace(sessionID))
	if err != nil {
		return nil, fmt.Errorf("list chat runs: %w", err)
	}
	defer rows.Close()
	result := make([]review.ChatRunRecord, 0)
	for rows.Next() {
		var id string
		if err := rows.Scan(&id); err != nil {
			return nil, fmt.Errorf("list chat runs: %w", err)
		}
		run, err := store.GetChatRun(ctx, id)
		if err != nil {
			return nil, err
		}
		result = append(result, run)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list chat runs: %w", err)
	}
	return result, nil
}

// GetChatTimeline returns durable messages and model runs after restart.
func (store *RepositoryStore) GetChatTimeline(ctx context.Context, sessionID string) (review.ChatTimeline, error) {
	messages, err := store.ListChatMessages(ctx, sessionID)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	runs, err := store.ListChatRuns(ctx, sessionID)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	return review.ChatTimeline{Messages: messages, Runs: runs}, nil
}

// SendChatTurn validates the active round, serializes chat work through the
// session operation lease, and delegates model execution to the domain runner.
func (store *RepositoryStore) SendChatTurn(ctx context.Context, request review.ChatTurnRequest, model review.Model, options review.ChatOptions) (review.ChatTurnResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	session, err := store.GetSession(ctx, request.SessionID)
	if err != nil {
		return review.ChatTurnResult{}, err
	}
	if request.RoundID == "" {
		request.RoundID = session.CurrentRoundID
	}
	if request.RoundID != session.CurrentRoundID {
		return review.ChatTurnResult{}, errors.New("send chat turn: request must target the session's active round")
	}
	round, err := store.GetRound(ctx, request.RoundID)
	if err != nil {
		return review.ChatTurnResult{}, err
	}
	if request.SnapshotID == "" {
		request.SnapshotID = round.SnapshotID
	}
	binding, err := store.ValidateChatBinding(ctx, review.ChatBinding{SessionID: session.ID, RoundID: round.ID, SnapshotID: request.SnapshotID, Context: request.Context})
	if err != nil {
		return review.ChatTurnResult{}, err
	}
	request.SnapshotID = binding.SnapshotID
	operation, err := store.CreateOperation(ctx, session.ID, "", OperationKindChat)
	if err != nil {
		return review.ChatTurnResult{}, err
	}
	if _, err := store.AcquireOperation(ctx, operation.ID); err != nil {
		_, _ = store.CancelOperation(context.WithoutCancel(ctx), operation.ID)
		return review.ChatTurnResult{}, err
	}
	options.Store = store
	result, runErr := review.RunChat(ctx, request, model, options)
	persistCtx := context.WithoutCancel(ctx)
	if runErr != nil {
		var chatErr *review.ChatError
		if errors.As(runErr, &chatErr) && chatErr.Status == review.RunStatusCancelled {
			_, _ = store.CancelOperation(persistCtx, operation.ID)
		} else {
			_, _ = store.FailOperation(persistCtx, operation.ID, runErr.Error())
		}
		return result, runErr
	}
	if _, err := store.CompleteOperation(persistCtx, operation.ID); err != nil {
		return result, fmt.Errorf("send chat turn: complete operation: %w", err)
	}
	return result, nil
}

func decodeChatMessage(data, expected string) (review.ChatMessage, error) {
	var message review.ChatMessage
	if err := json.Unmarshal([]byte(data), &message); err != nil {
		return review.ChatMessage{}, fmt.Errorf("decode chat message: %w", err)
	}
	if message.Digest != expected || message.Digest != review.ChatMessageDigest(message) {
		return review.ChatMessage{}, errors.New("chat message digest mismatch")
	}
	if err := review.ValidateChatMessage(message); err != nil {
		return review.ChatMessage{}, fmt.Errorf("validate chat message: %w", err)
	}
	return message, nil
}

func encodeChatRun(record review.ChatRunRecord) (string, string, string, string, error) {
	runJSON, err := json.Marshal(record.Run)
	if err != nil {
		return "", "", "", "", fmt.Errorf("encode chat run: %w", err)
	}
	inputJSON, err := json.Marshal(record.Input)
	if err != nil {
		return "", "", "", "", fmt.Errorf("encode chat input: %w", err)
	}
	bindingJSON, err := json.Marshal(record.Binding)
	if err != nil {
		return "", "", "", "", fmt.Errorf("encode chat binding: %w", err)
	}
	responseJSON := ""
	if record.Response != nil {
		data, err := json.Marshal(record.Response)
		if err != nil {
			return "", "", "", "", fmt.Errorf("encode chat response: %w", err)
		}
		responseJSON = string(data)
	}
	return string(runJSON), string(inputJSON), string(bindingJSON), responseJSON, nil
}

func sameJSON(left, right any) bool {
	leftData, leftErr := json.Marshal(left)
	rightData, rightErr := json.Marshal(right)
	return leftErr == nil && rightErr == nil && string(leftData) == string(rightData)
}

func normalizeRegisteredAnchor(anchor review.Anchor, snapshotID string) (review.Anchor, error) {
	anchor.SnapshotID = strings.TrimSpace(anchor.SnapshotID)
	anchor.Side = strings.TrimSpace(anchor.Side)
	anchor.Layer = strings.TrimSpace(anchor.Layer)
	anchor.Path = strings.TrimSpace(anchor.Path)
	anchor.BlobDigest = strings.TrimSpace(anchor.BlobDigest)
	anchor.HunkID = strings.TrimSpace(anchor.HunkID)
	anchor.HunkDigest = strings.TrimSpace(anchor.HunkDigest)
	anchor.ContextDigest = strings.TrimSpace(anchor.ContextDigest)
	anchor.OriginalHunk = strings.TrimSpace(anchor.OriginalHunk)
	if anchor.SnapshotID == "" {
		anchor.SnapshotID = snapshotID
	}
	if anchor.Side == "" {
		anchor.Side = snapshot.TreeSideTarget
	}
	if anchor.Layer == "" {
		anchor.Layer = anchor.Side
	}
	if err := review.ValidateDiffAnchor(anchor); err != nil {
		return review.Anchor{}, err
	}
	if anchor.SnapshotID != snapshotID {
		return review.Anchor{}, errors.New("anchor belongs to another snapshot")
	}
	return anchor, nil
}

func validateSnapshotAnchorTx(ctx context.Context, tx *sql.Tx, frozen Snapshot, anchor review.Anchor) error {
	if frozen.Kind == snapshot.ComparisonWorktree {
		switch anchor.Side {
		case snapshot.TreeSideHead, snapshot.TreeSideIndex, snapshot.TreeSideWorktree:
		default:
			return fmt.Errorf("side %q is not available in a working-tree snapshot", anchor.Side)
		}
	} else if anchor.Side != snapshot.TreeSideBase && anchor.Side != snapshot.TreeSideTarget {
		return fmt.Errorf("side %q is not available in a committed snapshot", anchor.Side)
	}
	column := "target_path"
	treeSide := anchor.Side
	if anchor.Side == snapshot.TreeSideBase || anchor.Side == snapshot.TreeSideHead {
		column = "base_path"
	}
	if anchor.Side == snapshot.TreeSideHead || anchor.Side == snapshot.TreeSideIndex || anchor.Side == snapshot.TreeSideWorktree {
		treeSide = anchor.Side
	}
	var changed int
	if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM snapshot_changes WHERE snapshot_id = ? AND `+column+` = ? AND status <> ?`, frozen.ID, anchor.Path, snapshot.ChangeUnchanged).Scan(&changed); err != nil {
		return fmt.Errorf("check changed path: %w", err)
	}
	if changed != 1 {
		return errors.New("diff anchor path is not an exact changed path")
	}
	var storedBlob string
	err := tx.QueryRowContext(ctx, `SELECT content_digest FROM snapshot_entries WHERE snapshot_id = ? AND tree_side = ? AND path = ?`, frozen.ID, treeSide, anchor.Path).Scan(&storedBlob)
	if errors.Is(err, sql.ErrNoRows) {
		return errors.New("diff anchor path is not present in the selected snapshot side")
	}
	if err != nil {
		return fmt.Errorf("read selected snapshot entry: %w", err)
	}
	if anchor.BlobDigest != "" && storedBlob != "" && anchor.BlobDigest != storedBlob {
		return errors.New("diff anchor blob digest does not match snapshot")
	}
	return nil
}

func anchorKey(anchor review.Anchor) string {
	return anchor.Side + "\x00" + anchor.Path + "\x00" + anchor.HunkID
}

func digestJSON(data []byte) string {
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

func contextDigestFromHunk(hunk review.Hunk) string {
	var context strings.Builder
	for _, line := range hunk.Lines {
		if strings.HasPrefix(line, " ") {
			context.WriteString(line)
		}
	}
	if context.Len() == 0 {
		return hunk.Digest
	}
	return digestJSON([]byte(context.String()))
}

// Keep time imported in builds that replace the SQLite driver with a test
// implementation.
var _ = time.RFC3339Nano
