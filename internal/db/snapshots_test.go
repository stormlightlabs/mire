package db

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestCreateCapturedSessionPersists(t *testing.T) {
	t.Parallel()

	stateDir := t.TempDir()
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	capture := testCapture(t)
	ids := []string{"repository-id", "session-id", "snapshot-id", "round-id"}
	store := NewRepositoryStore(database, WithIDGenerator(func() (string, error) {
		if len(ids) == 0 {
			return "", errors.New("test IDs exhausted")
		}
		id := ids[0]
		ids = ids[1:]
		return id, nil
	}))
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/snapshot",
		DisplayName:       "snapshot",
		DiscoveredGitDir:  "/workspaces/snapshot/.git",
	}
	session, round, persisted, err := store.CreateCapturedSession(context.Background(), identity, "Captured review", capture)
	if err != nil {
		t.Fatalf("CreateCapturedSession() error = %v", err)
	}
	if session.ID != "session-id" || session.CurrentRoundID != "round-id" {
		t.Fatalf("session = %#v", session)
	}
	if round.SnapshotID != "snapshot-id" || round.Status != RoundStatusPending {
		t.Fatalf("round = %#v", round)
	}
	if !persisted.Complete || persisted.ManifestDigest != capture.ManifestDigest {
		t.Fatalf("snapshot = %#v", persisted)
	}

	loadedRound, err := store.GetRound(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("GetRound() error = %v", err)
	}
	if loadedRound.SnapshotID != persisted.ID {
		t.Fatalf("loaded round snapshot ID = %q, want %q", loadedRound.SnapshotID, persisted.ID)
	}
	baseEntries, err := store.ListSnapshotEntries(context.Background(), persisted.ID, snapshot.TreeSideBase)
	if err != nil {
		t.Fatalf("ListSnapshotEntries(base) error = %v", err)
	}
	targetEntries, err := store.ListSnapshotEntries(context.Background(), persisted.ID, snapshot.TreeSideTarget)
	if err != nil {
		t.Fatalf("ListSnapshotEntries(target) error = %v", err)
	}
	if len(baseEntries) != 2 || len(targetEntries) != 2 || baseEntries[0].Path != "README.md" {
		t.Fatalf("stored entries = %#v/%#v", baseEntries, targetEntries)
	}
	changes, err := store.ListSnapshotChanges(context.Background(), persisted.ID)
	if err != nil {
		t.Fatalf("ListSnapshotChanges() error = %v", err)
	}
	if len(changes) != len(capture.Changes) || changes[0].Status != snapshot.ChangeModified {
		t.Fatalf("stored changes = %#v", changes)
	}

	if _, _, _, err := store.CreateCapturedSession(context.Background(), identity, "Should roll back", capture); err == nil {
		t.Fatal("duplicate captured session succeeded, want transaction failure")
	}
	var sessions, snapshots int
	if err := database.QueryRow("SELECT COUNT(*) FROM sessions").Scan(&sessions); err != nil {
		t.Fatalf("count sessions: %v", err)
	}
	if err := database.QueryRow("SELECT COUNT(*) FROM snapshots").Scan(&snapshots); err != nil {
		t.Fatalf("count snapshots: %v", err)
	}
	if sessions != 1 || snapshots != 1 {
		t.Fatalf("counts after rollback = sessions %d snapshots %d, want 1/1", sessions, snapshots)
	}
	if err := store.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestAppendCapturedRoundPreserves(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	database, err := OpenState(ctx, t.TempDir())
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	defer database.Close()
	ids := []string{"repository-id", "session-id", "snapshot-one", "round-one", "snapshot-two", "round-two", "cancel-operation"}
	store := NewRepositoryStore(database, WithIDGenerator(func() (string, error) {
		if len(ids) == 0 {
			return "", errors.New("test IDs exhausted")
		}
		id := ids[0]
		ids = ids[1:]
		return id, nil
	}))
	identity := RepositoryIdentity{CanonicalIdentity: "/workspaces/append", DisplayName: "append", DiscoveredGitDir: "/workspaces/append/.git"}
	firstCapture := testCapture(t)
	session, firstRound, _, err := store.CreateCapturedSession(ctx, identity, "Review", firstCapture)
	if err != nil {
		t.Fatalf("CreateCapturedSession() error = %v", err)
	}

	secondCapture := testCapture(t)
	secondCapture.CapturedAt = secondCapture.CapturedAt.Add(time.Minute)
	secondCapture.TargetOID = "target-two"
	secondCapture.TargetManifestDigest, err = snapshot.ManifestDigest(secondCapture.TargetEntries)
	if err != nil {
		t.Fatalf("ManifestDigest() error = %v", err)
	}
	secondCapture.ManifestDigest, err = snapshot.OverallManifestDigest(secondCapture)
	if err != nil {
		t.Fatalf("OverallManifestDigest() error = %v", err)
	}
	appended, secondRound, secondSnapshot, err := store.AppendCapturedRound(ctx, session.ID, identity, secondCapture)
	if err != nil {
		t.Fatalf("AppendCapturedRound() error = %v", err)
	}
	if appended.CurrentRoundID != secondRound.ID || secondRound.Number != 2 ||
		secondRound.PredecessorRoundID != firstRound.ID || secondSnapshot.ID != "snapshot-two" {
		t.Fatalf("appended session/round/snapshot = %#v/%#v/%#v", appended, secondRound, secondSnapshot)
	}
	loadedFirst, err := store.GetRound(ctx, firstRound.ID)
	if err != nil {
		t.Fatalf("GetRound(first) error = %v", err)
	}
	if loadedFirst.SnapshotID != "snapshot-one" || loadedFirst.PredecessorRoundID != "" {
		t.Fatalf("first round was mutated = %#v", loadedFirst)
	}
	rounds, err := store.ListRounds(ctx, session.ID)
	if err != nil {
		t.Fatalf("ListRounds() error = %v", err)
	}
	if len(rounds) != 2 || rounds[0].Number != 1 || rounds[1].Number != 2 {
		t.Fatalf("round history = %#v", rounds)
	}
	operation, err := store.CreateOperation(ctx, session.ID, secondRound.ID, OperationKindReview)
	if err != nil {
		t.Fatalf("CreateOperation() error = %v", err)
	}
	if _, err := store.CancelOperation(ctx, operation.ID); err != nil {
		t.Fatalf("CancelOperation() error = %v", err)
	}
	restoredSession, err := store.GetSession(ctx, session.ID)
	if err != nil {
		t.Fatalf("GetSession() after cancellation error = %v", err)
	}
	if restoredSession.CurrentRoundID != firstRound.ID {
		t.Fatalf("current round after cancellation = %q, want predecessor %q", restoredSession.CurrentRoundID, firstRound.ID)
	}

	other := identity
	other.CanonicalIdentity = "/workspaces/other"
	other.DiscoveredGitDir = "/workspaces/other/.git"
	if _, _, _, err := store.AppendCapturedRound(ctx, session.ID, other, firstCapture); !errors.Is(err, ErrSessionRepositoryMismatch) {
		t.Fatalf("AppendCapturedRound(other repository) error = %v, want ErrSessionRepositoryMismatch", err)
	}
	var snapshots int
	if err := database.QueryRow(`SELECT COUNT(*) FROM snapshots`).Scan(&snapshots); err != nil {
		t.Fatalf("count snapshots: %v", err)
	}
	if snapshots != 2 {
		t.Fatalf("snapshots after rejected append = %d, want 2", snapshots)
	}
}

func testCapture(t *testing.T) snapshot.Capture {
	t.Helper()
	baseEntries := []snapshot.Entry{
		{Path: "README.md", Kind: snapshot.EntryKindFile, Mode: 0o100644, Size: 4,
			ContentDigest: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", GitOID: "base-readme"},
		{Path: "old.txt", Kind: snapshot.EntryKindFile, Mode: 0o100644, Size: 3,
			ContentDigest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", GitOID: "base-old"},
	}
	targetEntries := []snapshot.Entry{
		{Path: "README.md", Kind: snapshot.EntryKindFile, Mode: 0o100644, Size: 5,
			ContentDigest: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", GitOID: "target-readme"},
		{Path: "new.txt", Kind: snapshot.EntryKindFile, Mode: 0o100644, Size: 3,
			ContentDigest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", GitOID: "target-old"},
	}
	capture := snapshot.Capture{
		RequestedComparison: "base..target",
		EffectiveBaseOID:    "base-oid",
		TargetOID:           "target-oid",
		ObjectFormat:        "sha1",
		ContextPolicyHash:   snapshot.DefaultContextPolicyHash(),
		CapturedAt:          time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC),
		BaseEntries:         baseEntries,
		TargetEntries:       targetEntries,
		Changes:             snapshot.BuildChanges(baseEntries, targetEntries),
	}
	var err error
	capture.BaseManifestDigest, err = snapshot.ManifestDigest(baseEntries)
	if err != nil {
		t.Fatal(err)
	}
	capture.TargetManifestDigest, err = snapshot.ManifestDigest(targetEntries)
	if err != nil {
		t.Fatal(err)
	}
	capture.ManifestDigest, err = snapshot.OverallManifestDigest(capture)
	if err != nil {
		t.Fatal(err)
	}
	return capture
}
