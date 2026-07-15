package db

import (
	"context"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

func TestReviewPassCandidatesAndCoveragePersistAcrossRestart(t *testing.T) {
	t.Parallel()

	stateDir := t.TempDir()
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	store := NewRepositoryStore(database, WithClock(func() time.Time {
		return time.Date(2026, time.July, 15, 14, 0, 0, 0, time.UTC)
	}))
	identity := RepositoryIdentity{CanonicalIdentity: "/workspaces/reviewer", DisplayName: "reviewer", DiscoveredGitDir: "/workspaces/reviewer/.git"}
	session, err := store.CreateSession(context.Background(), identity, "Reviewer persistence")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	round, err := store.CreateRound(context.Background(), session.ID)
	if err != nil {
		t.Fatalf("CreateRound() error = %v", err)
	}
	change := review.ChangeModel{
		SchemaVersion: "mire/v1/change-model", SessionID: session.ID, SnapshotID: "snapshot-1", SnapshotDigest: "manifest-1", Digest: "change-1",
		Files: []review.FileChange{{Status: "modified", TargetPath: "src/a.go", Patch: "change", Hunks: []review.Hunk{{ID: "src/a.go#hunk", Available: true}}}},
	}
	model := &dbFixtureModel{output: []byte(`{"schema_version":"mire/v1/review-candidates","candidates":[{"claim":"claim","impact":"impact","category":"correctness","severity":"high","anchors":[{"hunk_id":"src/a.go#hunk"}]}]}`)}
	result, err := review.RunReviewPasses(context.Background(), change, model, review.ReviewerOpts{
		Retry: review.RetryPolicy{MaxAttempts: 1}, RoundID: round.ID, Store: store,
		Passes: []review.PlannedPass{{Name: "correctness", Order: 0, Applicable: true, Reason: "fixture"}},
		Now:    func() time.Time { return time.Date(2026, time.July, 15, 14, 1, 0, 0, time.UTC) },
	})
	if err != nil {
		t.Fatalf("RunReviewPasses() error = %v", err)
	}
	if len(result.Candidates) != 1 {
		t.Fatalf("result candidates = %#v", result.Candidates)
	}
	coverage, err := store.GetReviewCoverage(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("GetReviewCoverage() error = %v", err)
	}
	if coverage.Passes[0].Status != review.ReviewPassCompleted {
		t.Fatalf("coverage = %#v", coverage)
	}
	candidates, err := store.ListReviewCandidates(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("ListReviewCandidates() error = %v", err)
	}
	if len(candidates) != 1 || candidates[0].Candidate.Claim != "claim" {
		t.Fatalf("candidates = %#v", candidates)
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
	restartedCoverage, err := restarted.GetReviewCoverage(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("GetReviewCoverage() after restart error = %v", err)
	}
	restartedCandidates, err := restarted.ListReviewCandidates(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("ListReviewCandidates() after restart error = %v", err)
	}
	if restartedCoverage.Digest != coverage.Digest || len(restartedCandidates) != 1 {
		t.Fatalf("restart coverage=%#v candidates=%#v", restartedCoverage, restartedCandidates)
	}
	passes, err := restarted.ListReviewPasses(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("ListReviewPasses() after restart error = %v", err)
	}
	if len(passes) != 1 || passes[0].Status != review.ReviewPassCompleted {
		t.Fatalf("passes after restart = %#v", passes)
	}
}

type dbFixtureModel struct {
	output []byte
}

func (model *dbFixtureModel) Complete(context.Context, review.ModelRequest) (review.ModelResponse, error) {
	return review.ModelResponse{Output: model.output, FinishReason: "stop"}, nil
}
