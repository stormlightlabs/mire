package cli

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/echo"
	"github.com/stormlightlabs/mire/internal/gitrepo"
)

func newReviewCommand(state *commandContext) *cobra.Command {
	var requestedComparison string
	var sessionID string
	command := &cobra.Command{
		Use:   "review",
		Short: "Capture a committed Git range for review",
		Args:  cobra.NoArgs,
		RunE: func(command *cobra.Command, _ []string) error {
			requestedComparison = strings.TrimSpace(requestedComparison)
			if requestedComparison == "" {
				return fmt.Errorf("review: --range is required")
			}
			if strings.TrimSpace(sessionID) != "" {
				return fmt.Errorf("review: --session is not available for initial snapshot capture")
			}
			identity, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("review: %w", err)
			}
			objectStore, err := state.openObjectStore()
			if err != nil {
				return fmt.Errorf("review: initialize private object store: %w", err)
			}
			capture, err := gitrepo.CaptureRange(command.Context(), identity.CanonicalIdentity, requestedComparison, objectStore)
			if err != nil {
				return fmt.Errorf("review: capture %q: %w", requestedComparison, err)
			}
			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("review: initialize private state: %w", err)
			}
			defer closeStore()
			title := "Review " + requestedComparison
			session, round, persistedSnapshot, err := store.CreateCapturedSession(command.Context(), identity, title, capture)
			if err != nil {
				return fmt.Errorf("review: persist captured snapshot: %w", err)
			}
			return echo.RenderReviewCapture(command.OutOrStdout(), session, round, persistedSnapshot)
		},
	}
	command.Flags().StringVar(&requestedComparison, "range", "", "committed comparison in the form <base>..<head> or <base>...<head>")
	command.Flags().StringVar(&sessionID, "session", "", "append to an existing session")
	return command
}
