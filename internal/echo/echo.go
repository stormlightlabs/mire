// Package echo owns user-facing terminal rendering. It deliberately has no
// logging side effects; diagnostics use NewLogger and stderr.
package echo

import (
	"fmt"
	"io"
	"os"
	"text/tabwriter"

	"github.com/charmbracelet/lipgloss"
	"github.com/stormlightlabs/mire/internal/db"
)

// Eldritch.nvim-inspired colors.
const (
	colorPurple = "#a48cf2"
	colorPink   = "#f16c9e"
	colorBlue   = "#6cb6ff"
	colorGreen  = "#7bd88f"
	colorYellow = "#f9c859"
	colorMuted  = "#777b9c"
	colorText   = "#ebfafa"
)

// ColorDisabled reports whether NO_COLOR requests plain output.
func ColorDisabled() bool {
	return os.Getenv("NO_COLOR") != ""
}

// RenderSessions writes a deterministic session table to output.
//
// Only stored session and repository metadata is rendered.
func RenderSessions(output io.Writer, sessions []db.Session) error {
	if output == nil {
		return fmt.Errorf("render sessions: output is nil")
	}
	if len(sessions) == 0 {
		_, err := fmt.Fprintln(output, Muted("No sessions found."))
		return err
	}

	writer := tabwriter.NewWriter(output, 0, 4, 2, ' ', 0)
	if _, err := fmt.Fprintf(writer, "%s\t%s\t%s\t%s\n",
		Label("ID"), Label("TITLE"), Label("REPOSITORY"), Label("CREATED")); err != nil {
		return err
	}
	for _, session := range sessions {
		repository := session.RepositoryName
		if repository == "" {
			repository = session.RepositoryIdentity
		}
		if _, err := fmt.Fprintf(writer, "%s\t%s\t%s\t%s\n",
			SessionID(session.ID), session.Title, repository, session.CreatedAt.UTC().Format("2006-01-02 15:04:05Z")); err != nil {
			return err
		}
	}
	return writer.Flush()
}

// RenderReviewCapture writes the stable result of the model-free
// capture. Progress and diagnostics remain owned by the command's stderr.
func RenderReviewCapture(output io.Writer, session db.Session, round db.Round, snapshot db.Snapshot) error {
	if output == nil {
		return fmt.Errorf("render review capture: output is nil")
	}
	_, err := fmt.Fprintf(output,
		"Captured review\nSession: %s\nRound: %s\nSnapshot: %s\nRange: %s\nBase: %s\nTarget: %s\n",
		session.ID, round.ID, snapshot.ID, snapshot.RequestedComparison,
		snapshot.EffectiveBaseOID, snapshot.TargetOID)
	return err
}

// Success styles a successful result message.
func Success(message string) string {
	return makeStyle(colorGreen, true).Render(message)
}

// Error styles a user-facing error message for callers that render errors
// directly. Process diagnostics should normally use NewLogger instead.
func Error(message string) string {
	return makeStyle(colorPink, true).Render(message)
}

// Muted styles secondary terminal text.
func Muted(message string) string {
	return makeStyle(colorMuted, false).Render(message)
}

// Label styles table labels.
func Label(message string) string {
	return makeStyle(colorPurple, true).Render(message)
}

// SessionID styles stable session identifiers.
func SessionID(message string) string {
	return makeStyle(colorBlue, false).Render(message)
}

func styleMuted() lipgloss.Style {
	return makeStyle(colorMuted, false)
}

func styleAccent() lipgloss.Style {
	return makeStyle(colorPurple, true)
}

func styleMessage() lipgloss.Style {
	return makeStyle(colorText, false)
}

func styleValue() lipgloss.Style {
	return makeStyle(colorBlue, false)
}

func makeStyle(color string, bold bool) lipgloss.Style {
	result := lipgloss.NewStyle()
	if ColorDisabled() {
		return result
	}
	result = result.Foreground(lipgloss.Color(color))
	if bold {
		result = result.Bold(true)
	}
	return result
}
