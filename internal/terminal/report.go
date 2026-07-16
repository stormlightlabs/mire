// Package terminal renders deterministic, non-interactive review reports.
package terminal

import (
	"fmt"
	"io"
	"sort"
	"strings"

	"github.com/stormlightlabs/mire/internal/echo"
	"github.com/stormlightlabs/mire/internal/review"
)

// DefaultWidth is used when a caller does not provide a report width.
const DefaultWidth int = 100

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

// Render writes a static report. It never reads files, talks to a model, or
// changes persisted state.
func (r Report) Render(output io.Writer, options Options) error {
	if output == nil {
		return fmt.Errorf("render terminal report: output is nil")
	}
	width := options.Width
	if width <= 0 {
		width = widthFromEnv()
	}
	if width < 24 {
		width = 24
	}

	if options.Verbose {
		return r.renderVerbose(output, width, options.Candidates)
	}
	return r.renderSummary(output, width, options.Candidates)
}

func (r Report) renderVerbose(output io.Writer, width int, includeCandidates bool) error {
	if err := r.renderHeader(output, "Review report", width); err != nil {
		return err
	}
	if err := r.renderDiff(output, width, includeCandidates); err != nil {
		return err
	}
	if err := r.renderFindings(output, width); err != nil {
		return err
	}
	if err := r.renderCandidateSections(output, width, includeCandidates); err != nil {
		return err
	}
	return r.renderCoverage(output, width)
}

func (r Report) renderSummary(output io.Writer, width int, includeCandidates bool) error {
	if err := r.renderHeader(output, "Review summary", width); err != nil {
		return err
	}
	if err := r.renderTotals(output, width); err != nil {
		return err
	}
	if err := r.renderFindings(output, width); err != nil {
		return err
	}
	if err := r.renderCandidateSections(output, width, includeCandidates); err != nil {
		return err
	}
	return r.renderCoverageSummary(output, width)
}

func (r Report) renderHeader(output io.Writer, title string, width int) error {
	if err := writeSectionHeading(output, title, width); err != nil {
		return err
	}
	values := []struct{ label, value string }{
		{"Session", r.SessionID},
		{"Round", r.RoundID},
		{"Snapshot", r.SnapshotID},
		{"Kind", r.SnapshotKind},
		{"Comparison", r.RequestedComparison},
		{"Status", r.Status},
	}
	for _, value := range values {
		if strings.TrimSpace(value.value) == "" {
			continue
		}
		if err := writeWrapped(output, echo.Label(value.label)+": ", value.value, width); err != nil {
			return err
		}
	}
	if r.IncompleteReason != "" {
		if err := writeWrapped(
			output,
			echo.Error("Incomplete analysis: "),
			r.IncompleteReason,
			width,
		); err != nil {
			return err
		}
	}
	return nil
}

func (r Report) renderTotals(output io.Writer, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Review totals", width); err != nil {
		return err
	}
	totals := []string{
		fmt.Sprintf("Changed files: %d", len(r.Change.Files)),
		fmt.Sprintf("Verified findings: %d", len(r.Findings)),
		fmt.Sprintf("Retained candidates: %d", len(r.Candidates)),
		fmt.Sprintf("Refuted findings: %d", len(r.Refuted)),
	}
	for _, total := range totals {
		if err := writeWrapped(output, "- ", total, width); err != nil {
			return err
		}
	}
	return nil
}

func (r Report) renderCandidateSections(output io.Writer, width int, includeCandidates bool) error {
	if includeCandidates {
		if err := renderCandidates(output, "Candidates", r.Candidates, width); err != nil {
			return err
		}
		return renderCandidates(output, "Refuted findings (audit)", r.Refuted, width)
	}
	if len(r.Candidates)+len(r.Refuted) == 0 {
		return nil
	}
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	return writeWrapped(
		output,
		"",
		"Candidates and refuted findings are hidden; rerun with --candidates.",
		width,
	)
}

func (r Report) renderDiff(output io.Writer, width int, includeCandidates bool) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Diff", width); err != nil {
		return err
	}
	if len(r.Change.Files) == 0 {
		_, err := fmt.Fprintln(output, echo.Muted("No changed files."))
		return err
	}
	for _, file := range r.Change.Files {
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
			if err := writeWrapped(
				output,
				"",
				fmt.Sprintf("@@ -%d,%d +%d,%d @@", hunk.OldStart, hunk.OldLines, hunk.NewStart, hunk.NewLines),
				width,
			); err != nil {
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
			for _, finding := range r.Findings {
				if hasHunk(finding.Revision.Anchors, hunk.ID) {
					if err := renderAnchorComment(output, finding.Lane, finding.Revision, width); err != nil {
						return err
					}
				}
			}
			if includeCandidates {
				for _, candidate := range r.Candidates {
					if hasHunk(candidate.Candidate.Candidate.Anchors, hunk.ID) {
						if err := renderCandidateAnchorComment(
							output,
							review.FindingLaneCandidate,
							candidate.Candidate,
							width,
						); err != nil {
							return err
						}
					}
				}
				for _, candidate := range r.Refuted {
					if hasHunk(candidate.Candidate.Candidate.Anchors, hunk.ID) {
						if err := renderCandidateAnchorComment(
							output,
							review.FindingLaneRefuted,
							candidate.Candidate,
							width,
						); err != nil {
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

func (r Report) renderFindings(output io.Writer, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Verified findings", width); err != nil {
		return err
	}
	if len(r.Findings) == 0 {
		if _, err := fmt.Fprintln(output, echo.Muted("No verified findings.")); err != nil {
			return err
		}
		return nil
	}
	for _, finding := range r.Findings {
		if err := finding.render(output, width); err != nil {
			return err
		}
	}
	return nil
}

func (r Report) renderCoverage(output io.Writer, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Coverage and incomplete analysis", width); err != nil {
		return err
	}
	if len(r.Passes) == 0 {
		if err := writeWrapped(output, "", "No review passes were persisted.", width); err != nil {
			return err
		}
	} else {
		for _, pass := range r.Passes {
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
	for _, diagnostic := range r.Diagnostics {
		if err := writeWrapped(output, echo.Error("! ")+diagnostic.Code+": ", diagnostic.Message, width); err != nil {
			return err
		}
	}
	for _, failure := range r.Coverage.Failures {
		if err := writeWrapped(
			output,
			echo.Error("! coverage: "),
			failure.PassName+": "+failure.Message,
			width,
		); err != nil {
			return err
		}
	}
	for _, exclusion := range r.Coverage.Exclusions {
		if err := writeWrapped(
			output,
			echo.Muted("! omitted: "),
			exclusion.PassName+": "+exclusion.Reason,
			width,
		); err != nil {
			return err
		}
	}
	for _, gap := range r.Coverage.Gaps {
		if err := writeWrapped(output, echo.Muted("! gap: "), gap, width); err != nil {
			return err
		}
	}
	if len(r.Diagnostics) == 0 && len(r.Coverage.Failures) == 0 && len(r.Coverage.Exclusions) == 0 &&
		len(r.Coverage.Gaps) == 0 &&
		r.IncompleteReason == "" {
		return writeWrapped(output, "", "No incomplete-analysis diagnostics.", width)
	}
	return nil
}

func (r Report) renderCoverageSummary(output io.Writer, width int) error {
	if _, err := fmt.Fprintln(output); err != nil {
		return err
	}
	if err := writeSectionHeading(output, "Coverage summary", width); err != nil {
		return err
	}
	if len(r.Passes) == 0 {
		if err := writeWrapped(output, "", "No review passes were persisted.", width); err != nil {
			return err
		}
	} else {
		for _, pass := range r.Passes {
			line := fmt.Sprintf("- %s: %s", pass.Name, pass.Status)
			if pass.CandidateCount > 0 {
				line += fmt.Sprintf(" (%d candidate(s))", pass.CandidateCount)
			}
			if err := writeWrapped(output, "", line, width); err != nil {
				return err
			}
		}
	}

	metrics := []string{
		fmt.Sprintf("Examined files: %d", len(r.Coverage.ExaminedFiles)),
		fmt.Sprintf("Examined hunks: %d", len(r.Coverage.ExaminedHunks)),
		fmt.Sprintf("Context exclusions: %d", len(r.Coverage.Exclusions)),
		fmt.Sprintf("Coverage failures: %d", len(r.Coverage.Failures)),
		fmt.Sprintf("Coverage gaps: %d", len(r.Coverage.Gaps)),
		fmt.Sprintf("Diagnostics: %d", len(r.Diagnostics)),
	}
	for _, metric := range metrics {
		if err := writeWrapped(output, "", "- "+metric, width); err != nil {
			return err
		}
	}

	for _, exclusion := range summarizeCoverageExclusions(r.Coverage.Exclusions) {
		line := fmt.Sprintf(
			"  - %s: %d exclusion(s) — %s",
			exclusion.PassName,
			exclusion.Count,
			exclusion.Reason,
		)
		if err := writeWrapped(output, "", line, width); err != nil {
			return err
		}
	}

	hasDiagnostics := len(r.Diagnostics) > 0 || len(r.Coverage.Failures) > 0 ||
		len(r.Coverage.Exclusions) > 0 || len(r.Coverage.Gaps) > 0
	if hasDiagnostics {
		return writeWrapped(
			output,
			"",
			"Detailed coverage diagnostics are hidden; rerun with --verbose.",
			width,
		)
	}
	if r.IncompleteReason == "" {
		return writeWrapped(output, "", "No incomplete-analysis diagnostics.", width)
	}
	return nil
}

// FindingView is one finding with its derived presentation lane.
type FindingView struct {
	Revision     review.FindingRevision
	Lane         review.FindingLane
	Candidate    *review.CandidateRecord
	Verification *review.VerificationRecord
}

func (f FindingView) render(output io.Writer, width int) error {
	revision := f.Revision
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
	if err := writeWrapped(
		output,
		"  Category: ",
		revision.Category+"; verification="+string(revision.Verification),
		width,
	); err != nil {
		return err
	}
	return nil
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
	Verbose    bool
}

type coverageExclusionSummary struct {
	PassName string
	Reason   string
	Count    int
}

func summarizeCoverageExclusions(values []review.CoverageExclusion) []coverageExclusionSummary {
	counts := make(map[string]*coverageExclusionSummary)
	for _, value := range values {
		key := value.PassName + "\x00" + value.Reason
		if summary, ok := counts[key]; ok {
			summary.Count++
			continue
		}
		counts[key] = &coverageExclusionSummary{PassName: value.PassName, Reason: value.Reason, Count: 1}
	}
	result := make([]coverageExclusionSummary, 0, len(counts))
	for _, summary := range counts {
		result = append(result, *summary)
	}
	sort.Slice(result, func(i, j int) bool {
		if result[i].PassName != result[j].PassName {
			return result[i].PassName < result[j].PassName
		}
		return result[i].Reason < result[j].Reason
	})
	return result
}
