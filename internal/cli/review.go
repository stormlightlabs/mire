package cli

import (
	"fmt"
	"io"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/echo"
	"github.com/stormlightlabs/mire/internal/gitrepo"
	"github.com/stormlightlabs/mire/internal/snapshot"
	"github.com/stormlightlabs/mire/internal/terminal"
)

func newReviewCommand(state *commandContext) *cobra.Command {
	var requestedComparison string
	var sessionID string
	var worktree bool
	var width int
	var candidates bool
	command := &cobra.Command{
		Use:   "review",
		Short: "Run a review against a captured Git range or working tree",
		Args:  cobra.NoArgs,
		RunE: func(command *cobra.Command, _ []string) error {
			requestedComparison = strings.TrimSpace(requestedComparison)
			sessionID = strings.TrimSpace(sessionID)
			if requestedComparison != "" && worktree {
				return fmt.Errorf("review: --range and --worktree are mutually exclusive")
			}
			if requestedComparison == "" && !worktree {
				return fmt.Errorf("review: --range or --worktree is required")
			}
			identity, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("review: %w", err)
			}
			objectStore, err := state.openObjectStore()
			if err != nil {
				return fmt.Errorf("review: initialize private object store: %w", err)
			}
			var capture snapshot.Capture
			if worktree {
				capture, err = gitrepo.CaptureWorktree(command.Context(), identity.CanonicalIdentity, objectStore)
			} else {
				capture, err = gitrepo.CaptureRange(command.Context(), identity.CanonicalIdentity, requestedComparison, objectStore)
			}
			if err != nil {
				if worktree {
					return fmt.Errorf("review: capture worktree: %w", err)
				}
				return fmt.Errorf("review: capture %q: %w", requestedComparison, err)
			}
			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("review: initialize private state: %w", err)
			}
			defer closeStore()
			var session db.Session
			var round db.Round
			var persistedSnapshot db.Snapshot
			if sessionID == "" {
				title := "Review " + requestedComparison
				if worktree {
					title = "Review working tree"
				}
				session, round, persistedSnapshot, err = store.CreateCapturedSession(command.Context(), identity, title, capture)
			} else {
				session, round, persistedSnapshot, err = store.AppendCapturedRound(command.Context(), sessionID, identity, capture)
			}
			if err != nil {
				return fmt.Errorf("review: persist captured snapshot: %w", err)
			}
			progress := state.config.Progress
			writeProgress(progress, "review: captured immutable snapshot "+persistedSnapshot.ID)
			operation, err := store.CreateOperation(command.Context(), session.ID, round.ID, db.OperationKindReview)
			if err != nil {
				return fmt.Errorf("review: queue review operation: %w", err)
			}
			if _, err := store.AcquireOperation(command.Context(), operation.ID); err != nil {
				return fmt.Errorf("review: acquire review operation: %w", err)
			}
			writeProgress(progress, "review: assembling frozen change model")
			execution, executionErr := executeReview(command.Context(), store, session, round, persistedSnapshot, capture, objectStore, state.config.Model)
			if executionErr != nil {
				_, _ = store.FailOperation(command.Context(), operation.ID, executionErr.Error())
				return fmt.Errorf("review: run review pipeline: %w", executionErr)
			}
			if execution.IncompleteReason != "" {
				_, err = store.FailOperation(command.Context(), operation.ID, execution.IncompleteReason)
			} else {
				_, err = store.CompleteOperation(command.Context(), operation.ID)
			}
			if err != nil {
				return fmt.Errorf("review: persist review operation result: %w", err)
			}
			writeProgress(progress, "review: persisted review ledger")
			updatedRound, err := store.GetRound(command.Context(), round.ID)
			if err != nil {
				return fmt.Errorf("review: reload round: %w", err)
			}
			report, err := buildTerminalReport(command.Context(), store, session, updatedRound, persistedSnapshot, capture, objectStore)
			if err != nil {
				return fmt.Errorf("review: assemble terminal report: %w", err)
			}
			if err := echo.RenderReviewCapture(command.OutOrStdout(), session, updatedRound, persistedSnapshot); err != nil {
				return err
			}
			if err := terminal.Render(command.OutOrStdout(), report, terminal.Options{Width: width, Candidates: candidates}); err != nil {
				return err
			}
			if report.IncompleteReason != "" {
				writeProgress(progress, "review: incomplete analysis — "+report.IncompleteReason)
			} else {
				writeProgress(progress, fmt.Sprintf("review: complete — %d verified finding(s), %d retained candidate(s)", len(report.Findings), len(report.Candidates)))
			}
			return nil
		},
	}
	command.Flags().StringVar(&requestedComparison, "range", "", "committed comparison in the form <base>..<head> or <base>...<head>")
	command.Flags().BoolVar(&worktree, "worktree", false, "capture HEAD, the index, and the final working tree")
	command.Flags().StringVar(&sessionID, "session", "", "append to an existing session")
	command.Flags().IntVar(&width, "width", terminal.DefaultWidth, "report width in terminal columns")
	command.Flags().BoolVar(&candidates, "candidates", false, "include retained candidate and refuted sections")
	return command
}

func writeProgress(output io.Writer, message string) {
	if output == nil {
		return
	}
	_, _ = fmt.Fprintln(output, message)
}
