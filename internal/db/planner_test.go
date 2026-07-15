package db

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

func TestPlannerRunAndPlanPersistAcrossRestart(t *testing.T) {
	t.Parallel()

	stateDir := t.TempDir()
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	store := NewRepositoryStore(database, WithClock(func() time.Time {
		return time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC)
	}))
	identity := RepositoryIdentity{CanonicalIdentity: "/workspaces/mire", DisplayName: "mire", DiscoveredGitDir: "/workspaces/mire/.git"}
	session, err := store.CreateSession(context.Background(), identity, "Planner test")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	change := review.ChangeModel{
		SchemaVersion: "mire/v1/change-model", SessionID: session.ID, SnapshotID: "snapshot-1",
		SnapshotDigest: "manifest-digest", Digest: "change-model-digest",
		Files: []review.FileChange{{Status: "modified", TargetPath: "a.go", Hunks: []review.Hunk{{ID: "a.go#hunk", Available: true}}}},
	}
	result, err := review.RunPlanner(context.Background(), change, review.NewFixtureModel(change), review.PlannerOptions{
		Retry: review.RetryPolicy{MaxAttempts: 1}, Store: store, Adapter: "fixture", Protocol: "fixture/v1",
		Now: func() time.Time { return time.Date(2026, time.July, 14, 12, 1, 0, 0, time.UTC) },
	})
	if err != nil {
		t.Fatalf("RunPlanner() error = %v", err)
	}
	if result.Run.ID == "" || result.Plan == nil {
		t.Fatalf("result = %#v", result)
	}
	loadedRun, err := store.GetPlanRun(context.Background(), result.Run.ID)
	if err != nil {
		t.Fatalf("GetPlanRun() error = %v", err)
	}
	if loadedRun.Status != review.RunStatusComplete || loadedRun.Provenance.Adapter != "fixture" {
		t.Fatalf("loaded run = %#v", loadedRun)
	}
	loadedPlan, err := store.GetReviewPlan(context.Background(), result.Run.ID)
	if err != nil {
		t.Fatalf("GetReviewPlan() error = %v", err)
	}
	if loadedPlan.Digest != result.Plan.Digest || loadedPlan.ChangeModelDigest != change.Digest {
		t.Fatalf("loaded plan = %#v", loadedPlan)
	}
	if err := store.Close(); err != nil {
		t.Fatal(err)
	}

	restartedDB, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("reopen state: %v", err)
	}
	restarted := NewRepositoryStore(restartedDB)
	t.Cleanup(func() { _ = restarted.Close() })
	if _, err := restarted.GetPlanRun(context.Background(), result.Run.ID); err != nil {
		t.Fatalf("GetPlanRun() after restart error = %v", err)
	}
}

func TestPlannerPersistenceRejectsUnknownRunAndMissingSession(t *testing.T) {
	t.Parallel()

	database, err := OpenState(context.Background(), t.TempDir())
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	store := NewRepositoryStore(database)
	t.Cleanup(func() { _ = store.Close() })
	run := review.RunRecord{SessionID: "missing", Role: review.ModelRolePlanner, Status: review.RunStatusQueued, MaxAttempts: 1}
	if _, err := store.CreatePlanRun(context.Background(), run); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("CreatePlanRun() error = %v, want ErrSessionNotFound", err)
	}
	if _, err := store.GetPlanRun(context.Background(), "missing"); !errors.Is(err, ErrPlannerRunNotFound) {
		t.Fatalf("GetPlanRun() error = %v, want ErrPlannerRunNotFound", err)
	}
}
