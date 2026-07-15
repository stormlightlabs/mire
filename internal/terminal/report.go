// Package terminal renders deterministic, non-interactive review reports.
package terminal

import (
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"

	"github.com/stormlightlabs/mire/internal/echo"
	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/shared"
)

// DefaultWidth is used when a caller does not provide a report width. It is
// intentionally fixed so captured output is stable in pipes and tests.
const DefaultWidth = 100

// Report is the read-only projection needed by the terminal renderer. It is
// assembled from the stored ledger and never contains live repository data.
type Report struct {
	SessionID           string
	RoundID             string
	SnapshotID          string
	SnapshotKind        string
	RequestedComparison string
	Status              string
	Change              review.ChangeModel
	Coverage            review.ReviewCoverage
	Passes              []review.PassCoverage
	Diagnostics         []review.ReviewDiagnostic
	Findings            []FindingView
	Candidates          []CandidateView
	Refuted             []CandidateView
	IncompleteReason    string
}

// FindingView is one finding with its derived presentation lane.
type FindingView struct {
	Revision     review.FindingRevision
	Lane         review.FindingLane
	Candidate    *review.CandidateRecord
	Verification *review.VerificationRecord
}

// CandidateView is one retained candidate that is not in the primary verified
// lane. Refuted candidates are kept separately for audit presentation.
type CandidateView struct {
	Candidate review.CandidateRecord
	Reason    string
}

// Options controls deterministic report layout.
type Options struct {
	Width      int
	Candidates bool
}

// Render writes a static report. It never reads files, talks to a model, or
// changes persisted state.
func Render(output io.Writer, report Report, options Options) error {
	if output == nil {
		return fmt.Errorf("render terminal report: output is nil")
	}
	width := options.Width
	if width <= 0 {
		width = widthFromEnvironment()
	}
	if width < 24 {
		width = 24
	}

	if err := renderHeader(output, report, width); err != nil {
		return err
	}
	if err := renderDiff(output, report, width, options.Candidates); err != nil {
		return err
	}
	if err := renderFindings(output, report, width); err != nil {
		return err
	}
	if options.Candidates {
		if err := renderCandidates(output, "Candidates", report.Candidates, width); err != nil {
			return err
		}
		if err := renderCandidates(output, "Refuted findings (audit)", report.Refuted, width); err != nil {
			return err
		}
	} else if len(report.Candidates)+len(report.Refuted) > 0 {
		if _, err := fmt.Fprintln(output); err != nil {
			return err
		}
		if err := writeWrapped(output, "", "Candidates and refuted findings are hidden; rerun with --candidates.", width); err != nil {
			return err
		}
	}
	return renderCoverage(output, report, width)
}

func renderHeader(output io.Writer, report Report, width int) error {
	if err := writeSectionHeading(output, "Review report", width); err != nil {
		return err
	}
	values := []struct{ label, value string }{
		{"Session", report.SessionID},
		{"Round", report.RoundID},
		{"Snapshot", report.SnapshotID},
		{"Kind", report.SnapshotKind},
		{"Comparison", report.RequestedComparison},
		{"Status", report.Status},
	}
	for _, value := range values {
		if strings.TrimSpace(value.value) == "" {
			continue
		}
		if err := writeWrapped(output, echo.Label(value.label)+": ", value.value, width); err != nil {
			return err
		}
	}
	if report.IncompleteReason != "" {
		if err := writeWrapped(output, echo.Error("Incomplete analysis: "), report.IncompleteReason, width); err != nil {
			return err
		}
	}
	return nil
}

func renderDiff(output io.Writer, report Report, width int, includeCandidates bool) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Diff", width); err != nil {
		return err
	}
	if len(report.Change.Files) == 0 {
		_, err := fmt.Fprintln(output, echo.Muted("No changed files."))
		return err
	}
	for _, file := range report.Change.Files {
		pathName := file.TargetPath
		if pathName == "" {
			pathName = file.BasePath
		}
		if err := writeWrapped(output, echo.Label("File: "), pathName+" ("+file.Status+")", width); err != nil {
			return err
		}
		oldPath, newPath := file.BasePath, file.TargetPath
		if oldPath == "" {
			oldPath = "/dev/null"
		} else {
			oldPath = "a/" + oldPath
		}
		if newPath == "" {
			newPath = "/dev/null"
		} else {
			newPath = "b/" + newPath
		}
		if err := writeWrapped(output, "--- ", oldPath, width); err != nil {
			return err
		}
		if err := writeWrapped(output, "+++ ", newPath, width); err != nil {
			return err
		}
		for _, hunk := range file.Hunks {
			if err := writeWrapped(output, "", fmt.Sprintf("@@ -%d,%d +%d,%d @@", hunk.OldStart, hunk.OldLines, hunk.NewStart, hunk.NewLines), width); err != nil {
				return err
			}
			if hunk.Binary {
				if err := writeWrapped(output, "", "Binary files differ.", width); err != nil {
					return err
				}
			} else if len(hunk.Lines) == 0 {
				if err := writeWrapped(output, "", "(no textual hunk content)", width); err != nil {
					return err
				}
			} else {
				for _, line := range hunk.Lines {
					if err := renderDiffLine(output, line, width); err != nil {
						return err
					}
				}
			}
			for _, finding := range report.Findings {
				if hasHunk(finding.Revision.Anchors, hunk.ID) {
					if err := renderAnchorComment(output, finding.Lane, finding.Revision, width); err != nil {
						return err
					}
				}
			}
			if includeCandidates {
				for _, candidate := range report.Candidates {
					if hasHunk(candidate.Candidate.Candidate.Anchors, hunk.ID) {
						if err := renderCandidateAnchorComment(output, review.FindingLaneCandidate, candidate.Candidate, width); err != nil {
							return err
						}
					}
				}
				for _, candidate := range report.Refuted {
					if hasHunk(candidate.Candidate.Candidate.Anchors, hunk.ID) {
						if err := renderCandidateAnchorComment(output, review.FindingLaneRefuted, candidate.Candidate, width); err != nil {
							return err
						}
					}
				}
			}
			if hunk.Kind == "rename" {
				if _, err := fmt.Fprintln(output, echo.Muted("  (moved context; content is unchanged)")); err != nil {
					return err
				}
			}
		}
	}
	return nil
}

func renderDiffLine(output io.Writer, line string, width int) error {
	if line == "" {
		_, err := fmt.Fprintln(output)
		return err
	}
	prefix := line[:1]
	body := strings.TrimSuffix(line[1:], "\n")
	if body == "" {
		_, err := fmt.Fprintln(output, prefix)
		return err
	}
	parts := wrapDiffBody(body, max(1, width-2))
	for _, part := range parts {
		styledPrefix := prefix
		switch prefix {
		case "+":
			styledPrefix = echo.Success(prefix)
		case "-":
			styledPrefix = echo.Error(prefix)
		default:
			styledPrefix = echo.Muted(prefix)
		}
		if _, err := fmt.Fprintln(output, styledPrefix+part); err != nil {
			return err
		}
	}
	return nil
}

func renderAnchorComment(output io.Writer, lane review.FindingLane, finding review.FindingRevision, width int) error {
	anchor := "hunk"
	if len(finding.Anchors) > 0 {
		value := finding.Anchors[0]
		anchor = value.HunkID
		if value.Path != "" && !strings.HasPrefix(anchor, value.Path+"#") {
			anchor = value.Path + "#" + anchor
		}
		if value.StartLine > 0 {
			anchor += ":" + strconv.Itoa(value.StartLine)
		}
	}
	return writeWrapped(output, echo.Muted("  ! ")+echo.Label(string(lane))+": ", finding.Claim+" ["+anchor+"]", width)
}

func renderCandidateAnchorComment(output io.Writer, lane review.FindingLane, candidate review.CandidateRecord, width int) error {
	anchor := "hunk"
	if len(candidate.Candidate.Anchors) > 0 {
		value := candidate.Candidate.Anchors[0]
		anchor = value.HunkID
		if value.Path != "" && !strings.HasPrefix(anchor, value.Path+"#") {
			anchor = value.Path + "#" + anchor
		}
		if value.StartLine > 0 {
			anchor += ":" + strconv.Itoa(value.StartLine)
		}
	}
	return writeWrapped(output, echo.Muted("  ! ")+echo.Label(string(lane))+": ", candidate.Candidate.Claim+" ["+anchor+"]", width)
}

func renderFindings(output io.Writer, report Report, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Verified findings", width); err != nil {
		return err
	}
	if len(report.Findings) == 0 {
		if _, err := fmt.Fprintln(output, echo.Muted("No verified findings.")); err != nil {
			return err
		}
		return nil
	}
	for _, finding := range report.Findings {
		if err := renderFinding(output, finding, width); err != nil {
			return err
		}
	}
	return nil
}

func renderFinding(output io.Writer, finding FindingView, width int) error {
	revision := finding.Revision
	title := fmt.Sprintf("[%s] %s r%d", revision.Severity, revision.FindingID, revision.Revision)
	if err := writeWrapped(output, echo.Label("- "), title, width); err != nil {
		return err
	}
	if err := writeWrapped(output, "  Claim: ", revision.Claim, width); err != nil {
		return err
	}
	if err := writeWrapped(output, "  Impact: ", revision.Impact, width); err != nil {
		return err
	}
	if err := writeWrapped(output, "  Category: ", revision.Category+"; verification="+string(revision.Verification), width); err != nil {
		return err
	}
	return nil
}

func renderCandidates(output io.Writer, title string, values []CandidateView, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, title, width); err != nil {
		return err
	}
	if len(values) == 0 {
		_, err := fmt.Fprintln(output, echo.Muted("None."))
		return err
	}
	for _, value := range values {
		candidate := value.Candidate
		if err := writeWrapped(output, echo.Label("- "), fmt.Sprintf("%s (%s)", candidate.ID, candidate.Candidate.Severity), width); err != nil {
			return err
		}
		if err := writeWrapped(output, "  Claim: ", candidate.Candidate.Claim, width); err != nil {
			return err
		}
		if value.Reason != "" {
			if err := writeWrapped(output, "  State: ", value.Reason, width); err != nil {
				return err
			}
		}
	}
	return nil
}

func renderCoverage(output io.Writer, report Report, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Coverage and incomplete analysis", width); err != nil {
		return err
	}
	if len(report.Passes) == 0 {
		if err := writeWrapped(output, "", "No review passes were persisted.", width); err != nil {
			return err
		}
	} else {
		for _, pass := range report.Passes {
			line := fmt.Sprintf("- %s: %s", pass.Name, pass.Status)
			if pass.Reason != "" {
				line += " — " + pass.Reason
			}
			if pass.CandidateCount > 0 {
				line += fmt.Sprintf(" (%d candidate(s))", pass.CandidateCount)
			}
			if err := writeWrapped(output, "", line, width); err != nil {
				return err
			}
		}
	}
	for _, diagnostic := range report.Diagnostics {
		if err := writeWrapped(output, echo.Error("! ")+diagnostic.Code+": ", diagnostic.Message, width); err != nil {
			return err
		}
	}
	for _, failure := range report.Coverage.Failures {
		if err := writeWrapped(output, echo.Error("! coverage: "), failure.PassName+": "+failure.Message, width); err != nil {
			return err
		}
	}
	for _, exclusion := range report.Coverage.Exclusions {
		if err := writeWrapped(output, echo.Muted("! omitted: "), exclusion.PassName+": "+exclusion.Reason, width); err != nil {
			return err
		}
	}
	for _, gap := range report.Coverage.Gaps {
		if err := writeWrapped(output, echo.Muted("! gap: "), gap, width); err != nil {
			return err
		}
	}
	if len(report.Diagnostics) == 0 && len(report.Coverage.Failures) == 0 && len(report.Coverage.Exclusions) == 0 && len(report.Coverage.Gaps) == 0 && report.IncompleteReason == "" {
		return writeWrapped(output, "", "No incomplete-analysis diagnostics.", width)
	}
	return nil
}

func writeWrapped(output io.Writer, prefix, value string, width int) error {
	prefixWidth := shared.RuneWidth(prefix)
	available := max(1, width-prefixWidth)
	parts := shared.WrapText(value, available)
	if len(parts) == 0 {
		_, err := fmt.Fprintln(output, prefix)
		return err
	}
	for index, part := range parts {
		linePrefix := prefix
		if index > 0 {
			linePrefix = strings.Repeat(" ", prefixWidth)
		}
		if _, err := fmt.Fprintln(output, linePrefix+part); err != nil {
			return err
		}
	}
	return nil
}

func writeSectionHeading(output io.Writer, title string, width int) error {
	for _, line := range shared.WrapText(title, width) {
		if _, err := fmt.Fprintln(output, echo.StyleHeading(line)); err != nil {
			return err
		}
	}
	return nil
}

func wrapDiffBody(value string, width int) []string {
	value = strings.TrimSuffix(strings.ReplaceAll(value, "\r\n", "\n"), "\n")
	if value == "" {
		return nil
	}
	if shared.RuneWidth(value) <= width {
		return []string{value}
	}
	parts := make([]string, 0, (shared.RuneWidth(value)+width-1)/width)
	for value != "" {
		part, rest := shared.SplitRunes(value, width)
		parts = append(parts, part)
		value = rest
	}
	return parts
}

func hasHunk(anchors []review.Anchor, hunkID string) bool {
	for _, anchor := range anchors {
		if anchor.HunkID == hunkID {
			return true
		}
	}
	return false
}

func widthFromEnvironment() int {
	if value, err := strconv.Atoi(strings.TrimSpace(os.Getenv("COLUMNS"))); err == nil && value > 0 {
		return value
	}
	return DefaultWidth
}
