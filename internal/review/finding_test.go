package review

import (
	"testing"
	"time"
)

func TestFindingIdentitySurvivesMovedLinesAndRenamedPaths(t *testing.T) {
	t.Parallel()

	firstChange := findingChange("src/old.go", "blob-1", "hunk-1")
	firstCandidate := findingCandidate(firstChange, "src/old.go", 4)
	first, err := NewFindingRevision(firstChange, firstCandidate, "round-1", time.Date(2026, time.July, 15, 17, 0, 0, 0, time.UTC))
	if err != nil {
		t.Fatalf("NewFindingRevision(first) error = %v", err)
	}
	first, err = CorrelateFinding(nil, first)
	if err != nil {
		t.Fatalf("CorrelateFinding(first) error = %v", err)
	}

	secondChange := findingChange("pkg/new.go", "blob-1", "hunk-1")
	secondCandidate := findingCandidate(secondChange, "pkg/new.go", 91)
	second, err := NewFindingRevision(secondChange, secondCandidate, "round-2", time.Date(2026, time.July, 15, 17, 1, 0, 0, time.UTC))
	if err != nil {
		t.Fatalf("NewFindingRevision(second) error = %v", err)
	}
	second, err = CorrelateFinding([]FindingRevision{first}, second)
	if err != nil {
		t.Fatalf("CorrelateFinding(second) error = %v", err)
	}
	if second.FindingID != first.FindingID || second.Revision != first.Revision+1 {
		t.Fatalf("correlated identity = %q/%d, want %q/%d", second.FindingID, second.Revision, first.FindingID, first.Revision+1)
	}
	if len(second.Relationships) != 1 || second.Relationships[0].Kind != FindingRelationshipPredecessor {
		t.Fatalf("relationships = %#v, want one predecessor", second.Relationships)
	}
	if second.Anchors[0].StartLine == first.Anchors[0].StartLine {
		t.Fatal("fixture did not move the line anchor")
	}
}

func TestFindingCorrelationRejectsRewrittenClaimsAndLinksAmbiguity(t *testing.T) {
	t.Parallel()

	change := findingChange("src/a.go", "blob-1", "hunk-1")
	baseCandidate := findingCandidate(change, "src/a.go", 1)
	first, err := NewFindingRevision(change, baseCandidate, "round-1")
	if err != nil {
		t.Fatal(err)
	}
	first, err = CorrelateFinding(nil, first)
	if err != nil {
		t.Fatal(err)
	}

	rewritten := baseCandidate
	rewritten.Candidate.Claim = "A materially different invariant is violated."
	rewrittenRevision, err := NewFindingRevision(findingChange("src/a.go", "blob-1", "hunk-1"), rewritten, "round-2")
	if err != nil {
		t.Fatal(err)
	}
	rewrittenRevision, err = CorrelateFinding([]FindingRevision{first}, rewrittenRevision)
	if err != nil {
		t.Fatal(err)
	}
	if rewrittenRevision.FindingID == first.FindingID || rewrittenRevision.Revision != 1 {
		t.Fatal("rewritten claim incorrectly retained identity")
	}

	secondPrevious := first
	secondPrevious.FindingID = "finding-second"
	secondPrevious.Digest = FindingRevisionDigest(secondPrevious)
	ambiguous, err := CorrelateFinding([]FindingRevision{first, secondPrevious}, findingCandidateRevision(change, baseCandidate, "round-2"))
	if err != nil {
		t.Fatal(err)
	}
	if ambiguous.FindingID == first.FindingID || ambiguous.Revision != 1 {
		t.Fatal("ambiguous match incorrectly retained identity")
	}
	if len(ambiguous.Relationships) != 2 || ambiguous.Relationships[0].Kind != FindingRelationshipPossibleSuccessor {
		t.Fatalf("ambiguous relationships = %#v", ambiguous.Relationships)
	}
}

func TestCorrelateFindingsLinksDuplicateOutputs(t *testing.T) {
	t.Parallel()

	change := findingChange("src/a.go", "blob-1", "hunk-1")
	candidate := findingCandidate(change, "src/a.go", 1)
	prior, err := NewFindingRevision(change, candidate, "round-1")
	if err != nil {
		t.Fatal(err)
	}
	prior, err = CorrelateFinding(nil, prior)
	if err != nil {
		t.Fatal(err)
	}
	firstNext, err := NewFindingRevision(change, candidate, "round-2")
	if err != nil {
		t.Fatal(err)
	}
	secondNext, err := NewFindingRevision(change, candidate, "round-2")
	if err != nil {
		t.Fatal(err)
	}
	correlated, err := CorrelateFindings([]FindingRevision{prior}, []FindingRevision{firstNext, secondNext})
	if err != nil {
		t.Fatal(err)
	}
	if len(correlated) != 2 || correlated[0].FindingID == prior.FindingID || correlated[1].FindingID == prior.FindingID {
		t.Fatalf("duplicate outputs reused prior identity: %#v", correlated)
	}
	for index, finding := range correlated {
		foundDuplicate := false
		for _, relationship := range finding.Relationships {
			if relationship.Kind == FindingRelationshipDuplicate {
				foundDuplicate = true
			}
		}
		if !foundDuplicate {
			t.Fatalf("correlated duplicate %d has no duplicate relationship: %#v", index, finding.Relationships)
		}
	}
}

func TestDispositionAndPresentationValidation(t *testing.T) {
	t.Parallel()

	when := time.Date(2026, time.July, 15, 17, 2, 0, 0, time.UTC)
	disposition := DispositionRecord{FindingID: "finding-1", Revision: 1, Disposition: FindingDispositionAcceptedRisk, CreatedAt: when}
	if err := ValidateDisposition(disposition); err == nil {
		t.Fatal("accepted risk without rationale was accepted")
	}
	disposition.Rationale = "The operational risk is bounded and documented."
	disposition.Digest = DispositionDigest(disposition)
	if err := ValidateDisposition(disposition); err != nil {
		t.Fatalf("ValidateDisposition() error = %v", err)
	}

	presentation := PresentationRecord{FindingID: "finding-1", FindingRevision: 1, Version: 1, Body: "Please reject the invalid input.", CreatedAt: when}
	presentation.SchemaVersion = FindingPresentationSchemaVersion
	presentation.Digest = PresentationDigest(presentation)
	if err := ValidatePresentation(presentation); err != nil {
		t.Fatalf("ValidatePresentation() error = %v", err)
	}
}

func findingChange(path, blobDigest, hunkDigest string) ChangeModel {
	return ChangeModel{
		SchemaVersion: "mire/v1/change-model", SessionID: "session-1", SnapshotID: "snapshot-1", SnapshotDigest: "manifest-1", Digest: "change-1",
		Files: []FileChange{{
			Status: "modified", TargetPath: path, TargetDigest: blobDigest,
			Hunks: []Hunk{{
				ID: path + "#hunk", Digest: hunkDigest, Available: true,
				Lines: []string{"@@ -1 +1 @@\n", "-old\n", "+new\n"},
			}},
		}},
	}
}

func findingCandidate(change ChangeModel, path string, line int) CandidateRecord {
	return CandidateRecord{ID: "candidate-1", RunID: "review-run-1", PassName: "correctness", Ordinal: 0, Fingerprint: "candidate-fingerprint", Candidate: Candidate{
		Claim: "The changed branch accepts invalid input.", Impact: "Invalid input reaches a state that assumes the guard ran.", Category: "correctness", Severity: "high", Confidence: 0.75,
		Anchors: []CandidateAnchor{{SnapshotID: change.SnapshotID, Side: "target", Path: path, HunkID: path + "#hunk", StartLine: line, EndLine: line, HunkDigest: "hunk-1"}},
	}}
}

func findingCandidateRevision(change ChangeModel, candidate CandidateRecord, roundID string) FindingRevision {
	revision, err := NewFindingRevision(change, candidate, roundID)
	if err != nil {
		panic(err)
	}
	return revision
}
