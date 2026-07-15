package review

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestRunReviewPassesRetainsDuplicatesAndSeparatesIncompleteOutcomes(t *testing.T) {
	t.Parallel()

	change := reviewerFixtureChange()
	valid := CandidateEnvelope{SchemaVersion: ReviewCandidateSchemaVersion, Candidates: []Candidate{
		{SourceID: "same-emission", Claim: "The guard is bypassed.", Impact: "Invalid input reaches the changed branch.", Category: "correctness", Severity: "high", Confidence: 0.8,
			Anchors: []CandidateAnchor{{HunkID: "src/a.go#hunk"}}},
		{SourceID: "same-emission", Claim: "The guard is bypassed.", Impact: "Invalid input reaches the changed branch.", Category: "correctness", Severity: "high", Confidence: 0.8,
			Anchors: []CandidateAnchor{{HunkID: "src/a.go#hunk"}}},
	}}
	validJSON := mustJSON(t, valid)
	large := append([]byte(`{"schema_version":"mire/v1/review-candidates","candidates":[]}`), []byte(strings.Repeat(" ", 64))...)
	model := &reviewerFixtureModel{responses: []reviewerFixtureResponse{
		{output: mustJSON(t, CandidateEnvelope{SchemaVersion: ReviewCandidateSchemaVersion, Candidates: []Candidate{}})},
		{output: validJSON},
		{output: []byte(`{"schema_version":`)},
		{output: large},
		{output: []byte("The code looks good.")},
	}}
	passes := []PlannedPass{
		{Name: "empty", Order: 0, Applicable: true, Reason: "fixture"},
		{Name: "duplicates", Order: 1, Applicable: true, Reason: "fixture"},
		{Name: "failed", Order: 2, Applicable: true, Reason: "fixture"},
		{Name: "truncated", Order: 3, Applicable: true, Reason: "fixture"},
		{Name: "unsupported", Order: 4, Applicable: true, Reason: "fixture"},
		{Name: "skipped", Order: 5, Applicable: false, Reason: "not applicable"},
	}
	result, err := RunReviewPasses(context.Background(), change, model, ReviewerOpts{
		Retry: RetryPolicy{MaxAttempts: 1, RepairAttempts: 0}, Passes: passes,
		Budgets: map[string]PassBudget{"truncated": {MaxOutputBytes: 32}},
		Now:     func() time.Time { return time.Date(2026, time.July, 15, 12, 0, 0, 0, time.UTC) },
	})
	if err == nil {
		t.Fatal("RunReviewPasses() error = nil, want aggregate failed-pass error")
	}
	if len(result.Candidates) != 2 || result.Candidates[0].ID == result.Candidates[1].ID || result.Candidates[0].Fingerprint != result.Candidates[1].Fingerprint {
		t.Fatalf("retained candidates = %#v, want two distinct duplicate emissions", result.Candidates)
	}
	if len(result.Coverage.Passes) != len(passes) {
		t.Fatalf("coverage passes = %#v, want %d pass outcomes", result.Coverage.Passes, len(passes))
	}
	wantStatuses := map[string]ReviewPassStatus{
		"empty": ReviewPassCompleted, "duplicates": ReviewPassCompleted, "failed": ReviewPassFailed,
		"truncated": ReviewPassTruncated, "unsupported": ReviewPassUnsupported, "skipped": ReviewPassSkipped,
	}
	for _, pass := range result.Coverage.Passes {
		if pass.Status != wantStatuses[pass.Name] {
			t.Fatalf("pass %q status = %q, want %q", pass.Name, pass.Status, wantStatuses[pass.Name])
		}
	}
	if !hasDiagnostic(result.Diagnostics, DiagnosticMalformedCandidates) || !hasDiagnostic(result.Diagnostics, DiagnosticUnsupportedProse) || !hasDiagnostic(result.Diagnostics, DiagnosticOutputBudget) {
		t.Fatalf("diagnostics = %#v, want malformed, prose, and budget diagnostics", result.Diagnostics)
	}
}

func TestRunReviewPassesBoundsRetrievalAndRetainsItsAuditTrail(t *testing.T) {
	t.Parallel()

	change := reviewerFixtureChange()
	model := &reviewerFixtureModel{responses: []reviewerFixtureResponse{{output: mustJSON(t, CandidateEnvelope{SchemaVersion: ReviewCandidateSchemaVersion, Candidates: []Candidate{}})}}}
	retriever := SnapshotRetrieverFunc(func(_ context.Context, request RetrievalRequest) ([]RetrievedArtifact, error) {
		return []RetrievedArtifact{
			{Kind: "test", Path: "src/a_test.go", Relation: request.Relation, Content: "test context"},
			{Kind: "contract", Path: "api/schema.json", Relation: request.Relation, Content: "contract context"},
		}, nil
	})
	result, err := RunReviewPasses(context.Background(), change, model, ReviewerOpts{
		Retry: RetryPolicy{MaxAttempts: 1}, Passes: []PlannedPass{{Name: "tests", Order: 0, Applicable: true, Reason: "fixture"}},
		Retriever: retriever, Budgets: map[string]PassBudget{"tests": {MaxArtifacts: 2}},
		Now: func() time.Time { return time.Date(2026, time.July, 15, 13, 0, 0, 0, time.UTC) },
	})
	if err != nil {
		t.Fatalf("RunReviewPasses() error = %v", err)
	}
	if len(result.Coverage.RetrievedArtifacts) != 3 || result.Coverage.RetrievedArtifacts[1].Kind != "test" || !result.Coverage.RetrievedArtifacts[2].Excluded {
		t.Fatalf("retrieved artifacts = %#v, want changed code, included context, and excluded context", result.Coverage.RetrievedArtifacts)
	}
	if result.Coverage.Passes[0].Status != ReviewPassTruncated || len(result.Coverage.Exclusions) == 0 {
		t.Fatalf("coverage = %#v, want truncated pass and recorded exclusion", result.Coverage)
	}
	if result.Coverage.RetrievedArtifacts[0].RunID == "" || result.Coverage.RetrievedArtifacts[1].RunID == "" || result.Coverage.RetrievedArtifacts[1].Digest == "" {
		t.Fatalf("retrieval provenance = %#v", result.Coverage.RetrievedArtifacts)
	}
}

func TestRunReviewPassesInvalidCandidateIsDiagnosticNotFinding(t *testing.T) {
	t.Parallel()

	change := reviewerFixtureChange()
	model := &reviewerFixtureModel{responses: []reviewerFixtureResponse{{output: mustJSON(t, CandidateEnvelope{SchemaVersion: ReviewCandidateSchemaVersion, Candidates: []Candidate{
		{Claim: "valid", Impact: "impact", Category: "tests", Severity: "low", Anchors: []CandidateAnchor{{HunkID: "src/a.go#hunk"}}},
		{Claim: "missing anchor", Impact: "impact", Category: "tests", Severity: "low"},
	}})}}}
	result, err := RunReviewPasses(context.Background(), change, model, ReviewerOpts{Retry: RetryPolicy{MaxAttempts: 1}, Passes: []PlannedPass{{Name: "tests", Order: 0, Applicable: true}}})
	if err != nil {
		t.Fatalf("RunReviewPasses() error = %v", err)
	}
	if len(result.Candidates) != 1 || result.Coverage.Passes[0].CandidateCount != 1 {
		t.Fatalf("result = %#v, want one retained candidate", result)
	}
	if !hasDiagnostic(result.Diagnostics, DiagnosticInvalidCandidate) {
		t.Fatalf("diagnostics = %#v, want invalid candidate diagnostic", result.Diagnostics)
	}
}

func reviewerFixtureChange() ChangeModel {
	return ChangeModel{
		SchemaVersion: "mire/v1/change-model", SessionID: "session-1", SnapshotID: "snapshot-1", SnapshotDigest: "manifest-1", Digest: "change-1",
		Files: []FileChange{{Status: "modified", TargetPath: "src/a.go", Patch: "@@ -1 +1 @@\n-old\n+new\n", Hunks: []Hunk{{ID: "src/a.go#hunk", Available: true, Digest: "hunk-1"}}}},
	}
}

type reviewerFixtureResponse struct {
	output []byte
	err    error
}

type reviewerFixtureModel struct {
	responses []reviewerFixtureResponse
	calls     int
}

func (model *reviewerFixtureModel) Complete(_ context.Context, _ ModelRequest) (ModelResponse, error) {
	if model.calls >= len(model.responses) {
		return ModelResponse{}, errors.New("fixture response exhausted")
	}
	response := model.responses[model.calls]
	model.calls++
	return ModelResponse{Output: response.output, FinishReason: "stop"}, response.err
}

func mustJSON(t *testing.T, value any) []byte {
	t.Helper()
	data, err := CanonicalFixtureJSON(value)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func hasDiagnostic(diagnostics []ReviewDiagnostic, code string) bool {
	for _, diagnostic := range diagnostics {
		if diagnostic.Code == code {
			return true
		}
	}
	return false
}

// CanonicalFixtureJSON keeps fixture encoding local to this package without
// adding a second provider-facing JSON contract.
func CanonicalFixtureJSON(value any) ([]byte, error) {
	return json.Marshal(value)
}
