package db

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
)

func TestVerificationPersistsRunEvidenceAndDerivedLaneAcrossRestart(t *testing.T) {
	t.Parallel()

	stateDir := t.TempDir()
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	clock := func() time.Time { return time.Date(2026, time.July, 15, 16, 0, 0, 0, time.UTC) }
	store := NewRepositoryStore(database, WithClock(clock))
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/verifier",
		DisplayName:       "verifier",
		DiscoveredGitDir:  "/workspaces/verifier/.git",
	}
	session, err := store.CreateSession(context.Background(), identity, "Verifier persistence")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	round, err := store.CreateRound(context.Background(), session.ID)
	if err != nil {
		t.Fatalf("CreateRound() error = %v", err)
	}
	change := review.ChangeModel{
		SchemaVersion:  "mire/v1/change-model",
		SessionID:      session.ID,
		SnapshotID:     "snapshot-1",
		SnapshotDigest: "manifest-1",
		Digest:         "change-1",
		Files: []review.FileChange{
			{
				Status:     "modified",
				TargetPath: "src/a.go",
				Patch:      "change",
				Hunks:      []review.Hunk{{ID: "src/a.go#hunk", Available: true, Digest: "hunk-1"}},
			},
		},
	}
	candidateResult, err := review.RunReviewPasses(
		context.Background(),
		change,
		&dbFixtureModel{
			output: []byte(
				`{"schema_version":"mire/v1/review-candidates","candidates":[{"claim":"claim","impact":"impact","category":"correctness","severity":"high","anchors":[{"hunk_id":"src/a.go#hunk"}]}]}`,
			),
		},
		review.ReviewerOpts{
			ModelRunOptions: review.ModelRunOptions{
				Retry: review.RetryPolicy{MaxAttempts: 1},
				Now:   clock,
			},
			RoundID: round.ID,
			Store:   store,
			Passes:  []review.PlannedPass{{Name: "correctness", Order: 0, Applicable: true, Reason: "fixture"}},
		},
	)
	if err != nil {
		t.Fatalf("RunReviewPasses() error = %v", err)
	}
	candidate := candidateResult.Candidates[0]
	artifacts, err := store.ListReviewArtifacts(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("ListReviewArtifacts() error = %v", err)
	}
	if len(artifacts) != 1 || artifacts[0].Content() != "change" {
		t.Fatalf("stored artifacts = %#v, want one artifact retaining private content", artifacts)
	}
	anchor := review.Anchor{
		SnapshotID: change.SnapshotID,
		Path:       "src/a.go",
		HunkID:     "src/a.go#hunk",
		HunkDigest: "hunk-1",
	}
	output := review.VerificationEnvelope{
		SchemaVersion:      review.VerificationSchemaVersion,
		State:              review.VerificationSupported,
		SuspectedInvariant: "The guard must reject invalid input.",
		ConcretePath: []review.VerificationPathStep{
			{
				EvidenceLocation: review.EvidenceLocation{
					Summary:        "Invalid input reaches the changed branch.",
					SnapshotID:     change.SnapshotID,
					Anchors:        []review.Anchor{anchor},
					ArtifactDigest: "path-digest",
				},
			},
		},
		SupportingEvidence: []review.Evidence{
			{
				EvidenceLocation: review.EvidenceLocation{
					SnapshotID:     change.SnapshotID,
					Anchors:        []review.Anchor{anchor},
					Summary:        "The changed branch lacks a rejecting guard.",
					ArtifactDigest: "source-digest",
				},
			},
		},
		RefutationAttempt: "Searched for a guard and a test that would disprove the path.",
	}
	verificationResult, err := review.VerifyCandidate(
		context.Background(),
		change,
		candidate,
		&dbFixtureModel{output: mustDBJSON(t, output)},
		review.VerifierOptions{
			ModelRunOptions: review.ModelRunOptions{
				Retry: review.RetryPolicy{MaxAttempts: 1},
				Now:   clock,
			},
			RoundID: round.ID,
			Store:   store,
		},
	)
	if err != nil {
		t.Fatalf("VerifyCandidate() error = %v", err)
	}
	if verificationResult.Lane != review.FindingLaneVerified {
		t.Fatalf("lane = %q, want verified", verificationResult.Lane)
	}
	loadedRun, err := store.GetVerificationRun(context.Background(), verificationResult.Run.ID)
	if err != nil {
		t.Fatalf("GetVerificationRun() error = %v", err)
	}
	if loadedRun.Status != review.RunStatusComplete || loadedRun.Provenance.OutputDigest == "" ||
		loadedRun.CandidateID != candidate.ID {
		t.Fatalf("loaded run = %#v", loadedRun)
	}
	loadedVerification, err := store.GetVerification(context.Background(), verificationResult.Verification.ID)
	if err != nil {
		t.Fatalf("GetVerification() error = %v", err)
	}
	if loadedVerification.Digest != verificationResult.Verification.Digest || len(loadedVerification.Evidence) == 0 {
		t.Fatalf("loaded verification = %#v", loadedVerification)
	}
	if _, err := store.GetLatestVerification(context.Background(), candidate.ID); err != nil {
		t.Fatalf("GetLatestVerification() error = %v", err)
	}

	loadedRun.Status = review.RunStatusComplete
	if err := store.UpdateVerificationRun(context.Background(), loadedRun); err == nil {
		t.Fatal("UpdateVerificationRun() error = nil, want immutable transition rejection")
	}
	if !errors.Is(
		func() error { _, err := store.GetVerification(context.Background(), "missing"); return err }(),
		ErrVerificationNotFound,
	) {
		t.Fatal("missing verification did not return ErrVerificationNotFound")
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
	verifications, err := restarted.ListVerifications(context.Background(), round.ID)
	if err != nil {
		t.Fatalf("ListVerifications() after restart error = %v", err)
	}
	if len(verifications) != 1 || verifications[0].State != review.VerificationSupported {
		t.Fatalf("verifications after restart = %#v", verifications)
	}
}

func mustDBJSON(t *testing.T, value any) []byte {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return data
}
