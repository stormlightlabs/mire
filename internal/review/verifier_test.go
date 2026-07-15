package review

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestVerifyCandidateDerivesEvidenceLanes(t *testing.T) {
	t.Parallel()

	change := verifierFixtureChange()
	candidate := verifierFixtureCandidate()
	valid := verifierFixtureEnvelope(VerificationSupported)
	result, err := VerifyCandidate(context.Background(), change, candidate, verifierFixtureModel{output: mustJSON(t, valid)}, VerifierOptions{
		Retry: RetryPolicy{MaxAttempts: 1},
		Now:   func() time.Time { return time.Date(2026, time.July, 15, 15, 0, 0, 0, time.UTC) },
	})
	if err != nil {
		t.Fatalf("VerifyCandidate() error = %v", err)
	}
	if result.Lane != FindingLaneVerified || !EvidenceFloorSatisfied(change, candidate, result.Verification) {
		t.Fatalf("result = %#v, want verified lane", result)
	}
	if result.Verification.Evidence[0].OutputPointer == "" || result.Verification.Evidence[0].ProducingRunID != result.Run.ID {
		t.Fatalf("evidence provenance = %#v", result.Verification.Evidence)
	}

	unsupported := valid
	unsupported.SupportingEvidence = nil
	candidateResult, err := VerifyCandidate(context.Background(), change, candidate, verifierFixtureModel{output: mustJSON(t, unsupported)}, VerifierOptions{
		Retry: RetryPolicy{MaxAttempts: 1},
	})
	if err != nil {
		t.Fatalf("VerifyCandidate() without support error = %v", err)
	}
	if candidateResult.Lane != FindingLaneCandidate {
		t.Fatalf("lane = %q, want candidate", candidateResult.Lane)
	}

	refuted, err := VerifyCandidate(context.Background(), change, candidate, verifierFixtureModel{output: mustJSON(t, verifierFixtureEnvelope(VerificationRefuted))}, VerifierOptions{
		Retry: RetryPolicy{MaxAttempts: 1},
	})
	if err != nil {
		t.Fatalf("refuted VerifyCandidate() error = %v", err)
	}
	if refuted.Lane != FindingLaneRefuted {
		t.Fatalf("refuted lane = %q, want refuted", refuted.Lane)
	}
}

func TestVerifyCandidateKeepsBlockedWorkAuditable(t *testing.T) {
	t.Parallel()

	change := verifierFixtureChange()
	result, err := VerifyCandidate(context.Background(), change, verifierFixtureCandidate(), nil, VerifierOptions{
		Retry: RetryPolicy{MaxAttempts: 1},
	})
	if err == nil {
		t.Fatal("VerifyCandidate() error = nil, want blocked model error")
	}
	var verificationErr *VerificationError
	if !errors.As(err, &verificationErr) || verificationErr.State != VerificationBlocked || result.Verification.State != VerificationBlocked {
		t.Fatalf("error=%v result=%#v, want blocked verification", err, result)
	}
	if result.Lane != FindingLaneCandidate || result.Run.Status != RunStatusFailed {
		t.Fatalf("result = %#v, want failed run and candidate lane", result)
	}
}

func TestVerificationStateTransitionsAreAppendOnly(t *testing.T) {
	t.Parallel()

	for _, next := range []VerificationState{VerificationSupported, VerificationInconclusive, VerificationRefuted, VerificationBlocked} {
		if err := ValidateVerificationTransition(VerificationNotRun, next); err != nil {
			t.Errorf("not_run -> %s: %v", next, err)
		}
	}
	for _, current := range []VerificationState{VerificationSupported, VerificationInconclusive, VerificationRefuted, VerificationBlocked} {
		if current.CanTransitionTo(VerificationSupported) {
			t.Errorf("%s unexpectedly permits a rewrite", current)
		}
	}
	if err := ValidateVerificationTransition(VerificationNotRun, VerificationNotRun); err == nil {
		t.Fatal("not_run -> not_run unexpectedly accepted")
	}
}

func TestEvidenceFloorIgnoresConfidenceAndAnalyzerSignals(t *testing.T) {
	t.Parallel()

	change := verifierFixtureChange()
	candidate := verifierFixtureCandidate()
	candidate.Candidate.Confidence = 0
	record := VerificationRecord{
		CandidateID: candidate.ID, SnapshotID: change.SnapshotID, RunID: "verify-run", State: VerificationSupported,
		SuspectedInvariant: "the invariant is violated", RefutationAttempt: "Tried the valid-input and guard paths.",
		ConcretePath: []VerificationPathStep{{Summary: "Input reaches the changed branch.", SnapshotID: change.SnapshotID, Anchors: candidate.Candidate.Anchors, ArtifactDigest: "path-digest"}},
		Evidence:     []Evidence{{Relation: EvidenceSupports, SnapshotID: change.SnapshotID, Anchors: candidate.Candidate.Anchors, Summary: "The guard is absent on the changed path.", ProducingRunID: "verify-run", ArtifactDigest: "source-digest", Independent: true, Concrete: true}},
	}
	if err := ValidateEvidenceFloor(change, candidate, record); err != nil {
		t.Fatalf("ValidateEvidenceFloor() error = %v", err)
	}
	record.State = VerificationInconclusive
	if EvidenceFloorSatisfied(change, candidate, record) {
		t.Fatal("inconclusive verification satisfied the floor")
	}
}

func TestValidateEvidenceFloorBoundaries(t *testing.T) {
	t.Parallel()

	change := verifierFixtureChange()
	candidate := verifierFixtureCandidate()
	valid := VerificationRecord{
		CandidateID: candidate.ID, SnapshotID: change.SnapshotID, RunID: "verify-run", State: VerificationSupported,
		SuspectedInvariant: "The invariant is violated.", RefutationAttempt: "Tried to refute the path.",
		ConcretePath: []VerificationPathStep{{Summary: "The input reaches the branch.", SnapshotID: change.SnapshotID, Anchors: candidate.Candidate.Anchors, ArtifactDigest: "path-digest"}},
		Evidence:     []Evidence{{Relation: EvidenceSupports, SnapshotID: change.SnapshotID, Anchors: candidate.Candidate.Anchors, Summary: "The guard is absent.", ProducingRunID: "verify-run", ArtifactDigest: "source-digest", Independent: true, Concrete: true}},
	}
	tests := []struct {
		name   string
		mutate func(*CandidateRecord, *VerificationRecord)
	}{
		{name: "wrong state", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.State = VerificationInconclusive }},
		{name: "missing claim", mutate: func(candidate *CandidateRecord, _ *VerificationRecord) { candidate.Candidate.Claim = "" }},
		{name: "missing impact", mutate: func(candidate *CandidateRecord, _ *VerificationRecord) { candidate.Candidate.Impact = "" }},
		{name: "missing snapshot", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.SnapshotID = "" }},
		{name: "missing run", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.RunID = "" }},
		{name: "missing invariant", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.SuspectedInvariant = "" }},
		{name: "missing refutation", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.RefutationAttempt = "" }},
		{name: "missing path", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.ConcretePath = nil }},
		{name: "non-supporting evidence", mutate: func(_ *CandidateRecord, record *VerificationRecord) {
			record.Evidence[0].Relation = EvidenceContextualizes
		}},
		{name: "missing summary", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.Evidence[0].Summary = "" }},
		{name: "missing artifact", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.Evidence[0].ArtifactDigest = "" }},
		{name: "missing anchor", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.Evidence[0].Anchors = nil }},
		{name: "dependent evidence", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.Evidence[0].Independent = false }},
		{name: "non-concrete evidence", mutate: func(_ *CandidateRecord, record *VerificationRecord) { record.Evidence[0].Concrete = false }},
		{name: "unknown anchor", mutate: func(_ *CandidateRecord, record *VerificationRecord) {
			record.Evidence[0].Anchors[0].HunkID = "missing-hunk"
		}},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			candidateCopy := candidate
			candidateCopy.Candidate.Anchors = append([]Anchor(nil), candidate.Candidate.Anchors...)
			recordCopy := valid
			recordCopy.ConcretePath = append([]VerificationPathStep(nil), valid.ConcretePath...)
			recordCopy.ConcretePath[0].Anchors = append([]Anchor(nil), valid.ConcretePath[0].Anchors...)
			recordCopy.Evidence = append([]Evidence(nil), valid.Evidence...)
			recordCopy.Evidence[0].Anchors = append([]Anchor(nil), valid.Evidence[0].Anchors...)
			test.mutate(&candidateCopy, &recordCopy)
			if err := ValidateEvidenceFloor(change, candidateCopy, recordCopy); err == nil {
				t.Fatal("ValidateEvidenceFloor() error = nil, want boundary rejection")
			}
		})
	}
}

func TestVerifyCandidateBoundsRetrievalAndRepairsMalformedOutput(t *testing.T) {
	t.Parallel()

	change := verifierFixtureChange()
	model := &verifierSequenceModel{responses: []verifierFixtureResponse{
		{output: []byte(`{"schema_version":"mire/v1/verification"}`)},
		{output: mustJSON(t, verifierFixtureEnvelope(VerificationSupported))},
	}}
	retriever := SnapshotRetrieverFunc(func(_ context.Context, request RetrievalRequest) ([]RetrievedArtifact, error) {
		return []RetrievedArtifact{{Kind: request.Kind, Path: request.Path, Relation: request.Relation, Content: request.Kind}}, nil
	})
	result, err := VerifyCandidate(context.Background(), change, verifierFixtureCandidate(), model, VerifierOptions{
		Retry: RetryPolicy{MaxAttempts: 2, RepairAttempts: 1}, Budget: PassBudget{MaxArtifacts: 1}, Retriever: retriever,
	})
	if err != nil {
		t.Fatalf("VerifyCandidate() error = %v", err)
	}
	if result.Lane != FindingLaneVerified || len(result.Verification.Diagnostics) == 0 || len(model.requests) != 2 || !model.requests[1].Repair {
		t.Fatalf("result=%#v requests=%#v, want repaired verified result with retrieval diagnostic", result, model.requests)
	}
	if result.Run.Provenance.InputDigest == "" || result.Run.Provenance.OutputDigest == "" {
		t.Fatalf("run provenance = %#v", result.Run.Provenance)
	}
}

func TestRunCandidateVerificationsKeepsEachResult(t *testing.T) {
	t.Parallel()

	change := verifierFixtureChange()
	model := &verifierSequenceModel{responses: []verifierFixtureResponse{
		{output: mustJSON(t, verifierFixtureEnvelope(VerificationSupported))},
		{output: mustJSON(t, verifierFixtureEnvelope(VerificationRefuted))},
	}}
	first := verifierFixtureCandidate()
	second := verifierFixtureCandidate()
	second.ID = "candidate-2"
	result, err := RunCandidateVerifications(context.Background(), change, []CandidateRecord{first, second}, model, VerifierOptions{Retry: RetryPolicy{MaxAttempts: 1}})
	if err != nil {
		t.Fatalf("RunCandidateVerifications() error = %v", err)
	}
	if len(result.Results) != 2 || result.Results[0].Lane != FindingLaneVerified || result.Results[1].Lane != FindingLaneRefuted {
		t.Fatalf("batch result = %#v", result)
	}
}

func verifierFixtureChange() ChangeModel {
	return ChangeModel{
		SchemaVersion: "mire/v1/change-model", SessionID: "session-1", SnapshotID: "snapshot-1", SnapshotDigest: "manifest-1", Digest: "change-1",
		Files: []FileChange{{Status: "modified", TargetPath: "src/a.go", Patch: "change", Hunks: []Hunk{{ID: "src/a.go#hunk", Available: true, Digest: "hunk-1"}}}},
	}
}

func verifierFixtureCandidate() CandidateRecord {
	return CandidateRecord{ID: "candidate-1", RunID: "review-run-1", PassName: "correctness", Ordinal: 0, Fingerprint: "candidate-fingerprint", Candidate: Candidate{
		Claim: "The changed guard is bypassed.", Impact: "Invalid input reaches the protected branch.", Category: "correctness", Severity: "high", Confidence: 0.99,
		Anchors: []Anchor{{SnapshotID: "snapshot-1", Path: "src/a.go", HunkID: "src/a.go#hunk", HunkDigest: "hunk-1"}},
	}}
}

func verifierFixtureEnvelope(state VerificationState) VerificationEnvelope {
	anchor := Anchor{SnapshotID: "snapshot-1", Path: "src/a.go", HunkID: "src/a.go#hunk", HunkDigest: "hunk-1"}
	return VerificationEnvelope{
		SchemaVersion: VerificationSchemaVersion, State: state, SuspectedInvariant: "The guard must reject invalid input before the changed branch.",
		ConcretePath:       []VerificationPathStep{{Kind: "control_flow", Summary: "Invalid input reaches the changed branch.", SnapshotID: "snapshot-1", Anchors: []Anchor{anchor}, ArtifactDigest: "path-digest"}},
		SupportingEvidence: []Evidence{{Kind: "source", SnapshotID: "snapshot-1", Anchors: []Anchor{anchor}, Summary: "The changed branch has no rejecting guard.", ArtifactDigest: "source-digest"}},
		GuardSearch:        []Evidence{{Kind: "guard", SnapshotID: "snapshot-1", Anchors: []Anchor{anchor}, Summary: "No guard protects the branch.", ArtifactDigest: "guard-digest"}},
		TestSearch:         []Evidence{{Kind: "test", SnapshotID: "snapshot-1", Anchors: []Anchor{anchor}, Summary: "No test covers invalid input.", ArtifactDigest: "test-digest"}},
		RefutationAttempt:  "Tried to find an input guard and a covering test; neither refuted the path.",
	}
}

type verifierFixtureModel struct {
	output []byte
	err    error
}

func (model verifierFixtureModel) Complete(context.Context, ModelRequest) (ModelResponse, error) {
	return ModelResponse{Output: model.output, FinishReason: "stop"}, model.err
}

type verifierSequenceModel struct {
	responses []verifierFixtureResponse
	requests  []ModelRequest
}

type verifierFixtureResponse struct {
	output []byte
	err    error
}

func (model *verifierSequenceModel) Complete(_ context.Context, request ModelRequest) (ModelResponse, error) {
	model.requests = append(model.requests, request)
	if len(model.requests) > len(model.responses) {
		return ModelResponse{}, errors.New("verifier fixture response exhausted")
	}
	response := model.responses[len(model.requests)-1]
	return ModelResponse{Output: response.output, FinishReason: "stop"}, response.err
}
