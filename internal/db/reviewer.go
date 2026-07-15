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
	"github.com/stormlightlabs/mire/internal/shared"
)

var _ review.ReviewStore = (*RepositoryStore)(nil)

var (
	// ErrReviewRunNotFound indicates that a specialized reviewer run is absent.
	ErrReviewRunNotFound = errors.New("review run not found")
	// ErrReviewCoverageNotFound indicates that no durable coverage exists for a
	// round.
	ErrReviewCoverageNotFound = errors.New("review coverage not found")
	// ErrReviewCandidateNotFound indicates that a retained candidate is absent.
	ErrReviewCandidateNotFound = errors.New("review candidate not found")
)

// CreateReviewRun persists a queued specialized reviewer run.
func (store *RepositoryStore) CreateReviewRun(ctx context.Context, run review.RunRecord) (review.RunRecord, error) {
	if err := store.validate(); err != nil {
		return review.RunRecord{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if strings.TrimSpace(run.SessionID) == "" {
		return review.RunRecord{}, errors.New("create review run: session ID is empty")
	}
	if run.Role != review.ModelRoleReviewer || run.Status != review.RunStatusQueued {
		return review.RunRecord{}, errors.New("create review run: run must be a queued reviewer run")
	}
	if strings.TrimSpace(run.PassName) == "" {
		return review.RunRecord{}, errors.New("create review run: pass name is empty")
	}
	if run.MaxAttempts < 1 {
		return review.RunRecord{}, errors.New("create review run: max attempts must be positive")
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
			return review.RunRecord{}, fmt.Errorf("create review run: round belongs to another session")
		}
	}
	var err error
	if run.ID == "" {
		run.ID, err = store.newID()
		if err != nil {
			return review.RunRecord{}, fmt.Errorf("create review run ID: %w", err)
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
	_, err = store.database.ExecContext(ctx, `
INSERT INTO review_runs (
    id, session_id, round_id, snapshot_id, role, pass_name, status, attempt,
    max_attempts, error, adapter, protocol, prompt_template_version, model,
    parameters_json, input_manifest_digest, input_digest, output_digest,
    usage_json, finish_reason, redactions_json, termination_cause, created_at,
    updated_at, started_at, finished_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULLIF(?, ''), NULLIF(?, ''))`,
		run.ID, run.SessionID, run.RoundID, run.SnapshotID, run.Role, run.PassName, run.Status,
		run.Attempt, run.MaxAttempts, run.Error, run.Provenance.Adapter, run.Provenance.Protocol,
		run.Provenance.PromptTemplateVersion, run.Provenance.Model, parameters,
		run.Provenance.InputManifestDigest, run.Provenance.InputDigest, run.Provenance.OutputDigest,
		usage, run.Provenance.FinishReason, redactions, run.Provenance.TerminationCause,
		shared.TimestampString(now), shared.TimestampString(run.UpdatedAt), shared.TimestampString(run.StartedAt), shared.TimestampString(run.FinishedAt))
	if err != nil {
		return review.RunRecord{}, fmt.Errorf("insert review run: %w", err)
	}
	run.CreatedAt = now
	return run, nil
}

// UpdateReviewRun persists an in-progress or terminal specialized reviewer run.
func (store *RepositoryStore) UpdateReviewRun(ctx context.Context, run review.RunRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if run.Role != review.ModelRoleReviewer || !run.Status.Valid() {
		return errors.New("update review run: invalid reviewer run")
	}
	if strings.TrimSpace(run.ID) == "" || strings.TrimSpace(run.PassName) == "" {
		return errors.New("update review run: run and pass IDs are required")
	}
	parameters, usage, redactions, err := encodeRunFields(run)
	if err != nil {
		return err
	}
	result, err := store.database.ExecContext(ctx, `
UPDATE review_runs SET status = ?, attempt = ?, max_attempts = ?, error = ?,
    pass_name = ?, adapter = ?, protocol = ?, prompt_template_version = ?,
    model = ?, parameters_json = ?, input_manifest_digest = ?, input_digest = ?,
    output_digest = ?, usage_json = ?, finish_reason = ?, redactions_json = ?,
    termination_cause = ?, updated_at = ?, started_at = NULLIF(?, ''),
    finished_at = NULLIF(?, '')
WHERE id = ?`,
		run.Status, run.Attempt, run.MaxAttempts, run.Error, run.PassName,
		run.Provenance.Adapter, run.Provenance.Protocol, run.Provenance.PromptTemplateVersion,
		run.Provenance.Model, parameters, run.Provenance.InputManifestDigest,
		run.Provenance.InputDigest, run.Provenance.OutputDigest, usage,
		run.Provenance.FinishReason, redactions, run.Provenance.TerminationCause,
		shared.TimestampString(run.UpdatedAt), shared.TimestampString(run.StartedAt), shared.TimestampString(run.FinishedAt), run.ID)
	if err != nil {
		return fmt.Errorf("update review run %q: %w", run.ID, err)
	}
	updated, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check review run %q update: %w", run.ID, err)
	}
	if updated != 1 {
		return fmt.Errorf("%w: %q", ErrReviewRunNotFound, run.ID)
	}
	return nil
}

// GetReviewRun returns the authoritative specialized reviewer run.
func (store *RepositoryStore) GetReviewRun(ctx context.Context, runID string) (review.RunRecord, error) {
	if err := store.validate(); err != nil {
		return review.RunRecord{}, err
	}
	runID = strings.TrimSpace(runID)
	if runID == "" {
		return review.RunRecord{}, errors.New("get review run: run ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	run, err := scanReviewRun(store.database.QueryRowContext(ctx, reviewRunQuery+` WHERE id = ?`, runID))
	if errors.Is(err, sql.ErrNoRows) {
		return review.RunRecord{}, fmt.Errorf("%w: %q", ErrReviewRunNotFound, runID)
	}
	if err != nil {
		return review.RunRecord{}, fmt.Errorf("get review run %q: %w", runID, err)
	}
	return run, nil
}

// SaveReviewPass atomically retains a pass outcome, every candidate emission,
// every retrieval descriptor, and the cumulative coverage projection.
func (store *RepositoryStore) SaveReviewPass(ctx context.Context, result review.ReviewPassResult) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	pass := result.Pass
	if strings.TrimSpace(pass.SessionID) == "" || strings.TrimSpace(pass.Name) == "" {
		return errors.New("save review pass: session and pass name are required")
	}
	if !pass.Status.Valid() {
		return fmt.Errorf("save review pass: invalid status %q", pass.Status)
	}
	if result.PassID == "" {
		var err error
		result.PassID, err = store.newID()
		if err != nil {
			return fmt.Errorf("save review pass ID: %w", err)
		}
	}
	if result.Run.ID != "" {
		if result.Run.Role != review.ModelRoleReviewer || result.Run.SessionID != pass.SessionID || result.Run.PassName != pass.Name {
			return errors.New("save review pass: run does not belong to pass")
		}
		var runCount int
		if err := store.database.QueryRowContext(ctx, `SELECT COUNT(*) FROM review_runs WHERE id = ? AND session_id = ?`, result.Run.ID, pass.SessionID).Scan(&runCount); err != nil {
			return fmt.Errorf("validate review pass run: %w", err)
		}
		if runCount != 1 {
			return fmt.Errorf("%w: %q", ErrReviewRunNotFound, result.Run.ID)
		}
	}
	if len(result.Candidates) > 0 && result.Run.ID == "" {
		return errors.New("save review pass: candidates require a reviewer run")
	}
	coverage := result.Coverage
	if coverage.Digest == "" {
		coverage = digestCoverage(coverage)
	}
	passJSON, err := json.Marshal(pass)
	if err != nil {
		return fmt.Errorf("encode review pass: %w", err)
	}
	diagnosticsJSON, err := json.Marshal(result.Diagnostics)
	if err != nil {
		return fmt.Errorf("encode review diagnostics: %w", err)
	}
	coverageJSON, err := json.Marshal(coverage)
	if err != nil {
		return fmt.Errorf("encode review coverage: %w", err)
	}
	now := store.now().UTC()
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin review pass transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()
	if _, err := tx.ExecContext(ctx, `
INSERT INTO review_passes (
    id, session_id, round_id, snapshot_id, run_id, name, status, applicable,
    reason, candidate_count, pass_json, diagnostics_json, created_at,
    started_at, finished_at
)
VALUES (?, ?, ?, ?, NULLIF(?, ''), ?, ?, ?, ?, ?, ?, ?, ?, NULLIF(?, ''), NULLIF(?, ''))`,
		result.PassID, pass.SessionID, pass.RoundID, pass.SnapshotID, result.Run.ID, pass.Name,
		pass.Status, shared.BoolInt(pass.Applicable), pass.Reason, len(result.Candidates), passJSON,
		diagnosticsJSON, shared.TimestampString(now), shared.TimestampString(pass.StartedAt), shared.TimestampString(pass.FinishedAt)); err != nil {
		return fmt.Errorf("insert review pass: %w", err)
	}
	for _, candidate := range result.Candidates {
		if candidate.ID == "" || candidate.RunID != result.Run.ID || candidate.PassName != pass.Name || candidate.Ordinal < 0 {
			return errors.New("save review pass: candidate identity does not match pass")
		}
		candidateJSON, err := json.Marshal(candidate.Candidate)
		if err != nil {
			return fmt.Errorf("encode retained candidate %q: %w", candidate.ID, err)
		}
		if _, err := tx.ExecContext(ctx, `
INSERT INTO review_candidates (
    id, session_id, round_id, snapshot_id, pass_id, run_id, pass_name,
    ordinal, fingerprint, candidate_json, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, candidate.ID, pass.SessionID, pass.RoundID,
			pass.SnapshotID, result.PassID, candidate.RunID, candidate.PassName, candidate.Ordinal,
			candidate.Fingerprint, candidateJSON, shared.TimestampString(candidate.CreatedAt)); err != nil {
			return fmt.Errorf("insert retained candidate %q: %w", candidate.ID, err)
		}
	}
	for _, artifact := range result.Artifacts {
		if artifact.ID == "" || artifact.RunID != result.Run.ID || artifact.PassName != pass.Name || artifact.Digest == "" || artifact.Size < 0 {
			return errors.New("save review pass: artifact identity is incomplete")
		}
		hunkIDs, err := json.Marshal(artifact.HunkIDs)
		if err != nil {
			return fmt.Errorf("encode retrieved artifact %q: %w", artifact.ID, err)
		}
		if _, err := tx.ExecContext(ctx, `
INSERT INTO review_artifacts (
    id, session_id, round_id, snapshot_id, pass_id, run_id, pass_name, kind,
    path, relation, hunk_ids_json, digest, size, content, excluded,
    exclusion_reason, truncated, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, artifact.ID,
			pass.SessionID, pass.RoundID, pass.SnapshotID, result.PassID, artifact.RunID,
			artifact.PassName, artifact.Kind, artifact.Path, artifact.Relation, hunkIDs,
			artifact.Digest, artifact.Size, artifact.Content, shared.BoolInt(artifact.Excluded),
			artifact.ExclusionReason, shared.BoolInt(artifact.Truncated), shared.TimestampString(now)); err != nil {
			return fmt.Errorf("insert retrieved artifact %q: %w", artifact.ID, err)
		}
	}
	if _, err := tx.ExecContext(ctx, `
INSERT INTO review_coverage (round_id, session_id, snapshot_id, coverage_digest,
    coverage_json, created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(round_id) DO UPDATE SET session_id = excluded.session_id,
    snapshot_id = excluded.snapshot_id, coverage_digest = excluded.coverage_digest,
    coverage_json = excluded.coverage_json, updated_at = excluded.updated_at`,
		pass.RoundID, pass.SessionID, pass.SnapshotID, coverage.Digest, coverageJSON,
		shared.TimestampString(now), shared.TimestampString(now)); err != nil {
		return fmt.Errorf("save review coverage: %w", err)
	}
	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit review pass: %w", err)
	}
	return nil
}

// GetReviewCoverage returns the durable cumulative coverage for a round.
func (store *RepositoryStore) GetReviewCoverage(ctx context.Context, roundID string) (review.ReviewCoverage, error) {
	if err := store.validate(); err != nil {
		return review.ReviewCoverage{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var data, expectedDigest string
	err := store.database.QueryRowContext(ctx, `SELECT coverage_digest, coverage_json FROM review_coverage WHERE round_id = ?`, roundID).Scan(&expectedDigest, &data)
	if errors.Is(err, sql.ErrNoRows) {
		return review.ReviewCoverage{}, fmt.Errorf("%w: %q", ErrReviewCoverageNotFound, roundID)
	}
	if err != nil {
		return review.ReviewCoverage{}, fmt.Errorf("get review coverage: %w", err)
	}
	var coverage review.ReviewCoverage
	if err := json.Unmarshal([]byte(data), &coverage); err != nil {
		return review.ReviewCoverage{}, fmt.Errorf("decode review coverage: %w", err)
	}
	if digestCoverage(coverage).Digest != expectedDigest {
		return review.ReviewCoverage{}, fmt.Errorf("get review coverage: coverage digest mismatch")
	}
	return coverage, nil
}

// ListReviewPasses returns all persisted pass outcomes in deterministic order.
func (store *RepositoryStore) ListReviewPasses(ctx context.Context, roundID string) ([]review.PassCoverage, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `SELECT pass_json FROM review_passes WHERE round_id = ? ORDER BY name ASC, id ASC`, roundID)
	if err != nil {
		return nil, fmt.Errorf("list review passes: %w", err)
	}
	defer rows.Close()
	passes := make([]review.PassCoverage, 0)
	for rows.Next() {
		var data string
		if err := rows.Scan(&data); err != nil {
			return nil, fmt.Errorf("list review passes: %w", err)
		}
		var pass review.PassCoverage
		if err := json.Unmarshal([]byte(data), &pass); err != nil {
			return nil, fmt.Errorf("decode review pass: %w", err)
		}
		passes = append(passes, pass)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list review passes: %w", err)
	}
	return passes, nil
}

// ListReviewDiagnostics returns the durable diagnostics emitted by all passes
// in a round, ordered by pass and diagnostic identity.
func (store *RepositoryStore) ListReviewDiagnostics(ctx context.Context, roundID string) ([]review.ReviewDiagnostic, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT diagnostics_json FROM review_passes
WHERE round_id = ? ORDER BY name ASC, id ASC`, roundID)
	if err != nil {
		return nil, fmt.Errorf("list review diagnostics: %w", err)
	}
	defer rows.Close()
	diagnostics := make([]review.ReviewDiagnostic, 0)
	for rows.Next() {
		var data string
		if err := rows.Scan(&data); err != nil {
			return nil, fmt.Errorf("list review diagnostics: %w", err)
		}
		var values []review.ReviewDiagnostic
		if err := json.Unmarshal([]byte(data), &values); err != nil {
			return nil, fmt.Errorf("decode review diagnostics: %w", err)
		}
		diagnostics = append(diagnostics, values...)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list review diagnostics: %w", err)
	}
	return diagnostics, nil
}

// ListReviewCandidates returns every retained candidate for a round. No
// correlation or presentation filtering is performed by this query.
func (store *RepositoryStore) ListReviewCandidates(ctx context.Context, roundID string) ([]review.CandidateRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT id, run_id, pass_name, ordinal, fingerprint, candidate_json, created_at
FROM review_candidates WHERE round_id = ? ORDER BY created_at ASC, id ASC`, roundID)
	if err != nil {
		return nil, fmt.Errorf("list review candidates: %w", err)
	}
	defer rows.Close()
	candidates := make([]review.CandidateRecord, 0)
	for rows.Next() {
		var candidate review.CandidateRecord
		var data, created string
		if err := rows.Scan(&candidate.ID, &candidate.RunID, &candidate.PassName, &candidate.Ordinal, &candidate.Fingerprint, &data, &created); err != nil {
			return nil, fmt.Errorf("list review candidates: %w", err)
		}
		if err := json.Unmarshal([]byte(data), &candidate.Candidate); err != nil {
			return nil, fmt.Errorf("decode retained candidate %q: %w", candidate.ID, err)
		}
		var err error
		candidate.CreatedAt, err = time.Parse(time.RFC3339Nano, created)
		if err != nil {
			return nil, fmt.Errorf("parse retained candidate time: %w", err)
		}
		candidates = append(candidates, candidate)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list review candidates: %w", err)
	}
	return candidates, nil
}

func digestCoverage(coverage review.ReviewCoverage) review.ReviewCoverage {
	coverage.Digest = ""
	data, err := json.Marshal(coverage)
	if err == nil {
		digest := sha256.Sum256(data)
		coverage.Digest = hex.EncodeToString(digest[:])
	}
	return coverage
}

const reviewRunQuery = `
SELECT id, session_id, round_id, snapshot_id, role, pass_name, status, attempt,
       max_attempts, error, adapter, protocol, prompt_template_version, model,
       parameters_json, input_manifest_digest, input_digest, output_digest,
       usage_json, finish_reason, redactions_json, termination_cause, created_at,
       updated_at, started_at, finished_at
FROM review_runs`

func scanReviewRun(row scanner) (review.RunRecord, error) {
	var run review.RunRecord
	var role, status, parameters, usage, redactions string
	var created, updated, started, finished string
	err := row.Scan(&run.ID, &run.SessionID, &run.RoundID, &run.SnapshotID, &role, &run.PassName, &status,
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
		return review.RunRecord{}, fmt.Errorf("decode review run parameters: %w", err)
	}
	if err := json.Unmarshal([]byte(usage), &run.Provenance.Usage); err != nil {
		return review.RunRecord{}, fmt.Errorf("decode review run usage: %w", err)
	}
	if err := json.Unmarshal([]byte(redactions), &run.Provenance.Redactions); err != nil {
		return review.RunRecord{}, fmt.Errorf("decode review run redactions: %w", err)
	}
	if run.CreatedAt, err = time.Parse(time.RFC3339Nano, created); err != nil {
		return review.RunRecord{}, fmt.Errorf("parse review run created time: %w", err)
	}
	if run.UpdatedAt, err = time.Parse(time.RFC3339Nano, updated); err != nil {
		return review.RunRecord{}, fmt.Errorf("parse review run updated time: %w", err)
	}
	if started != "" {
		run.StartedAt, err = time.Parse(time.RFC3339Nano, started)
		if err != nil {
			return review.RunRecord{}, fmt.Errorf("parse review run started time: %w", err)
		}
	}
	if finished != "" {
		run.FinishedAt, err = time.Parse(time.RFC3339Nano, finished)
		if err != nil {
			return review.RunRecord{}, fmt.Errorf("parse review run finished time: %w", err)
		}
	}
	return run, nil
}
