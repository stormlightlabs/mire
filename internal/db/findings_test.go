package db

import (
	"context"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

func TestFindingLedgerPersistence(t *testing.T) {
	t.Parallel()

	stateDir := t.TempDir()
	clock := func() time.Time { return time.Date(2026, time.July, 15, 18, 0, 0, 0, time.UTC) }
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatal(err)
	}
	store := NewRepositoryStore(database, WithClock(clock))
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/finding-ledger",
		DisplayName:       "finding-ledger",
		DiscoveredGitDir:  "/workspaces/finding-ledger/.git",
	}
	session, err := store.CreateSession(context.Background(), identity, "Finding ledger")
	if err != nil {
		t.Fatal(err)
	}
	round, err := store.CreateRound(context.Background(), session.ID)
	if err != nil {
		t.Fatal(err)
	}
	change := review.ChangeModel{
		SchemaVersion:  "mire/v1/change-model",
		SessionID:      session.ID,
		SnapshotID:     "snapshot-1",
		SnapshotDigest: "manifest-1",
		Digest:         "change-1",
		Files: []review.FileChange{
			{
				Status:       "modified",
				TargetPath:   "src/a.go",
				TargetDigest: "blob-1",
				Hunks: []review.Hunk{
					{
						ID:        "src/a.go#hunk",
						Digest:    "hunk-1",
						Available: true,
						Lines:     []string{"@@ -1 +1 @@\n", "-old\n", "+new\n"},
					},
				},
			},
		},
	}
	candidate := review.CandidateRecord{
		ID:       "candidate-1",
		RunID:    "review-run-1",
		PassName: "correctness",
		Ordinal:  0,
		Candidate: review.Candidate{
			Claim:      "The changed branch accepts invalid input.",
			Impact:     "Invalid input reaches a state that assumes the guard ran.",
			Category:   "correctness",
			Severity:   "high",
			Confidence: 0.75,
			Anchors: []review.Anchor{
				{
					SnapshotID: change.SnapshotID,
					Side:       "target",
					Path:       "src/a.go",
					HunkID:     "src/a.go#hunk",
					HunkDigest: "hunk-1",
				},
			},
		},
	}
	finding, err := review.NewFindingRevision(change, candidate, round.ID, clock)
	if err != nil {
		t.Fatal(err)
	}
	finding, err = review.CorrelateFinding(nil, finding)
	if err != nil {
		t.Fatal(err)
	}
	if err := store.SaveFindingRevision(context.Background(), finding); err != nil {
		t.Fatalf("SaveFindingRevision() error = %v", err)
	}

	loaded, err := store.GetFindingRevision(context.Background(), finding.FindingID, finding.Revision)
	if err != nil {
		t.Fatal(err)
	}
	if loaded.Digest != finding.Digest || loaded.Anchors[0].HunkDigest != "hunk-1" {
		t.Fatalf("loaded finding = %#v", loaded)
	}

	if err := store.SaveDisposition(context.Background(), review.DispositionRecord{
		FindingID: finding.FindingID, Revision: finding.Revision, Disposition: review.FindingDispositionAccepted,
	}); err != nil {
		t.Fatalf("SaveDisposition(accepted) error = %v", err)
	}
	if err := store.SaveDisposition(context.Background(), review.DispositionRecord{
		FindingID: finding.FindingID, Revision: finding.Revision, Disposition: review.FindingDispositionAcceptedRisk,
		Rationale: "The risk is bounded by an external control.",
	}); err != nil {
		t.Fatalf("AppendDisposition(accepted risk) error = %v", err)
	}
	current, err := store.GetCurrentDisposition(context.Background(), finding.FindingID)
	if err != nil {
		t.Fatal(err)
	}
	if current.Disposition != review.FindingDispositionAcceptedRisk || current.Rationale == "" {
		t.Fatalf("current disposition = %#v", current)
	}

	if err := store.SavePresentation(
		context.Background(),
		review.PresentationRecord{FindingID: finding.FindingID, Body: "Reject invalid input."},
	); err != nil {
		t.Fatalf("SavePresentation(first) error = %v", err)
	}
	if err := store.SavePresentation(
		context.Background(),
		review.PresentationRecord{
			FindingID: finding.FindingID,
			Body:      "Reject invalid input before the changed branch.",
		},
	); err != nil {
		t.Fatalf("SaveCommentRevision(second) error = %v", err)
	}
	presentations, err := store.ListPresentations(context.Background(), finding.FindingID)
	if err != nil {
		t.Fatal(err)
	}
	if len(presentations) != 2 || presentations[0].Version != 1 || presentations[1].Version != 2 {
		t.Fatalf("presentations = %#v", presentations)
	}

	mutated := finding
	mutated.Claim = "changed after persistence"
	mutated.Digest = review.FindingRevisionDigest(mutated)
	if err := store.SaveFindingRevision(context.Background(), mutated); err == nil {
		t.Fatal("SaveFindingRevision() succeeded when rewriting an immutable revision")
	}
	if loadedAgain, err := store.GetFindingRevision(
		context.Background(),
		finding.FindingID,
		finding.Revision,
	); err != nil ||
		loadedAgain.Claim != finding.Claim {
		t.Fatalf("finding was changed by rejected rewrite: %#v, %v", loadedAgain, err)
	}

	if err := store.Close(); err != nil {
		t.Fatal(err)
	}
	restartedDB, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatal(err)
	}
	restarted := NewRepositoryStore(restartedDB)
	t.Cleanup(func() { _ = restarted.Close() })
	restartedFindings, err := restarted.ListFindingRevisions(context.Background(), round.ID)
	if err != nil {
		t.Fatal(err)
	}
	if len(restartedFindings) != 1 || restartedFindings[0].FindingID != finding.FindingID {
		t.Fatalf("findings after restart = %#v", restartedFindings)
	}
	restartedPresentation, err := restarted.GetLatestPresentation(context.Background(), finding.FindingID)
	if err != nil {
		t.Fatal(err)
	}
	if restartedPresentation.Version != 2 {
		t.Fatalf("latest presentation after restart = %#v", restartedPresentation)
	}
}
