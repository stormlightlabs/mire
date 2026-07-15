package cli

import (
	"errors"
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/echo"
)

func newSessionsCommand(state *commandContext) *cobra.Command {
	sessions := &cobra.Command{
		Use:   "sessions",
		Short: "List and manage persisted review sessions",
	}
	sessions.AddCommand(newSessionsListCommand(state))
	sessions.AddCommand(newSessionsDeleteCommand(state))
	return sessions
}

func newSessionsListCommand(state *commandContext) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List persisted review sessions",
		Args:  cobra.NoArgs,
		RunE: func(command *cobra.Command, _ []string) error {
			repository, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("list sessions: %w", err)
			}

			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("initialize private state: %w", err)
			}
			defer closeStore()

			sessions, err := store.ListSessionsForRepository(command.Context(), repository.CanonicalIdentity)
			if err != nil {
				return fmt.Errorf("list sessions: %w", err)
			}
			return echo.RenderSessions(command.OutOrStdout(), sessions)
		},
	}
}

func newSessionsDeleteCommand(state *commandContext) *cobra.Command {
	return &cobra.Command{
		Use:   "delete <SESSION>",
		Short: "Delete a persisted review session",
		Args:  cobra.ExactArgs(1),
		RunE: func(command *cobra.Command, args []string) error {
			sessionID := strings.TrimSpace(args[0])
			if sessionID == "" {
				return fmt.Errorf("delete session: session ID is empty")
			}

			repository, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("delete session: %w", err)
			}

			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("initialize private state: %w", err)
			}
			defer closeStore()

			if err := store.DeleteSessionForRepository(command.Context(), repository.CanonicalIdentity, sessionID); err != nil {
				if errors.Is(err, db.ErrSessionNotFound) {
					return fmt.Errorf("delete session %q: session does not exist", sessionID)
				}
				return fmt.Errorf("delete session %q: %w", sessionID, err)
			}

			_, err = fmt.Fprintln(command.OutOrStdout(), echo.Success("Deleted session "+sessionID))
			return err
		},
	}
}
