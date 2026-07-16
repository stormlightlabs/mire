package terminal

import (
	"fmt"
	"io"
	"strconv"
	"strings"

	"github.com/stormlightlabs/mire/internal/echo"
	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/shared"
)

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

func renderCandidateAnchorComment(
	output io.Writer,
	lane review.FindingLane,
	candidate review.CandidateRecord,
	width int,
) error {
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
	return writeWrapped(
		output,
		echo.Muted("  ! ")+echo.Label(string(lane))+": ",
		candidate.Candidate.Claim+" ["+anchor+"]",
		width,
	)
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
		if err := writeWrapped(
			output,
			echo.Label("- "),
			fmt.Sprintf("%s (%s)", candidate.ID, candidate.Candidate.Severity),
			width,
		); err != nil {
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
