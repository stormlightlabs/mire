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

var _ review.FindingStore = (*RepositoryStore)(nil)

var (
	// ErrFindingNotFound indicates that an immutable finding revision is absent.
	ErrFindingNotFound = errors.New("finding revision not found")
	// ErrDispositionNotFound indicates that a finding has no recorded decision.
	ErrDispositionNotFound = errors.New("finding disposition not found")
	// ErrPresentationNotFound indicates that a finding has no publishable wording.
	ErrPresentationNotFound = errors.New("finding presentation not found")
)

// SaveFindingRevision inserts one immutable finding revision. Reusing the same
// finding ID and revision is rejected by SQLite rather than silently changing
// machine evidence or identity history.
func (store *RepositoryStore) SaveFindingRevision(ctx context.Context, finding review.FindingRevision) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if err := review.ValidateFindingRevision(finding); err != nil {
		return fmt.Errorf("save finding revision: %w", err)
	}
	if _, err := store.GetSession(ctx, finding.SessionID); err != nil {
		return fmt.Errorf("save finding revision: %w", err)
	}
	round, err := store.GetRound(ctx, finding.RoundID)
	if err != nil {
		return fmt.Errorf("save finding revision: %w", err)
	}
	if round.SessionID != finding.SessionID {
		return errors.New("save finding revision: round belongs to another session")
	}
	data, err := json.Marshal(finding)
	if err != nil {
		return fmt.Errorf("encode finding revision: %w", err)
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO finding_revisions (
    finding_id, revision, session_id, round_id, snapshot_id, digest,
    finding_json, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)`, finding.FindingID, finding.Revision, finding.SessionID,
		finding.RoundID, finding.SnapshotID, finding.Digest, data, shared.TimestampString(finding.CreatedAt.UTC()))
	if err != nil {
		return fmt.Errorf("insert finding revision %q/%d: %w", finding.FindingID, finding.Revision, err)
	}
	return nil
}

// GetFindingRevision returns one immutable finding revision and verifies its
// stored digest and identity columns.
func (store *RepositoryStore) GetFindingRevision(
	ctx context.Context,
	findingID string,
	revision int,
) (review.FindingRevision, error) {
	if err := store.validate(); err != nil {
		return review.FindingRevision{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	findingID = strings.TrimSpace(findingID)
	if findingID == "" || revision < 1 {
		return review.FindingRevision{}, errors.New("get finding revision: finding ID and revision are required")
	}
	var data, expectedDigest, storedID, storedSession, storedRound, storedSnapshot string
	err := store.database.QueryRowContext(ctx, `
SELECT finding_id, session_id, round_id, snapshot_id, digest, finding_json
FROM finding_revisions WHERE finding_id = ? AND revision = ?`, findingID, revision).
		Scan(&storedID, &storedSession, &storedRound, &storedSnapshot, &expectedDigest, &data)
	if errors.Is(err, sql.ErrNoRows) {
		return review.FindingRevision{}, fmt.Errorf("%w: %q/%d", ErrFindingNotFound, findingID, revision)
	}
	if err != nil {
		return review.FindingRevision{}, fmt.Errorf("get finding revision %q/%d: %w", findingID, revision, err)
	}
	return decodeFindingRevision(data, expectedDigest, storedID, storedSession, storedRound, storedSnapshot)
}

// ListFindingRevisions returns all findings recorded in one review round in
// deterministic identity order.
func (store *RepositoryStore) ListFindingRevisions(
	ctx context.Context,
	roundID string,
) ([]review.FindingRevision, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return store.listFindingRevisions(ctx, `round_id = ?`, strings.TrimSpace(roundID))
}

// ListFindingRevisionsForFinding returns the immutable revision history for a
// stable finding ID.
func (store *RepositoryStore) ListFindingRevisionsForFinding(
	ctx context.Context,
	findingID string,
) ([]review.FindingRevision, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return store.listFindingRevisions(ctx, `finding_id = ?`, strings.TrimSpace(findingID))
}

// GetLatestFindingRevision returns the newest immutable revision for a stable
// finding ID.
func (store *RepositoryStore) GetLatestFindingRevision(
	ctx context.Context,
	findingID string,
) (review.FindingRevision, error) {
	if err := store.validate(); err != nil {
		return review.FindingRevision{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	var data, expectedDigest, storedID, storedSession, storedRound, storedSnapshot string
	err := store.database.QueryRowContext(ctx, `
SELECT finding_id, session_id, round_id, snapshot_id, digest, finding_json
FROM finding_revisions WHERE finding_id = ? ORDER BY revision DESC LIMIT 1`, strings.TrimSpace(findingID)).
		Scan(&storedID, &storedSession, &storedRound, &storedSnapshot, &expectedDigest, &data)
	if errors.Is(err, sql.ErrNoRows) {
		return review.FindingRevision{}, fmt.Errorf("%w: %q", ErrFindingNotFound, findingID)
	}
	if err != nil {
		return review.FindingRevision{}, fmt.Errorf("get latest finding revision %q: %w", findingID, err)
	}
	return decodeFindingRevision(data, expectedDigest, storedID, storedSession, storedRound, storedSnapshot)
}

func (store *RepositoryStore) listFindingRevisions(
	ctx context.Context,
	predicate, value string,
) ([]review.FindingRevision, error) {
	if value == "" {
		return nil, errors.New("list finding revisions: selector is empty")
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT finding_id, revision, session_id, round_id, snapshot_id, digest, finding_json, created_at
FROM finding_revisions WHERE `+predicate+` ORDER BY revision ASC, finding_id ASC, created_at ASC`, value)
	if err != nil {
		return nil, fmt.Errorf("list finding revisions: %w", err)
	}
	defer rows.Close()
	findings := make([]review.FindingRevision, 0)
	for rows.Next() {
		var finding review.FindingRevision
		var data, expectedDigest, created string
		if err := rows.Scan(&finding.FindingID, &finding.Revision, &finding.SessionID, &finding.RoundID,
			&finding.SnapshotID, &expectedDigest, &data, &created); err != nil {
			return nil, fmt.Errorf("list finding revisions: %w", err)
		}
		decoded, err := decodeFindingRevision(
			data,
			expectedDigest,
			finding.FindingID,
			finding.SessionID,
			finding.RoundID,
			finding.SnapshotID,
		)
		if err != nil {
			return nil, err
		}
		if decoded.CreatedAt, err = shared.ParseTimestamp(created); err != nil {
			return nil, fmt.Errorf("parse finding revision time: %w", err)
		}
		findings = append(findings, decoded)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list finding revisions: %w", err)
	}
	return findings, nil
}

func decodeFindingRevision(
	data, expectedDigest, storedID, storedSession, storedRound, storedSnapshot string,
) (review.FindingRevision, error) {
	var finding review.FindingRevision
	if err := json.Unmarshal([]byte(data), &finding); err != nil {
		return review.FindingRevision{}, fmt.Errorf("decode finding revision: %w", err)
	}
	if finding.FindingID != storedID || finding.SessionID != storedSession || finding.RoundID != storedRound ||
		finding.SnapshotID != storedSnapshot {
		return review.FindingRevision{}, errors.New("finding revision identity columns do not match payload")
	}
	if finding.Digest != expectedDigest || finding.Digest != review.FindingRevisionDigest(finding) {
		return review.FindingRevision{}, errors.New("finding revision digest mismatch")
	}
	if err := review.ValidateFindingRevision(finding); err != nil {
		return review.FindingRevision{}, fmt.Errorf("validate finding revision: %w", err)
	}
	return finding, nil
}

// SaveDisposition appends one human decision event. It never updates a prior
// event and fills missing session/round metadata from the referenced revision.
func (store *RepositoryStore) SaveDisposition(ctx context.Context, disposition review.DispositionRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	finding, err := store.GetFindingRevision(ctx, disposition.FindingID, disposition.Revision)
	if err != nil {
		return fmt.Errorf("save disposition: %w", err)
	}
	if disposition.SessionID == "" {
		disposition.SessionID = finding.SessionID
	}
	if disposition.RoundID == "" {
		disposition.RoundID = finding.RoundID
	}
	if disposition.SessionID != finding.SessionID || disposition.RoundID != finding.RoundID {
		return errors.New("save disposition: finding provenance does not match revision")
	}
	if disposition.CreatedAt.IsZero() {
		disposition.CreatedAt = store.now().UTC()
	}
	if disposition.ID == "" {
		disposition.ID, err = store.newID()
		if err != nil {
			return fmt.Errorf("save disposition ID: %w", err)
		}
	}
	if err := review.ValidateDisposition(disposition); err != nil {
		return fmt.Errorf("save disposition: %w", err)
	}
	disposition.Digest = review.DispositionDigest(disposition)
	data, err := json.Marshal(disposition)
	if err != nil {
		return fmt.Errorf("encode disposition: %w", err)
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO finding_dispositions (
    id, finding_id, revision, session_id, round_id, disposition, rationale,
    digest, disposition_json, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, disposition.ID, disposition.FindingID, disposition.Revision,
		disposition.SessionID, disposition.RoundID, disposition.Disposition, disposition.Rationale,
		disposition.Digest, data, shared.TimestampString(disposition.CreatedAt.UTC()))
	if err != nil {
		return fmt.Errorf("insert disposition %q: %w", disposition.ID, err)
	}
	return nil
}

// ListDispositions returns all human decision events for a finding in append
// order. The caller can derive the current decision from the final event.
func (store *RepositoryStore) ListDispositions(
	ctx context.Context,
	findingID string,
) ([]review.DispositionRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT digest, disposition_json FROM finding_dispositions
WHERE finding_id = ? ORDER BY created_at ASC, rowid ASC`, strings.TrimSpace(findingID))
	if err != nil {
		return nil, fmt.Errorf("list dispositions: %w", err)
	}
	defer rows.Close()
	result := make([]review.DispositionRecord, 0)
	for rows.Next() {
		var expectedDigest, data string
		if err := rows.Scan(&expectedDigest, &data); err != nil {
			return nil, fmt.Errorf("list dispositions: %w", err)
		}
		var disposition review.DispositionRecord
		if err := json.Unmarshal([]byte(data), &disposition); err != nil {
			return nil, fmt.Errorf("decode disposition: %w", err)
		}
		if disposition.Digest != expectedDigest || disposition.Digest != review.DispositionDigest(disposition) {
			return nil, errors.New("disposition digest mismatch")
		}
		result = append(result, disposition)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list dispositions: %w", err)
	}
	return result, nil
}

// GetCurrentDisposition returns the latest decision or an implicit open state
// when a finding has not yet received a human event.
func (store *RepositoryStore) GetCurrentDisposition(
	ctx context.Context,
	findingID string,
) (review.DispositionRecord, error) {
	events, err := store.ListDispositions(ctx, findingID)
	if err != nil {
		return review.DispositionRecord{}, err
	}
	if len(events) > 0 {
		return events[len(events)-1], nil
	}
	finding, findingErr := store.GetLatestFindingRevision(ctx, findingID)
	if findingErr != nil {
		return review.DispositionRecord{}, findingErr
	}
	return review.DispositionRecord{
		FindingID: finding.FindingID, Revision: finding.Revision,
		SessionID: finding.SessionID, RoundID: finding.RoundID, Disposition: review.FindingDispositionOpen,
		CreatedAt: finding.CreatedAt,
	}, nil
}

// SavePresentation appends one version of publishable finding wording.
func (store *RepositoryStore) SavePresentation(ctx context.Context, presentation review.PresentationRecord) error {
	if err := store.validate(); err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if presentation.SchemaVersion == "" {
		presentation.SchemaVersion = review.FindingPresentationSchemaVersion
	}
	if strings.TrimSpace(presentation.Body) == "" {
		presentation.Body = strings.TrimSpace(presentation.Comment)
	}
	if strings.TrimSpace(presentation.Body) == "" {
		presentation.Body = strings.TrimSpace(presentation.Wording)
	}
	if presentation.FindingRevision < 1 {
		finding, err := store.GetLatestFindingRevision(ctx, presentation.FindingID)
		if err != nil {
			return fmt.Errorf("save presentation: %w", err)
		}
		presentation.FindingRevision = finding.Revision
	}
	if _, err := store.GetFindingRevision(ctx, presentation.FindingID, presentation.FindingRevision); err != nil {
		return fmt.Errorf("save presentation: %w", err)
	}
	if presentation.Version < 1 {
		if err := store.database.QueryRowContext(ctx, `SELECT COALESCE(MAX(version), 0) + 1 FROM finding_presentations WHERE finding_id = ?`, presentation.FindingID).
			Scan(&presentation.Version); err != nil {
			return fmt.Errorf("choose presentation version: %w", err)
		}
	}
	if presentation.CreatedAt.IsZero() {
		presentation.CreatedAt = store.now().UTC()
	}
	if presentation.ID == "" {
		var err error
		presentation.ID, err = store.newID()
		if err != nil {
			return fmt.Errorf("save presentation ID: %w", err)
		}
	}
	if err := review.ValidatePresentation(presentation); err != nil {
		return fmt.Errorf("save presentation: %w", err)
	}
	presentation.Digest = review.PresentationDigest(presentation)
	data, err := json.Marshal(presentation)
	if err != nil {
		return fmt.Errorf("encode presentation: %w", err)
	}
	_, err = store.database.ExecContext(ctx, `
INSERT INTO finding_presentations (
    id, finding_id, finding_revision, version, digest, presentation_json, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?)`, presentation.ID, presentation.FindingID, presentation.FindingRevision,
		presentation.Version, presentation.Digest, data, shared.TimestampString(presentation.CreatedAt.UTC()))
	if err != nil {
		return fmt.Errorf("insert presentation %q: %w", presentation.ID, err)
	}
	return nil
}

// ListPresentations returns all wording versions for a finding in version
// order. It never changes the associated finding revision.
func (store *RepositoryStore) ListPresentations(
	ctx context.Context,
	findingID string,
) ([]review.PresentationRecord, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT digest, presentation_json FROM finding_presentations
WHERE finding_id = ? ORDER BY version ASC, id ASC`, strings.TrimSpace(findingID))
	if err != nil {
		return nil, fmt.Errorf("list presentations: %w", err)
	}
	defer rows.Close()
	result := make([]review.PresentationRecord, 0)
	for rows.Next() {
		var expectedDigest, data string
		if err := rows.Scan(&expectedDigest, &data); err != nil {
			return nil, fmt.Errorf("list presentations: %w", err)
		}
		var presentation review.PresentationRecord
		if err := json.Unmarshal([]byte(data), &presentation); err != nil {
			return nil, fmt.Errorf("decode presentation: %w", err)
		}
		if presentation.Digest != expectedDigest || presentation.Digest != review.PresentationDigest(presentation) {
			return nil, errors.New("presentation digest mismatch")
		}
		result = append(result, presentation)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list presentations: %w", err)
	}
	return result, nil
}

// GetLatestPresentation returns the newest wording version for a finding.
func (store *RepositoryStore) GetLatestPresentation(
	ctx context.Context,
	findingID string,
) (review.PresentationRecord, error) {
	presentations, err := store.ListPresentations(ctx, findingID)
	if err != nil {
		return review.PresentationRecord{}, err
	}
	if len(presentations) == 0 {
		return review.PresentationRecord{}, fmt.Errorf("%w: %q", ErrPresentationNotFound, findingID)
	}
	return presentations[len(presentations)-1], nil
}

// Ensure the compiler keeps the time import used by persistence timestamps
// explicit when this file is built with alternate database drivers.
var _ = time.RFC3339Nano
