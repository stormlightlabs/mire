package review

import (
	"encoding/json"
	"math"
	"reflect"
	"strings"
	"testing"
)

func TestEvidenceLocationKeepsFlattenedJSONAndHistoricalEvidenceOrder(t *testing.T) {
	t.Parallel()

	location := EvidenceLocation{
		Kind:           "source",
		Summary:        "The changed branch lacks a guard.",
		SnapshotID:     "snapshot-1",
		Anchors:        []Anchor{},
		ArtifactDigest: "artifact-1",
		OutputPointer:  "/supporting_evidence/0",
	}
	evidence := Evidence{
		ID: "evidence-1", EvidenceLocation: location, Relation: EvidenceSupports,
		ProducingRunID: "run-1", Independent: true, Concrete: true, Material: true,
	}
	wantEvidence := `{"id":"evidence-1","relation":"supports","snapshot_id":"snapshot-1","anchors":[],"summary":"The changed branch lacks a guard.","producing_run_id":"run-1","artifact_digest":"artifact-1","output_pointer":"/supporting_evidence/0","kind":"source","independent":true,"concrete":true,"material":true}`
	data, err := json.Marshal(evidence)
	if err != nil {
		t.Fatalf("json.Marshal(Evidence) error = %v", err)
	}
	if string(data) != wantEvidence {
		t.Fatalf("evidence JSON = %s, want %s", data, wantEvidence)
	}

	var decoded Evidence
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("json.Unmarshal(Evidence) error = %v", err)
	}
	if !reflect.DeepEqual(decoded, evidence) {
		t.Fatalf("decoded evidence = %#v, want %#v", decoded, evidence)
	}

	pathStep, err := json.Marshal(VerificationPathStep{EvidenceLocation: location})
	if err != nil {
		t.Fatalf("json.Marshal(VerificationPathStep) error = %v", err)
	}
	wantLocation := `{"kind":"source","summary":"The changed branch lacks a guard.","snapshot_id":"snapshot-1","anchors":[],"artifact_digest":"artifact-1","output_pointer":"/supporting_evidence/0"}`
	if string(pathStep) != wantLocation {
		t.Fatalf("path step JSON = %s, want %s", pathStep, wantLocation)
	}
}

func TestCandidateContentValidationIsSharedByReviewerAndChat(t *testing.T) {
	t.Parallel()

	anchor := Anchor{
		SnapshotID: "snapshot-1", Side: "target", Layer: "target", Path: "src/a.go",
		HunkID: "src/a.go#hunk", HunkDigest: "hunk-1",
	}
	valid := CandidateContent{
		Claim:      "The changed branch accepts invalid input.",
		Impact:     "Invalid input reaches a protected branch.",
		Category:   "correctness",
		Severity:   "high",
		Confidence: 0.8,
		Anchors:    []Anchor{anchor},
		Rationale:  "The guard is absent on the changed path.",
	}
	change := ChangeModel{
		SnapshotID: "snapshot-1",
		Files: []FileChange{{TargetPath: "src/a.go", Hunks: []Hunk{{
			ID: "src/a.go#hunk", Digest: "hunk-1",
		}}}},
	}
	if err := valid.validate("candidate"); err != nil {
		t.Fatalf("valid candidate content rejected: %v", err)
	}
	if _, err := (Candidate{CandidateContent: valid}).normalize(change); err != nil {
		t.Fatalf("reviewer candidate content rejected: %v", err)
	}
	binding := ChatBinding{
		ReviewScope: ReviewScope{SessionID: "session-1", RoundID: "round-1", SnapshotID: "snapshot-1"},
		Context:     ChatContext{References: []ChatReference{{DiffAnchor: &anchor}}},
	}
	if err := ValidateChatResponse(
		ChatResponse{
			SchemaVersion:     ChatResponseSchemaVersion,
			Body:              "A possible issue was found.",
			CandidateProposal: &ChatCandidateProposal{CandidateContent: valid},
		},
		binding,
	); err != nil {
		t.Fatalf("chat candidate content rejected: %v", err)
	}

	tests := []struct {
		name   string
		mutate func(*CandidateContent)
	}{
		{name: "claim", mutate: func(content *CandidateContent) { content.Claim = " " }},
		{name: "impact", mutate: func(content *CandidateContent) { content.Impact = " " }},
		{name: "category", mutate: func(content *CandidateContent) { content.Category = " " }},
		{name: "severity", mutate: func(content *CandidateContent) { content.Severity = "unknown" }},
		{name: "confidence", mutate: func(content *CandidateContent) { content.Confidence = math.NaN() }},
		{name: "anchors", mutate: func(content *CandidateContent) { content.Anchors = nil }},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			content := valid
			test.mutate(&content)
			if err := content.validate("candidate"); err == nil {
				t.Fatal("shared candidate validation accepted invalid content")
			}
			if _, err := (Candidate{CandidateContent: content}).normalize(change); err == nil {
				t.Fatal("reviewer candidate validation accepted invalid content")
			}
			response := ChatResponse{
				SchemaVersion: ChatResponseSchemaVersion,
				Body:          "A possible issue was found.",
				CandidateProposal: &ChatCandidateProposal{
					CandidateContent: content,
				},
			}
			if err := ValidateChatResponse(response, binding); err == nil {
				t.Fatal("chat candidate validation accepted invalid content")
			}
		})
	}
}

func TestReviewScopeEmbedsFlatRequiredFields(t *testing.T) {
	t.Parallel()

	scope := ReviewScope{SessionID: "session-1", RoundID: "round-1", SnapshotID: "snapshot-1"}
	tests := []struct {
		name   string
		value  any
		prefix string
	}{
		{
			name:   "run record",
			value:  RunRecord{ID: "run-1", ReviewScope: scope},
			prefix: `{"id":"run-1","session_id":"session-1","round_id":"round-1","snapshot_id":"snapshot-1"`,
		},
		{
			name:   "verification record",
			value:  VerificationRecord{ID: "verification-1", CandidateID: "candidate-1", ReviewScope: scope},
			prefix: `{"id":"verification-1","candidate_id":"candidate-1","session_id":"session-1","round_id":"round-1","snapshot_id":"snapshot-1"`,
		},
		{
			name:   "chat binding",
			value:  ChatBinding{ReviewScope: scope},
			prefix: `{"session_id":"session-1","round_id":"round-1","snapshot_id":"snapshot-1"`,
		},
		{
			name:   "chat message",
			value:  ChatMessage{SchemaVersion: ChatSchemaVersion, ID: "message-1", ReviewScope: scope},
			prefix: `{"schema_version":"mire/v1/chat-message","id":"message-1","session_id":"session-1","round_id":"round-1","snapshot_id":"snapshot-1"`,
		},
		{
			name:   "chat turn request",
			value:  ChatTurnRequest{ReviewScope: scope},
			prefix: `{"session_id":"session-1","round_id":"round-1","snapshot_id":"snapshot-1"`,
		},
		{
			name: "finding revision",
			value: FindingRevision{
				SchemaVersion: FindingSchemaVersion,
				FindingID:     "finding-1",
				Revision:      1,
				ReviewScope:   scope,
			},
			prefix: `{"schema_version":"mire/v1/finding-revision","finding_id":"finding-1","revision":1,"session_id":"session-1","round_id":"round-1","snapshot_id":"snapshot-1"`,
		},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			data, err := json.Marshal(test.value)
			if err != nil {
				t.Fatalf("json.Marshal() error = %v", err)
			}
			if !strings.HasPrefix(string(data), test.prefix) {
				t.Fatalf("JSON = %s, want prefix %s", data, test.prefix)
			}
			if strings.Contains(string(data), "ReviewScope") {
				t.Fatalf("JSON leaked embedded Go type: %s", data)
			}
		})
	}

	zeroScope, err := json.Marshal(ChatTurnRequest{ReviewScope: ReviewScope{SessionID: "session-1"}})
	if err != nil {
		t.Fatalf("json.Marshal(zero scope) error = %v", err)
	}
	if !strings.Contains(string(zeroScope), `"round_id":""`) ||
		!strings.Contains(string(zeroScope), `"snapshot_id":""`) {
		t.Fatalf("zero scope JSON omitted required fields: %s", zeroScope)
	}
}

func TestCandidateContentAndProposalJSONRemainFlat(t *testing.T) {
	t.Parallel()

	content := CandidateContent{
		Claim:    "claim",
		Impact:   "impact",
		Category: "category",
		Severity: "low",
		Anchors:  []Anchor{},
	}
	candidate, err := json.Marshal(Candidate{SourceID: "reviewer-1", CandidateContent: content})
	if err != nil {
		t.Fatalf("json.Marshal(Candidate) error = %v", err)
	}
	proposal, err := json.Marshal(ChatCandidateProposal{CandidateContent: content})
	if err != nil {
		t.Fatalf("json.Marshal(ChatCandidateProposal) error = %v", err)
	}
	wantCandidate := `{"source_id":"reviewer-1","claim":"claim","impact":"impact","category":"category","severity":"low","anchors":[]}`
	wantProposal := `{"claim":"claim","impact":"impact","category":"category","severity":"low","anchors":[]}`
	if string(candidate) != wantCandidate || string(proposal) != wantProposal {
		t.Fatalf("candidate JSON = %s and proposal JSON = %s", candidate, proposal)
	}
}
