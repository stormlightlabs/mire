package cli

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/echo"
	"github.com/stormlightlabs/mire/internal/gitrepo"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

func newReviewCommand(state *commandContext) *cobra.Command {
	var requestedComparison string
	var sessionID string
	var worktree bool
	command := &cobra.Command{
		Use:   "review",
		Short: "Capture a Git range or working tree for review",
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
			return echo.RenderReviewCapture(command.OutOrStdout(), session, round, persistedSnapshot)
		},
	}
	command.Flags().StringVar(&requestedComparison, "range", "", "committed comparison in the form <base>..<head> or <base>...<head>")
	command.Flags().BoolVar(&worktree, "worktree", false, "capture HEAD, the index, and the final working tree")
	command.Flags().StringVar(&sessionID, "session", "", "append to an existing session")
	return command
}
