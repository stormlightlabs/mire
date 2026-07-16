package review

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"
)

func TestRetrievedArtifactMetadataAndPrivateContentJSON(t *testing.T) {
	t.Parallel()

	metadata := RetrievedArtifactMetadata{
		ID:              "artifact-1",
		RunID:           "run-1",
		PassName:        "correctness",
		Kind:            "source",
		Path:            "src/a.go",
		Relation:        "related_to_changed_code",
		HunkIDs:         []string{"hunk-1", "hunk-2"},
		Digest:          "digest-1",
		Size:            17,
		Excluded:        true,
		ExclusionReason: "artifact budget",
		Truncated:       true,
	}
	wantJSON := `{"id":"artifact-1","run_id":"run-1","pass_name":"correctness","kind":"source","path":"src/a.go","relation":"related_to_changed_code","hunk_ids":["hunk-1","hunk-2"],"digest":"digest-1","size":17,"excluded":true,"exclusion_reason":"artifact budget","truncated":true}`

	metadataJSON, err := json.Marshal(metadata)
	if err != nil {
		t.Fatalf("json.Marshal(metadata) error = %v", err)
	}
	if string(metadataJSON) != wantJSON {
		t.Fatalf("metadata JSON = %s, want %s", metadataJSON, wantJSON)
	}

	artifact := NewRetrievedArtifact(metadata, "private source bytes")
	artifactJSON, err := json.Marshal(artifact)
	if err != nil {
		t.Fatalf("json.Marshal(artifact) error = %v", err)
	}
	if string(artifactJSON) != wantJSON || strings.Contains(string(artifactJSON), "private source bytes") {
		t.Fatalf("artifact JSON = %s, want metadata only", artifactJSON)
	}
	if artifact.Content() != "private source bytes" {
		t.Fatalf("artifact content = %q", artifact.Content())
	}

	var decoded RetrievedArtifact
	if err := json.Unmarshal(artifactJSON, &decoded); err != nil {
		t.Fatalf("json.Unmarshal(artifact) error = %v", err)
	}
	if !reflect.DeepEqual(decoded.Metadata(), metadata) {
		t.Fatalf("decoded metadata = %#v, want %#v", decoded.Metadata(), metadata)
	}
	if decoded.Content() != "" {
		t.Fatalf("decoded artifact unexpectedly retained content %q", decoded.Content())
	}

	metadata.HunkIDs[0] = "caller-mutated"
	if artifact.Metadata().HunkIDs[0] != "hunk-1" {
		t.Fatal("artifact reused caller-owned hunk IDs")
	}
	returned := artifact.Metadata()
	returned.HunkIDs[0] = "returned-mutated"
	if artifact.Metadata().HunkIDs[0] != "hunk-1" {
		t.Fatal("artifact metadata returned its owned hunk IDs")
	}
}

func TestRetrievedArtifactMetadataZeroJSONPreservesRequiredFields(t *testing.T) {
	t.Parallel()

	data, err := json.Marshal(RetrievedArtifactMetadata{})
	if err != nil {
		t.Fatalf("json.Marshal(zero metadata) error = %v", err)
	}
	want := `{"id":"","run_id":"","pass_name":"","kind":"","relation":"","digest":"","size":0}`
	if string(data) != want {
		t.Fatalf("zero metadata JSON = %s, want %s", data, want)
	}
}

func TestRunRecordCompositionKeepsVerifierFlatAndOperationalRunsNested(t *testing.T) {
	t.Parallel()

	run := RunRecord{
		ID:          "run-1",
		ReviewScope: ReviewScope{SessionID: "session-1", RoundID: "round-1", SnapshotID: "snapshot-1"},
		Role:        ModelRoleReviewer,
		Status:      RunStatusQueued,
		Provenance:  RunProvenance{InputManifestDigest: "manifest-1", InputDigest: "input-1"},
	}

	data, err := json.Marshal(VerificationRunRecord{RunRecord: run, CandidateID: "candidate-1"})
	if err != nil {
		t.Fatalf("json.Marshal(verification run) error = %v", err)
	}
	var verifierJSON map[string]json.RawMessage
	if err := json.Unmarshal(data, &verifierJSON); err != nil {
		t.Fatalf("decode verification run JSON: %v", err)
	}
	if _, ok := verifierJSON["run"]; ok {
		t.Fatalf("verification run unexpectedly nested: %s", data)
	}
	if _, ok := verifierJSON["candidate_id"]; !ok {
		t.Fatalf("verification run lost candidate_id: %s", data)
	}

	chatData, err := json.Marshal(ChatRunRecord{Run: run})
	if err != nil {
		t.Fatalf("json.Marshal(chat run) error = %v", err)
	}
	var chatJSON map[string]json.RawMessage
	if err := json.Unmarshal(chatData, &chatJSON); err != nil {
		t.Fatalf("decode chat run JSON: %v", err)
	}
	if _, ok := chatJSON["run"]; !ok {
		t.Fatalf("chat run lost nested run: %s", chatData)
	}

	passData, err := json.Marshal(ReviewPassResult{Run: run})
	if err != nil {
		t.Fatalf("json.Marshal(pass result) error = %v", err)
	}
	var passJSON map[string]json.RawMessage
	if err := json.Unmarshal(passData, &passJSON); err != nil {
		t.Fatalf("decode pass result JSON: %v", err)
	}
	if _, ok := passJSON["run"]; !ok {
		t.Fatalf("pass result lost nested run: %s", passData)
	}

	verificationData, err := json.Marshal(VerificationResult{
		Run: VerificationRunRecord{RunRecord: run, CandidateID: "candidate-1"},
	})
	if err != nil {
		t.Fatalf("json.Marshal(verification result) error = %v", err)
	}
	var verificationResultJSON map[string]json.RawMessage
	if err := json.Unmarshal(verificationData, &verificationResultJSON); err != nil {
		t.Fatalf("decode verification result JSON: %v", err)
	}
	if _, ok := verificationResultJSON["run"]; !ok {
		t.Fatalf("verification result lost nested run: %s", verificationData)
	}
}
