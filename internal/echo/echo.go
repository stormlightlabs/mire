// Package echo owns user-facing terminal rendering. It deliberately has no
// logging side effects; diagnostics use NewLogger and stderr.
package echo

import (
	"fmt"
	"io"
	"os"
	"strings"
	"text/tabwriter"

	"github.com/charmbracelet/lipgloss"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

// RoundReport combines immutable round data with a read-only live divergence
// observation for terminal rendering.
type RoundReport struct {
	Round      db.Round
	Snapshot   db.Snapshot
	Divergence snapshot.DivergenceReport
}

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
		if _, err := fmt.Fprintf(
			writer,
			"%s\t%s\t%s\t%s\n",
			SessionID(
				session.ID,
			),
			session.Title,
			repository,
			session.CreatedAt.UTC().Format("2006-01-02 15:04:05Z"),
		); err != nil {
			return err
		}
	}
	return writer.Flush()
}

// RenderReviewCapture writes the stable result of the model-free
// capture. Progress and diagnostics remain owned by the command's stderr.
func RenderReviewCapture(output io.Writer, session db.Session, round db.Round, persistedSnapshot db.Snapshot) error {
	if output == nil {
		return fmt.Errorf("render review capture: output is nil")
	}
	if persistedSnapshot.Kind == snapshot.ComparisonWorktree {
		_, err := fmt.Fprintf(
			output,
			"Captured review\nSession: %s\nRound: %s\nSnapshot: %s\nKind: %s\nComparison: %s\nHEAD: %s\nIndex: %s\nWorktree: %s\n",
			session.ID,
			round.ID,
			persistedSnapshot.ID,
			persistedSnapshot.Kind,
			persistedSnapshot.RequestedComparison,
			persistedSnapshot.BaseOID,
			persistedSnapshot.IndexOID,
			persistedSnapshot.TargetOID,
		)
		return err
	}
	if persistedSnapshot.Kind == snapshot.ComparisonThreeDot {
		_, err := fmt.Fprintf(
			output,
			"Captured review\nSession: %s\nRound: %s\nSnapshot: %s\nKind: %s\nRange: %s\nBase: %s\nEffective base: %s\nTarget: %s\nMerge base: %s\n",
			session.ID,
			round.ID,
			persistedSnapshot.ID,
			persistedSnapshot.Kind,
			persistedSnapshot.RequestedComparison,
			persistedSnapshot.BaseOID,
			persistedSnapshot.EffectiveBaseOID,
			persistedSnapshot.TargetOID,
			persistedSnapshot.MergeBaseOID,
		)
		return err
	}
	_, err := fmt.Fprintf(output,
		"Captured review\nSession: %s\nRound: %s\nSnapshot: %s\nRange: %s\nBase: %s\nTarget: %s\n",
		session.ID, round.ID, persistedSnapshot.ID, persistedSnapshot.RequestedComparison,
		persistedSnapshot.EffectiveBaseOID, persistedSnapshot.TargetOID)
	return err
}

// RenderReviewHistory writes a deterministic session history and divergence
// report. It does not infer findings or mutate persisted state.
func RenderReviewHistory(output io.Writer, session db.Session, reports []RoundReport) error {
	if output == nil {
		return fmt.Errorf("render review history: output is nil")
	}
	if session.ID == "" {
		_, err := fmt.Fprintln(output, Muted("No sessions found."))
		return err
	}
	if _, err := fmt.Fprintf(
		output,
		"Session: %s\nTitle: %s\nRepository: %s\n",
		session.ID,
		session.Title,
		session.RepositoryIdentity,
	); err != nil {
		return err
	}
	for _, report := range reports {
		if _, err := fmt.Fprintf(
			output,
			"\nRound %d: %s\nStatus: %s\n",
			report.Round.Number,
			report.Round.ID,
			report.Round.Status,
		); err != nil {
			return err
		}
		if report.Round.PredecessorRoundID != "" {
			if _, err := fmt.Fprintf(output, "Predecessor: %s\n", report.Round.PredecessorRoundID); err != nil {
				return err
			}
		}
		if report.Snapshot.ID != "" {
			if _, err := fmt.Fprintf(
				output,
				"Snapshot: %s\nKind: %s\n",
				report.Snapshot.ID,
				report.Snapshot.Kind,
			); err != nil {
				return err
			}
		}
		if _, err := fmt.Fprintf(output, "Divergence: %s\n", report.Divergence.Status); err != nil {
			return err
		}
		if report.Divergence.Message != "" {
			if _, err := fmt.Fprintf(output, "Detail: %s\n", report.Divergence.Message); err != nil {
				return err
			}
		}
		if len(report.Divergence.AffectedRefs) > 0 {
			if _, err := fmt.Fprintf(
				output,
				"Refs: %s\n",
				strings.Join(report.Divergence.AffectedRefs, ", "),
			); err != nil {
				return err
			}
		}
		if len(report.Divergence.AffectedPaths) > 0 {
			if _, err := fmt.Fprintf(
				output,
				"Paths: %s\n",
				strings.Join(report.Divergence.AffectedPaths, ", "),
			); err != nil {
				return err
			}
		}
	}
	return nil
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

// StyleHeading styles a report section heading without adding layout or
// terminal-control side effects beyond the current color policy.
func StyleHeading(message string) string {
	return makeStyle(colorText, true).Render(message)
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
