package db

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/stormlightlabs/mire/internal/shared"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

// ErrSnapshotNotFound is returned when a snapshot ID is not persisted.
var ErrSnapshotNotFound = errors.New("snapshot not found")

// Snapshot is immutable provenance and manifest metadata for one capture.
type Snapshot struct {
	ID                   string
	RepositoryID         string
	Kind                 string
	RequestedComparison  string
	BaseOID              string
	EffectiveBaseOID     string
	TargetOID            string
	MergeBaseOID         string
	IndexOID             string
	ObjectFormat         string
	ContextPolicyHash    string
	IgnorePolicy         string
	BaseManifestDigest   string
	TargetManifestDigest string
	ManifestDigest       string
	Complete             bool
	CreatedAt            time.Time
	Layers               []SnapshotLayer
}

// SnapshotLayer is one persisted immutable layer of a snapshot.
type SnapshotLayer struct {
	SnapshotID     string
	Layer          string
	Identity       string
	ManifestDigest string
}

// SnapshotEntry is one persisted complete-tree manifest entry.
type SnapshotEntry struct {
	SnapshotID    string
	TreeSide      string
	Path          string
	Kind          string
	Mode          uint32
	Size          int64
	ContentDigest string
	GitOID        string
	SymlinkTarget string
}

// SnapshotChange is one durable comparison record between the two complete
// manifests.
type SnapshotChange struct {
	SnapshotID   string
	Status       string
	BasePath     string
	TargetPath   string
	BaseDigest   string
	TargetDigest string
}

// CreateCapturedSession atomically creates the repository, user-visible
// session, immutable snapshot, complete manifests, changes, and first round.
// Callers must capture and verify all object bytes before invoking it.
func (store *RepositoryStore) CreateCapturedSession(
	ctx context.Context,
	identity RepositoryIdentity,
	title string,
	capture snapshot.Capture,
) (Session, Round, Snapshot, error) {
	if err := store.validate(); err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	identity, err := normalizeIdentity(identity)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	title = strings.TrimSpace(title)
	if title == "" {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("create captured session: title is empty")
	}
	capture, err = normalizeCapture(capture)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	if err := validateCapture(capture); err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now := capture.CapturedAt.UTC()
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("begin captured session transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	repository, err := store.ensureRepositoryTx(ctx, tx, identity)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	sessionID, err := store.newID()
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("create captured session ID: %w", err)
	}
	snapshotID, err := store.newID()
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("create snapshot ID: %w", err)
	}
	roundID, err := store.newID()
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("create round ID: %w", err)
	}
	if _, err := tx.ExecContext(ctx, `
INSERT INTO sessions (id, repository_id, title, created_at)
VALUES (?, ?, ?, ?)`, sessionID, repository.ID, title, shared.TimestampString(now)); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("insert captured session: %w", err)
	}
	persistedSnapshot, err := persistSnapshotTx(ctx, tx, repository.ID, snapshotID, capture)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	round := Round{
		ID: roundID, SessionID: sessionID, RepositoryID: repository.ID, SnapshotID: snapshotID,
		Number: 1, Status: RoundStatusPending, CreatedAt: now, UpdatedAt: now,
	}
	if _, err := tx.ExecContext(
		ctx,
		`
INSERT INTO rounds (id, session_id, repository_id, snapshot_id, number, status, created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		round.ID,
		round.SessionID,
		round.RepositoryID,
		round.SnapshotID,
		round.Number,
		round.Status,
		shared.TimestampString(round.CreatedAt),
		shared.TimestampString(round.UpdatedAt),
	); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("insert captured round: %w", err)
	}
	if _, err := tx.ExecContext(
		ctx,
		`UPDATE sessions SET current_round_id = ? WHERE id = ?`,
		round.ID,
		sessionID,
	); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("set captured current round: %w", err)
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID: sessionID, RepositoryID: repository.ID, RoundID: round.ID,
		Kind: "round.created", Status: string(round.Status), Message: "Round created.", CreatedAt: now,
	}); err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	if err := tx.Commit(); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("commit captured session: %w", err)
	}
	return Session{
		ID: sessionID, RepositoryID: repository.ID, RepositoryName: repository.DisplayName,
		RepositoryIdentity: repository.CanonicalIdentity, Title: title, CreatedAt: now,
		CurrentRoundID: round.ID,
	}, round, persistedSnapshot, nil
}

// AppendCapturedRound appends one immutable capture to an existing session.
// The session and capture repository identities must match, and the new round
// becomes current only when the complete snapshot transaction commits.
func (store *RepositoryStore) AppendCapturedRound(
	ctx context.Context,
	sessionID string,
	identity RepositoryIdentity,
	capture snapshot.Capture,
) (Session, Round, Snapshot, error) {
	if err := store.validate(); err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	sessionID = strings.TrimSpace(sessionID)
	if sessionID == "" {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("append captured round: session ID is empty")
	}
	identity, err := normalizeIdentity(identity)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	capture, err = normalizeCapture(capture)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	if err := validateCapture(capture); err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	now := capture.CapturedAt.UTC()
	tx, err := store.database.BeginTx(ctx, nil)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("begin append round transaction: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	session, err := scanSession(tx.QueryRowContext(ctx, sessionQuery+` WHERE s.id = ?`, sessionID))
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return Session{}, Round{}, Snapshot{}, fmt.Errorf("%w: %q", ErrSessionNotFound, sessionID)
		}
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("read append session %q: %w", sessionID, err)
	}
	if session.RepositoryIdentity != identity.CanonicalIdentity {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf(
			"%w: session %q is for %q, current repository is %q",
			ErrSessionRepositoryMismatch,
			sessionID,
			session.RepositoryIdentity,
			identity.CanonicalIdentity,
		)
	}

	snapshotID, err := store.newID()
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("create appended snapshot ID: %w", err)
	}
	persistedSnapshot, err := persistSnapshotTx(ctx, tx, session.RepositoryID, snapshotID, capture)
	if err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	var predecessorRoundID string
	if err := tx.QueryRowContext(
		ctx,
		`SELECT COALESCE(current_round_id, '') FROM sessions WHERE id = ?`, sessionID,
	).Scan(&predecessorRoundID); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("read append predecessor: %w", err)
	}
	var number int
	if err := tx.QueryRowContext(
		ctx,
		`SELECT COALESCE(MAX(number), 0) + 1 FROM rounds WHERE session_id = ?`, sessionID,
	).Scan(&number); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("choose appended round number: %w", err)
	}
	roundID, err := store.newID()
	if err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("create appended round ID: %w", err)
	}
	round := Round{
		ID: roundID, SessionID: sessionID, RepositoryID: session.RepositoryID,
		SnapshotID: snapshotID, PredecessorRoundID: predecessorRoundID,
		Number: number, Status: RoundStatusPending, CreatedAt: now, UpdatedAt: now,
	}
	if _, err := tx.ExecContext(
		ctx,
		`
INSERT INTO rounds (id, session_id, repository_id, snapshot_id, predecessor_round_id, number, status, created_at, updated_at)
VALUES (?, ?, ?, ?, NULLIF(?, ''), ?, ?, ?, ?)`,
		round.ID,
		round.SessionID,
		round.RepositoryID,
		round.SnapshotID,
		round.PredecessorRoundID,
		round.Number,
		round.Status,
		shared.TimestampString(round.CreatedAt),
		shared.TimestampString(round.UpdatedAt),
	); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("insert appended round: %w", err)
	}
	if _, err := tx.ExecContext(
		ctx,
		`UPDATE sessions SET current_round_id = ? WHERE id = ?`,
		round.ID,
		sessionID,
	); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("set appended current round: %w", err)
	}
	if err := insertActivityTx(ctx, tx, Activity{
		SessionID: sessionID, RepositoryID: session.RepositoryID, RoundID: round.ID,
		Kind: "round.created", Status: string(round.Status), Message: "Round created.", CreatedAt: now,
	}); err != nil {
		return Session{}, Round{}, Snapshot{}, err
	}
	if err := tx.Commit(); err != nil {
		return Session{}, Round{}, Snapshot{}, fmt.Errorf("commit appended round: %w", err)
	}
	session.CurrentRoundID = round.ID
	return session, round, persistedSnapshot, nil
}

func persistSnapshotTx(
	ctx context.Context,
	tx *sql.Tx,
	repositoryID, snapshotID string,
	capture snapshot.Capture,
) (Snapshot, error) {
	persisted := Snapshot{
		ID: snapshotID, RepositoryID: repositoryID, Kind: captureKind(capture),
		RequestedComparison: capture.RequestedComparison, BaseOID: capture.BaseOID,
		EffectiveBaseOID: capture.EffectiveBaseOID, TargetOID: capture.Target.OID,
		MergeBaseOID: capture.MergeBaseOID, IndexOID: capture.Index.OID,
		ObjectFormat: capture.ObjectFormat, ContextPolicyHash: capture.ContextPolicyHash,
		IgnorePolicy: capture.IgnorePolicy, BaseManifestDigest: capture.Base.ManifestDigest,
		TargetManifestDigest: capture.Target.ManifestDigest, ManifestDigest: capture.ManifestDigest,
		Complete: true, CreatedAt: capture.CapturedAt.UTC(),
	}
	if _, err := tx.ExecContext(ctx, `
INSERT INTO snapshots (
    id, repository_id, kind, requested_comparison, base_oid, effective_base_oid,
    target_oid, merge_base_oid, index_oid, object_format, context_policy_hash, ignore_policy,
    base_manifest_digest, target_manifest_digest, manifest_digest, complete, created_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)`,
		persisted.ID, persisted.RepositoryID, persisted.Kind, persisted.RequestedComparison,
		persisted.BaseOID, persisted.EffectiveBaseOID, persisted.TargetOID, persisted.MergeBaseOID,
		persisted.IndexOID, persisted.ObjectFormat, persisted.ContextPolicyHash, persisted.IgnorePolicy,
		persisted.BaseManifestDigest, persisted.TargetManifestDigest, persisted.ManifestDigest,
		shared.TimestampString(persisted.CreatedAt)); err != nil {
		return Snapshot{}, fmt.Errorf("insert snapshot: %w", err)
	}
	if captureKind(capture) == snapshot.ComparisonWorktree {
		for _, layer := range []struct {
			name    string
			entries []snapshot.Entry
		}{
			{name: snapshot.TreeSideHead, entries: capture.Head.Entries},
			{name: snapshot.TreeSideIndex, entries: capture.Index.Entries},
			{name: snapshot.TreeSideWorktree, entries: capture.Worktree.Entries},
		} {
			if err := insertSnapshotEntriesTx(ctx, tx, snapshotID, layer.name, layer.entries); err != nil {
				return Snapshot{}, err
			}
		}
	} else {
		if err := insertSnapshotEntriesTx(
			ctx,
			tx,
			snapshotID,
			snapshot.TreeSideBase,
			capture.Base.Entries,
		); err != nil {
			return Snapshot{}, err
		}
		if err := insertSnapshotEntriesTx(
			ctx,
			tx,
			snapshotID,
			snapshot.TreeSideTarget,
			capture.Target.Entries,
		); err != nil {
			return Snapshot{}, err
		}
	}
	for _, layer := range capture.ManifestLayers() {
		if _, err := tx.ExecContext(ctx, `
INSERT INTO snapshot_layers (snapshot_id, layer, identity, manifest_digest)
VALUES (?, ?, ?, ?)`, snapshotID, layer.Name, layer.Identity, layer.ManifestDigest); err != nil {
			return Snapshot{}, fmt.Errorf("insert snapshot layer %q: %w", layer.Name, err)
		}
	}
	for _, change := range capture.Changes {
		if _, err := tx.ExecContext(ctx, `
INSERT INTO snapshot_changes (snapshot_id, status, base_path, target_path, base_digest, target_digest)
VALUES (?, ?, ?, ?, ?, ?)`, snapshotID, change.Status, change.BasePath, change.TargetPath,
			change.BaseDigest, change.TargetDigest); err != nil {
			return Snapshot{}, fmt.Errorf("insert snapshot change: %w", err)
		}
	}
	return persisted, nil
}

// GetSnapshot returns immutable snapshot metadata.
func (store *RepositoryStore) GetSnapshot(ctx context.Context, snapshotID string) (Snapshot, error) {
	if err := store.validate(); err != nil {
		return Snapshot{}, err
	}
	snapshotID = strings.TrimSpace(snapshotID)
	if snapshotID == "" {
		return Snapshot{}, fmt.Errorf("get snapshot: snapshot ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	row := store.database.QueryRowContext(ctx, `
SELECT id, repository_id, kind, requested_comparison, base_oid, effective_base_oid,
       target_oid, merge_base_oid, index_oid, object_format, context_policy_hash, ignore_policy,
       base_manifest_digest, target_manifest_digest, manifest_digest, complete, created_at
FROM snapshots WHERE id = ?`, snapshotID)
	result, err := scanSnapshot(row)
	if errors.Is(err, sql.ErrNoRows) {
		return Snapshot{}, fmt.Errorf("%w: %q", ErrSnapshotNotFound, snapshotID)
	}
	if err != nil {
		return Snapshot{}, fmt.Errorf("get snapshot %q: %w", snapshotID, err)
	}
	result.Layers, err = store.ListSnapshotLayers(ctx, snapshotID)
	if err != nil {
		return Snapshot{}, err
	}
	return result, nil
}

// ListSnapshotLayers reads immutable layer identities and manifest digests in
// deterministic layer order.
func (store *RepositoryStore) ListSnapshotLayers(ctx context.Context, snapshotID string) ([]SnapshotLayer, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	snapshotID = strings.TrimSpace(snapshotID)
	if snapshotID == "" {
		return nil, fmt.Errorf("list snapshot layers: snapshot ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT snapshot_id, layer, identity, manifest_digest
FROM snapshot_layers WHERE snapshot_id = ? ORDER BY layer ASC`, snapshotID)
	if err != nil {
		return nil, fmt.Errorf("list snapshot layers: %w", err)
	}
	defer rows.Close()
	layers := make([]SnapshotLayer, 0)
	for rows.Next() {
		var layer SnapshotLayer
		if err := rows.Scan(&layer.SnapshotID, &layer.Layer, &layer.Identity, &layer.ManifestDigest); err != nil {
			return nil, fmt.Errorf("list snapshot layers: %w", err)
		}
		layers = append(layers, layer)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list snapshot layers: %w", err)
	}
	return layers, nil
}

// ListSnapshotEntries reads a stored complete tree manifest in path order.
func (store *RepositoryStore) ListSnapshotEntries(
	ctx context.Context,
	snapshotID, treeSide string,
) ([]SnapshotEntry, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	snapshotID = strings.TrimSpace(snapshotID)
	treeSide = strings.TrimSpace(treeSide)
	if snapshotID == "" || treeSide == "" {
		return nil, fmt.Errorf("list snapshot entries: snapshot ID and tree side are required")
	}
	if treeSide != snapshot.TreeSideBase && treeSide != snapshot.TreeSideTarget &&
		treeSide != snapshot.TreeSideHead && treeSide != snapshot.TreeSideIndex && treeSide != snapshot.TreeSideWorktree {
		return nil, fmt.Errorf("list snapshot entries: unsupported tree side %q", treeSide)
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT snapshot_id, tree_side, path, kind, mode, size, content_digest, git_oid, symlink_target
FROM snapshot_entries WHERE snapshot_id = ? AND tree_side = ? ORDER BY path ASC`, snapshotID, treeSide)
	if err != nil {
		return nil, fmt.Errorf("list snapshot entries: %w", err)
	}
	defer rows.Close()
	entries := make([]SnapshotEntry, 0)
	for rows.Next() {
		entry, err := scanSnapshotEntry(rows)
		if err != nil {
			return nil, fmt.Errorf("list snapshot entries: %w", err)
		}
		entries = append(entries, entry)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list snapshot entries: %w", err)
	}
	return entries, nil
}

// ListSnapshotChanges reads the stored comparison without consulting Git.
func (store *RepositoryStore) ListSnapshotChanges(ctx context.Context, snapshotID string) ([]SnapshotChange, error) {
	if err := store.validate(); err != nil {
		return nil, err
	}
	snapshotID = strings.TrimSpace(snapshotID)
	if snapshotID == "" {
		return nil, fmt.Errorf("list snapshot changes: snapshot ID is empty")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	rows, err := store.database.QueryContext(ctx, `
SELECT snapshot_id, status, base_path, target_path, base_digest, target_digest
FROM snapshot_changes WHERE snapshot_id = ? ORDER BY base_path ASC, target_path ASC`, snapshotID)
	if err != nil {
		return nil, fmt.Errorf("list snapshot changes: %w", err)
	}
	defer rows.Close()
	changes := make([]SnapshotChange, 0)
	for rows.Next() {
		var change SnapshotChange
		if err := rows.Scan(
			&change.SnapshotID,
			&change.Status,
			&change.BasePath,
			&change.TargetPath,
			&change.BaseDigest,
			&change.TargetDigest,
		); err != nil {
			return nil, fmt.Errorf("list snapshot changes: %w", err)
		}
		changes = append(changes, change)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("list snapshot changes: %w", err)
	}
	return changes, nil
}

func captureKind(capture snapshot.Capture) string {
	if capture.ComparisonKind != "" {
		return capture.ComparisonKind
	}
	kind, err := snapshot.ComparisonKindForComparison(capture.RequestedComparison)
	if err == nil {
		return kind
	}
	return ""
}

func normalizeCapture(capture snapshot.Capture) (snapshot.Capture, error) {
	kind, err := snapshot.ComparisonKindForComparison(capture.RequestedComparison)
	if err != nil {
		return snapshot.Capture{}, err
	}
	if capture.ComparisonKind == "" {
		capture.ComparisonKind = kind
	} else if capture.ComparisonKind != kind {
		return snapshot.Capture{}, fmt.Errorf(
			"snapshot capture: comparison kind %q does not match %q",
			capture.ComparisonKind,
			capture.RequestedComparison,
		)
	}
	if capture.BaseOID == "" {
		capture.BaseOID = capture.EffectiveBaseOID
	}
	return capture, nil
}

func validateCapture(capture snapshot.Capture) error {
	if err := capture.Validate(); err != nil {
		return err
	}
	baseDigest, err := snapshot.ManifestDigest(capture.Base.Entries)
	if err != nil {
		return err
	}
	if baseDigest != capture.Base.ManifestDigest {
		return fmt.Errorf("snapshot capture: base manifest digest does not match entries")
	}
	targetDigest, err := snapshot.ManifestDigest(capture.Target.Entries)
	if err != nil {
		return err
	}
	if targetDigest != capture.Target.ManifestDigest {
		return fmt.Errorf("snapshot capture: target manifest digest does not match entries")
	}
	if capture.ComparisonKind == snapshot.ComparisonWorktree {
		for side, tree := range map[string]snapshot.TreeState{
			snapshot.TreeSideHead:     capture.Head,
			snapshot.TreeSideIndex:    capture.Index,
			snapshot.TreeSideWorktree: capture.Worktree,
		} {
			digest, digestErr := snapshot.ManifestDigest(tree.Entries)
			if digestErr != nil {
				return digestErr
			}
			if digest != tree.ManifestDigest {
				return fmt.Errorf("snapshot capture: %s manifest digest does not match entries", side)
			}
		}
	}
	manifestDigest, err := snapshot.OverallManifestDigest(capture)
	if err != nil {
		return err
	}
	if manifestDigest != capture.ManifestDigest {
		return fmt.Errorf("snapshot capture: manifest digest does not match capture")
	}
	return nil
}

func insertSnapshotEntriesTx(
	ctx context.Context,
	tx *sql.Tx,
	snapshotID, treeSide string,
	entries []snapshot.Entry,
) error {
	for _, entry := range entries {
		if _, err := tx.ExecContext(ctx, `
INSERT INTO snapshot_entries (
    snapshot_id, tree_side, path, kind, mode, size, content_digest, git_oid, symlink_target
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`, snapshotID, treeSide, entry.Path, entry.Kind, entry.Mode, entry.Size,
			entry.ContentDigest, entry.GitOID, entry.SymlinkTarget); err != nil {
			return fmt.Errorf("insert %s snapshot entry %q: %w", treeSide, entry.Path, err)
		}
	}
	return nil
}

func scanSnapshot(row scanner) (Snapshot, error) {
	var result Snapshot
	var complete int
	var createdAtRaw string
	if err := row.Scan(&result.ID, &result.RepositoryID, &result.Kind, &result.RequestedComparison,
		&result.BaseOID, &result.EffectiveBaseOID, &result.TargetOID, &result.MergeBaseOID, &result.IndexOID,
		&result.ObjectFormat, &result.ContextPolicyHash, &result.IgnorePolicy,
		&result.BaseManifestDigest, &result.TargetManifestDigest, &result.ManifestDigest,
		&complete, &createdAtRaw); err != nil {
		return Snapshot{}, err
	}
	result.Complete = complete == 1
	var err error
	result.CreatedAt, err = shared.ParseTimestamp(createdAtRaw)
	if err != nil {
		return Snapshot{}, fmt.Errorf("parse snapshot creation time: %w", err)
	}
	return result, nil
}

func scanSnapshotEntry(row scanner) (SnapshotEntry, error) {
	var entry SnapshotEntry
	if err := row.Scan(&entry.SnapshotID, &entry.TreeSide, &entry.Path, &entry.Kind, &entry.Mode,
		&entry.Size, &entry.ContentDigest, &entry.GitOID, &entry.SymlinkTarget); err != nil {
		return SnapshotEntry{}, err
	}
	return entry, nil
}
