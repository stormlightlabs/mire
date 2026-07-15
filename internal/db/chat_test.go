package db

import (
	"context"
	"encoding/json"
	"path/filepath"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestContextBoundChatPersistsAcrossRestartAndRejectsUnscopedTurns(t *testing.T) {
	t.Parallel()

	stateDir := filepath.Join(t.TempDir(), "state")
	clock := func() time.Time { return time.Date(2026, time.July, 15, 19, 0, 0, 0, time.UTC) }
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatal(err)
	}
	store := NewRepositoryStore(database, WithClock(clock))
	identity := RepositoryIdentity{CanonicalIdentity: "/workspaces/chat", DisplayName: "chat", DiscoveredGitDir: "/workspaces/chat/.git"}
	session, round, frozen, err := store.CreateCapturedSession(context.Background(), identity, "Chat", testCapture(t))
	if err != nil {
		t.Fatal(err)
	}
	anchor := review.Anchor{SnapshotID: frozen.ID, Side: snapshot.TreeSideTarget, Layer: snapshot.TreeSideTarget,
		Path: "README.md", BlobDigest: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", HunkID: "README.md#1", HunkDigest: "hunk-1"}
	if err := store.RegisterDiffAnchors(context.Background(), round.ID, frozen.ID, []review.Anchor{anchor}); err != nil {
		t.Fatalf("RegisterDiffAnchors() error = %v", err)
	}
	request := review.ChatTurnRequest{SessionID: session.ID, RoundID: round.ID, SnapshotID: frozen.ID, Body: "Explain this change.", Context: review.ChatContext{References: []review.ChatReference{{DiffAnchor: &anchor}}}}
	model := &dbChatModel{response: review.ChatResponse{SchemaVersion: review.ChatResponseSchemaVersion, Body: "The change is bound to the captured README hunk."}}
	if _, err := store.SendChatTurn(context.Background(), request, model, review.ChatOptions{Retry: review.RetryPolicy{MaxAttempts: 1}, Now: clock}); err != nil {
		t.Fatalf("SendChatTurn() error = %v", err)
	}

	_, err = store.SendChatTurn(context.Background(), review.ChatTurnRequest{SessionID: session.ID, RoundID: round.ID, SnapshotID: frozen.ID, Body: "Unscoped", Context: review.ChatContext{}}, model, review.ChatOptions{Retry: review.RetryPolicy{MaxAttempts: 1}})
	if err == nil {
		t.Fatal("SendChatTurn() accepted an unscoped turn")
	}
	wrong := anchor
	wrong.HunkDigest = "wrong"
	_, err = store.SendChatTurn(context.Background(), review.ChatTurnRequest{SessionID: session.ID, RoundID: round.ID, SnapshotID: frozen.ID, Body: "Wrong anchor", Context: review.ChatContext{References: []review.ChatReference{{DiffAnchor: &wrong}}}}, model, review.ChatOptions{Retry: review.RetryPolicy{MaxAttempts: 1}})
	if err == nil {
		t.Fatalf("SendChatTurn() wrong anchor error = %v", err)
	}

	timeline, err := store.GetChatTimeline(context.Background(), session.ID)
	if err != nil || len(timeline.Messages) != 2 || len(timeline.Runs) != 1 {
		t.Fatalf("timeline before restart = %#v, error = %v", timeline, err)
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
	restartedTimeline, err := restarted.GetChatTimeline(context.Background(), session.ID)
	if err != nil || len(restartedTimeline.Messages) != 2 || len(restartedTimeline.Runs) != 1 {
		t.Fatalf("timeline after restart = %#v, error = %v", restartedTimeline, err)
	}
	if restartedTimeline.Runs[0].Binding.Context.Primary.Kind != review.ChatReferenceDiffAnchor || restartedTimeline.Messages[1].ReplyTo != restartedTimeline.Messages[0].ID {
		t.Fatalf("restarted binding/provenance = %#v / %#v", restartedTimeline.Runs[0].Binding, restartedTimeline.Messages[1])
	}
}

func TestFindingRevisionChatReferenceIsResolvedToActiveRound(t *testing.T) {
	t.Parallel()

	database, err := OpenState(context.Background(), filepath.Join(t.TempDir(), "state"))
	if err != nil {
		t.Fatal(err)
	}
	store := NewRepositoryStore(database)
	t.Cleanup(func() { _ = store.Close() })
	session, round, frozen, err := store.CreateCapturedSession(context.Background(), RepositoryIdentity{CanonicalIdentity: "/workspaces/chat-finding", DisplayName: "chat-finding", DiscoveredGitDir: "/workspaces/chat-finding/.git"}, "Chat finding", testCapture(t))
	if err != nil {
		t.Fatal(err)
	}
	change := review.ChangeModel{SchemaVersion: "mire/v1/change-model", SessionID: session.ID, SnapshotID: frozen.ID, SnapshotDigest: frozen.ManifestDigest,
		Digest: "change-digest", Files: []review.FileChange{{Status: "modified", TargetPath: "README.md", TargetDigest: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", Hunks: []review.Hunk{{ID: "README.md#1", Digest: "hunk-1", Available: true}}}}}
	candidate := review.CandidateRecord{ID: "candidate", RunID: "review-run", PassName: "correctness", Candidate: review.Candidate{Claim: "The README is misleading.", Impact: "Users may misunderstand the behavior.", Category: "documentation", Severity: "low", Anchors: []review.Anchor{{SnapshotID: frozen.ID, Side: snapshot.TreeSideTarget, Path: "README.md", HunkID: "README.md#1", HunkDigest: "hunk-1"}}}}
	finding, err := review.NewFindingRevision(change, candidate, round.ID, time.Now().UTC())
	if err != nil {
		t.Fatal(err)
	}
	finding, err = review.CorrelateFinding(nil, finding)
	if err != nil {
		t.Fatal(err)
	}
	if err := store.SaveFindingRevision(context.Background(), finding); err != nil {
		t.Fatal(err)
	}
	request := review.ChatTurnRequest{SessionID: session.ID, RoundID: round.ID, SnapshotID: frozen.ID, Body: "Challenge this finding.", Context: review.ChatContext{References: []review.ChatReference{{FindingRevision: &review.FindingRevisionRef{FindingID: finding.FindingID, Revision: finding.Revision}}}}}
	if _, err := store.SendChatTurn(context.Background(), request, &dbChatModel{response: review.ChatResponse{SchemaVersion: review.ChatResponseSchemaVersion, Body: "The finding is now scoped."}}, review.ChatOptions{Retry: review.RetryPolicy{MaxAttempts: 1}}); err != nil {
		t.Fatalf("finding-bound SendChatTurn() error = %v", err)
	}
}

type dbChatModel struct {
	response review.ChatResponse
}

func (model *dbChatModel) Complete(_ context.Context, _ review.ModelRequest) (review.ModelResponse, error) {
	data, err := json.Marshal(model.response)
	if err != nil {
		return review.ModelResponse{}, err
	}
	return review.ModelResponse{Output: data, FinishReason: "stop"}, nil
}
