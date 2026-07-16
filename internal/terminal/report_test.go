package terminal

import (
	"bytes"
	"strings"
	"testing"

	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/shared"
)

func TestRenderSeparatesVerifiedCandidatesAndRefutedAtFixedWidth(t *testing.T) {
	t.Setenv("NO_COLOR", "1")
	anchor := review.Anchor{Path: "src/ユニコード.go", HunkID: "src/ユニコード.go#hunk", StartLine: 2}
	change := review.ChangeModel{
		Files: []review.FileChange{
			{
				Status:     "modified",
				BasePath:   "src/ユニコード.go",
				TargetPath: "src/ユニコード.go",
				Hunks: []review.Hunk{
					{
						ID:       anchor.HunkID,
						OldStart: 1,
						OldLines: 2,
						NewStart: 1,
						NewLines: 2,
						Lines:    []string{" func main() {\n", "-\treturn \"old\"\n", "+\treturn \"新しい値\"\n", " }\n"},
					},
				},
			},
		},
	}
	report := Report{
		SessionID:           "session",
		RoundID:             "round",
		SnapshotID:          "snapshot",
		SnapshotKind:        "two_dot",
		RequestedComparison: "base..head",
		Status:              "complete",
		Change:              change,
		Findings: []FindingView{
			{
				Revision: review.FindingRevision{
					FindingID:    "finding-1",
					Revision:     1,
					Claim:        "The changed return can violate the invariant.",
					Impact:       "Callers receive an invalid value.",
					Category:     "correctness",
					Severity:     "high",
					Verification: review.VerificationSupported,
					Anchors:      []review.Anchor{anchor},
				},
				Lane: review.FindingLaneVerified,
			},
		},
		Candidates: []CandidateView{
			{
				Candidate: review.CandidateRecord{
					ID: "candidate-1",
					Candidate: review.Candidate{
						Claim:    "A retained candidate needs investigation.",
						Impact:   "Potential issue.",
						Category: "correctness",
						Severity: "medium",
						Anchors:  []review.Anchor{anchor},
					},
				},
				Reason: "inconclusive",
			},
		},
		Refuted: []CandidateView{
			{
				Candidate: review.CandidateRecord{
					ID: "candidate-2",
					Candidate: review.Candidate{
						Claim:    "A refuted hypothesis.",
						Impact:   "No impact.",
						Category: "correctness",
						Severity: "low",
						Anchors:  []review.Anchor{anchor},
					},
				},
				Reason: "refuted",
			},
		},
	}

	var hidden bytes.Buffer
	if err := Render(&hidden, report, Options{Width: 42}); err != nil {
		t.Fatalf("Render(hidden) error = %v", err)
	}
	if strings.Contains(hidden.String(), "candidate-1") || strings.Contains(hidden.String(), "Refuted findings") {
		t.Fatalf("hidden output exposed optional lanes: %q", hidden.String())
	}
	if !strings.Contains(hidden.String(), "Verified findings") || !strings.Contains(hidden.String(), "新しい値") ||
		!strings.Contains(hidden.String(), "src/ユニコード.go#hunk") {
		t.Fatalf("hidden output = %q", hidden.String())
	}
	if strings.Contains(hidden.String(), "\x1b[") {
		t.Fatalf("NO_COLOR output contains ANSI escapes: %q", hidden.String())
	}

	var visible bytes.Buffer
	if err := Render(&visible, report, Options{Width: 42, Candidates: true}); err != nil {
		t.Fatalf("Render(visible) error = %v", err)
	}
	for _, expected := range []string{"Candidates", "candidate-1", "Refuted findings (audit)", "candidate-2", "inconclusive", "refuted"} {
		if !strings.Contains(visible.String(), expected) {
			t.Fatalf("visible output missing %q: %q", expected, visible.String())
		}
	}
	for _, line := range strings.Split(visible.String(), "\n") {
		if len([]rune(line)) > 42 {
			t.Fatalf("line exceeds fixed width: %d > 42: %q", len([]rune(line)), line)
		}
	}

	var narrow bytes.Buffer
	if err := Render(&narrow, report, Options{Width: 24, Candidates: true}); err != nil {
		t.Fatalf("Render(narrow) error = %v", err)
	}
	for _, line := range strings.Split(narrow.String(), "\n") {
		if len([]rune(line)) > 24 {
			t.Fatalf("narrow line exceeds fixed width: %d > 24: %q", len([]rune(line)), line)
		}
	}
}

func TestRenderDiffHandlesAddedDeletedAndMovedFiles(t *testing.T) {
	t.Setenv("NO_COLOR", "1")
	report := Report{Change: review.ChangeModel{Files: []review.FileChange{
		{
			Status:     "added",
			TargetPath: "new.txt",
			Hunks: []review.Hunk{
				{Kind: "changed", OldStart: 0, NewStart: 1, NewLines: 1, Lines: []string{"+added\n"}},
			},
		},
		{
			Status:   "deleted",
			BasePath: "old.txt",
			Hunks:    []review.Hunk{{Kind: "changed", OldStart: 1, OldLines: 1, Lines: []string{"-deleted\n"}}},
		},
		{
			Status:     "renamed",
			BasePath:   "old-name.txt",
			TargetPath: "new-name.txt",
			Hunks:      []review.Hunk{{Kind: "rename", Lines: nil}},
		},
	}}}

	var output bytes.Buffer
	if err := Render(&output, report, Options{Width: 32}); err != nil {
		t.Fatalf("Render() error = %v", err)
	}
	for _, expected := range []string{"--- /dev/null", "+++ b/new.txt", "+added", "--- a/old.txt", "+++ /dev/null", "-deleted", "moved context"} {
		if !strings.Contains(output.String(), expected) {
			t.Fatalf("output missing %q: %q", expected, output.String())
		}
	}
}

func TestWrapSplitsLongUnicodeWordsWithoutDroppingText(t *testing.T) {
	parts := shared.WrapText("αβγδεζη", 3)
	if strings.Join(parts, "") != "αβγδεζη" {
		t.Fatalf("wrap parts = %#v, want original runes preserved", parts)
	}
}
