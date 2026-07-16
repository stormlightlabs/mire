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

var _ review.PlanStore = (*RepositoryStore)(nil)

// CreatePlanRun persists a queued provider-neutral planner run. The session
// must already exist so a run cannot outlive its review session.
func (store *RepositoryStore) CreatePlanRun(ctx context.Context, run review.RunRecord) (review.RunRecord, error) {
	if err := store.validate(); err != nil {
		return review.RunRecord{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if strings.TrimSpace(run.SessionID) == "" {
		return review.RunRecord{}, errors.New("create planner run: session ID is empty")
	}
	if run.Role != review.ModelRolePlanner || run.Status != review.RunStatusQueued {
		return review.RunRecord{}, errors.New("create planner run: run must be a queued planner run")
	}
	if run.MaxAttempts < 1 {
		return review.RunRecord{}, errors.New("create planner run: max attempts must be positive")
	}
	if _, err := store.GetSession(ctx, run.SessionID); err != nil {
		return review.RunRecord{}, err
	}
	if run.RoundID != "" {
		round, err := store.GetRound(ctx, run.RoundID)
		if err != nil {
			return review.RunRecord{}, err
		}
		if round.SessionID != run.SessionID {
			return review.RunRecord{}, fmt.Errorf("create planner run: round belongs to another session")
		}
	}
	var err error
	if run.ID == "" {
		run.ID, err = store.newID()
		if err != nil {
			return review.RunRecord{}, fmt.Errorf("create planner run ID: %w", err)
		}
	}
	now := run.CreatedAt.UTC()
	if now.IsZero() {
		now = store.now().UTC()
	}
	if run.UpdatedAt.IsZero() {
		run.UpdatedAt = now
	}
	parameters, usage, redactions, err := encodeRunFields(run)
	if err != nil {
		return review.RunRecord{}, err
	}
	_, err = store.database.ExecContext(
		ctx,
		`
INSERT INTO planner_runs (
    id, session_id, round_id, snapshot_id, role, status, attempt, max_attempts,
    error, adapter, protocol, prompt_template_version, model, parameters_json,
    input_manifest_digest, input_digest, output_digest, usage_json, finish_reason,
    redactions_json, termination_cause, created_at, updated_at, started_at, finished_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULLIF(?, ''), NULLIF(?, ''))`,
		run.ID,
		run.SessionID,
		run.RoundID,
		run.SnapshotID,
		run.Role,
		run.Status,
		run.Attempt,
		run.MaxAttempts,
		run.Error,
		run.Provenance.Adapter,
		run.Provenance.Protocol,
		run.Provenance.PromptTemplateVersion,
		run.Provenance.Model,
		parameters,
		run.Provenance.InputManifestDigest,
		run.Provenance.InputDigest,
		run.Provenance.OutputDigest,
		usage,
		run.Provenance.FinishReason,
		redactions,
		run.Provenance.TerminationCause,
		shared.TimestampString(
			now,
		),
		shared.TimestampString(run.UpdatedAt),
		shared.TimestampString(run.StartedAt),
		shared.TimestampString(run.FinishedAt),
	)
	if err != nil {
		return review.RunRecord{}, fmt.Errorf("insert planner run: %w", err)
	}
	run.CreatedAt = now
	return run, nil
}

// UpdatePlanRun persists the current terminal or in-progress run projection.
func (store *RepositoryStore) UpdatePlanRun(ctx context.Context, run review.RunRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if !run.Status.Valid() {
		return fmt.Errorf("update planner run: invalid status %q", run.Status)
	}
	if strings.TrimSpace(run.ID) == "" {
		return errors.New("update planner run: run ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	parameters, usage, redactions, err := encodeRunFields(run)
	if err != nil {
		return err
	}
	result, err := store.database.ExecContext(ctx, `
UPDATE planner_runs SET status = ?, attempt = ?, max_attempts = ?, error = ?,
    adapter = ?, protocol = ?, prompt_template_version = ?, model = ?, parameters_json = ?,
    input_manifest_digest = ?, input_digest = ?, output_digest = ?, usage_json = ?,
    finish_reason = ?, redactions_json = ?, termination_cause = ?, updated_at = ?,
    started_at = NULLIF(?, ''), finished_at = NULLIF(?, '')
WHERE id = ?`,
		run.Status, run.Attempt, run.MaxAttempts, run.Error, run.Provenance.Adapter,
		run.Provenance.Protocol, run.Provenance.PromptTemplateVersion, run.Provenance.Model,
		parameters, run.Provenance.InputManifestDigest, run.Provenance.InputDigest,
		run.Provenance.OutputDigest, usage, run.Provenance.FinishReason, redactions,
		run.Provenance.TerminationCause, shared.TimestampString(run.UpdatedAt), shared.TimestampString(run.StartedAt),
		shared.TimestampString(run.FinishedAt), run.ID)
	if err != nil {
		return fmt.Errorf("update planner run %q: %w", run.ID, err)
	}
	updated, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check planner run %q update: %w", run.ID, err)
	}
	if updated != 1 {
		return fmt.Errorf("%w: %q", ErrPlannerRunNotFound, run.ID)
	}
	return nil
}

// GetPlanRun returns the authoritative persisted planner run.
func (store *RepositoryStore) GetPlanRun(ctx context.Context, runID string) (review.RunRecord, error) {
	if err := store.validate(); err != nil {
		return review.RunRecord{}, err
	}
	runID = strings.TrimSpace(runID)
	if runID == "" {
		return review.RunRecord{}, errors.New("get planner run: run ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	run, err := scanPlanRun(store.database.QueryRowContext(ctx, plannerRunQuery+` WHERE id = ?`, runID))
	if errors.Is(err, sql.ErrNoRows) {
		return review.RunRecord{}, fmt.Errorf("%w: %q", ErrPlannerRunNotFound, runID)
	}
	if err != nil {
		return review.RunRecord{}, fmt.Errorf("get planner run %q: %w", runID, err)
	}
	return run, nil
}

// ListPlanRuns returns all planner provenance records for a round in stable
// creation order.
func (store *RepositoryStore) ListPlanRuns(ctx context.Context, roundID string) ([]review.RunRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(
		ctx,
		plannerRunQuery+` WHERE round_id = ? ORDER BY created_at ASC, id ASC`,
		strings.TrimSpace(roundID),
	)
	if err != nil {
		return nil, fmt.Errorf("list planner runs: %w", err)
	}
	defer rows.Close()
	result := make([]review.RunRecord, 0)
	for rows.Next() {
		run, scanErr := scanPlanRun(rows)
		if scanErr != nil {
			return nil, fmt.Errorf("list planner runs: %w", scanErr)
		}
		result = append(result, run)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list planner runs: %w", err)
	}
	return result, nil
}

// SaveReviewPlan persists the validated plan projection for one successful
// planner run. It does not overwrite an existing run's plan.
func (store *RepositoryStore) SaveReviewPlan(ctx context.Context, plan review.ReviewPlan) error {
	if err := store.validate(); err != nil {
		return err
	}
	if strings.TrimSpace(plan.RunID) == "" || strings.TrimSpace(plan.SessionID) == "" {
		return errors.New("save review plan: run and session IDs are required")
	}
	if plan.Digest == "" || plan.ChangeModelDigest == "" {
		return errors.New("save review plan: plan and change-model digests are required")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var runStatus string
	err := store.database.QueryRowContext(ctx, `SELECT status FROM planner_runs WHERE id = ? AND session_id = ?`, plan.RunID, plan.SessionID).
		Scan(&runStatus)
	if errors.Is(err, sql.ErrNoRows) {
		return fmt.Errorf("%w: %q", ErrPlannerRunNotFound, plan.RunID)
	}
	if err != nil {
		return fmt.Errorf("validate review plan run %q: %w", plan.RunID, err)
	}
	if runStatus != string(review.RunStatusRunning) && runStatus != string(review.RunStatusComplete) {
		return fmt.Errorf("save review plan: run %q is %s", plan.RunID, runStatus)
	}
	data, err := json.Marshal(plan)
	if err != nil {
		return fmt.Errorf("encode review plan: %w", err)
	}
	created := store.now().UTC()
	_, err = store.database.ExecContext(ctx, `
INSERT INTO review_plans (run_id, session_id, round_id, snapshot_id, change_model_digest, plan_digest, plan_json, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)`, plan.RunID, plan.SessionID, plan.RoundID, plan.SnapshotID,
		plan.ChangeModelDigest, plan.Digest, data, shared.TimestampString(created))
	if err != nil {
		return fmt.Errorf("save review plan: %w", err)
	}
	return nil
}

// GetReviewPlan returns the immutable plan for one successful run.
func (store *RepositoryStore) GetReviewPlan(ctx context.Context, runID string) (review.ReviewPlan, error) {
	if err := store.validate(); err != nil {
		return review.ReviewPlan{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	runID = strings.TrimSpace(runID)
	if runID == "" {
		return review.ReviewPlan{}, errors.New("get review plan: run ID is empty")
	}
	var data []byte
	err := store.database.QueryRowContext(ctx, `SELECT plan_json FROM review_plans WHERE run_id = ?`, strings.TrimSpace(runID)).
		Scan(&data)
	if errors.Is(err, sql.ErrNoRows) {
		return review.ReviewPlan{}, fmt.Errorf("%w: %q", ErrReviewPlanNotFound, runID)
	}
	if err != nil {
		return review.ReviewPlan{}, fmt.Errorf("get review plan %q: %w", runID, err)
	}
	var plan review.ReviewPlan
	if err := json.Unmarshal(data, &plan); err != nil {
		return review.ReviewPlan{}, fmt.Errorf("decode review plan %q: %w", runID, err)
	}
	return plan, nil
}

var (
	// ErrPlannerRunNotFound indicates that a run ID is not persisted.
	ErrPlannerRunNotFound = errors.New("planner run not found")
	// ErrReviewPlanNotFound indicates that no successful plan is persisted.
	ErrReviewPlanNotFound = errors.New("review plan not found")
)

func encodeRunFields(run review.RunRecord) (string, string, string, error) {
	parameters, err := json.Marshal(run.Provenance.Parameters)
	if err != nil {
		return "", "", "", fmt.Errorf("encode planner parameters: %w", err)
	}
	usage, err := json.Marshal(run.Provenance.Usage)
	if err != nil {
		return "", "", "", fmt.Errorf("encode planner usage: %w", err)
	}
	redactions, err := json.Marshal(run.Provenance.Redactions)
	if err != nil {
		return "", "", "", fmt.Errorf("encode planner redactions: %w", err)
	}
	return string(parameters), string(usage), string(redactions), nil
}

const plannerRunQuery = `
SELECT id, session_id, round_id, snapshot_id, role, status, attempt, max_attempts,
       error, adapter, protocol, prompt_template_version, model, parameters_json,
       input_manifest_digest, input_digest, output_digest, usage_json, finish_reason,
       redactions_json, termination_cause, created_at, updated_at, started_at, finished_at
FROM planner_runs`

func scanPlanRun(row scanner) (review.RunRecord, error) {
	var run review.RunRecord
	var role, status, parameters, usage, redactions string
	var created, updated, started, finished string
	err := row.Scan(&run.ID, &run.SessionID, &run.RoundID, &run.SnapshotID, &role, &status,
		&run.Attempt, &run.MaxAttempts, &run.Error, &run.Provenance.Adapter, &run.Provenance.Protocol,
		&run.Provenance.PromptTemplateVersion, &run.Provenance.Model, &parameters,
		&run.Provenance.InputManifestDigest, &run.Provenance.InputDigest, &run.Provenance.OutputDigest,
		&usage, &run.Provenance.FinishReason, &redactions, &run.Provenance.TerminationCause,
		&created, &updated, &started, &finished)
	if err != nil {
		return review.RunRecord{}, err
	}
	run.Role, run.Status = review.ModelRole(role), review.RunStatus(status)
	if err := json.Unmarshal([]byte(parameters), &run.Provenance.Parameters); err != nil {
		return review.RunRecord{}, fmt.Errorf("decode planner parameters: %w", err)
	}
	if err := json.Unmarshal([]byte(usage), &run.Provenance.Usage); err != nil {
		return review.RunRecord{}, fmt.Errorf("decode planner usage: %w", err)
	}
	if err := json.Unmarshal([]byte(redactions), &run.Provenance.Redactions); err != nil {
		return review.RunRecord{}, fmt.Errorf("decode planner redactions: %w", err)
	}
	if run.CreatedAt, err = time.Parse(time.RFC3339Nano, created); err != nil {
		return review.RunRecord{}, fmt.Errorf("parse planner created time: %w", err)
	}
	if run.UpdatedAt, err = time.Parse(time.RFC3339Nano, updated); err != nil {
		return review.RunRecord{}, fmt.Errorf("parse planner updated time: %w", err)
	}
	if started != "" {
		run.StartedAt, err = time.Parse(time.RFC3339Nano, started)
		if err != nil {
			return review.RunRecord{}, fmt.Errorf("parse planner started time: %w", err)
		}
	}
	if finished != "" {
		run.FinishedAt, err = time.Parse(time.RFC3339Nano, finished)
		if err != nil {
			return review.RunRecord{}, fmt.Errorf("parse planner finished time: %w", err)
		}
	}
	return run, nil
}
