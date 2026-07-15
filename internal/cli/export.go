package cli

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
	reviewexport "github.com/stormlightlabs/mire/internal/export"
)

func newExportCommand(state *commandContext) *cobra.Command {
	var format string
	var destination string
	command := &cobra.Command{
		Use:   "export [SESSION]",
		Short: "Export a stored review ledger",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(command *cobra.Command, args []string) error {
			selectedFormat := reviewexport.Format(strings.ToLower(strings.TrimSpace(format)))
			if !selectedFormat.Valid() {
				return fmt.Errorf("export: --format must be markdown, json, sarif, or bundle")
			}
			destination = strings.TrimSpace(destination)
			if destination == "" {
				return fmt.Errorf("export: --output is required")
			}
			identity, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("export: %w", err)
			}
			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("export: initialize private state: %w", err)
			}
			defer closeStore()
			var session db.Session
			if len(args) == 1 {
				session, err = store.GetSession(command.Context(), strings.TrimSpace(args[0]))
				if err != nil {
					return fmt.Errorf("export: %w", err)
				}
			} else {
				sessions, listErr := store.ListSessionsForRepository(command.Context(), identity.CanonicalIdentity)
				if listErr != nil {
					return fmt.Errorf("export: list sessions: %w", listErr)
				}
				if len(sessions) == 0 {
					return fmt.Errorf("export: no sessions found")
				}
				session = sessions[len(sessions)-1]
			}
			if session.RepositoryIdentity != identity.CanonicalIdentity {
				return fmt.Errorf("export: %w", db.ErrSessionRepositoryMismatch)
			}
			if session.CurrentRoundID == "" {
				return fmt.Errorf("export: session has no current round")
			}
			round, err := store.GetRound(command.Context(), session.CurrentRoundID)
			if err != nil {
				return fmt.Errorf("export: load current round: %w", err)
			}
			objectStore, err := state.openObjectStore()
			if err != nil {
				return fmt.Errorf("export: initialize private object store: %w", err)
			}
			projection, err := reviewexport.Build(command.Context(), store, session, round, objectStore)
			if err != nil {
				return fmt.Errorf("export: assemble ledger: %w", err)
			}
			warning := "Warning: export may contain sensitive source code and model conversation; it is not an import or replay format."
			if _, err := fmt.Fprintln(command.ErrOrStderr(), warning); err != nil {
				return err
			}
			if err := reviewexport.Write(projection, selectedFormat, destination); err != nil {
				return fmt.Errorf("export: %w", err)
			}
			_, err = fmt.Fprintf(command.OutOrStdout(), "Exported %s review to %s\n", selectedFormat, destination)
			return err
		},
	}
	command.Flags().StringVar(&format, "format", "", "export format: markdown, json, sarif, or bundle")
	command.Flags().StringVar(&destination, "output", "", "destination file or bundle directory")
	return command
}
