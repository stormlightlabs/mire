package cli

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/server"
)

func newWebCommand(state *commandContext) *cobra.Command {
	var address string
	command := &cobra.Command{
		Use:   "web [SESSION]",
		Short: "Serve the authenticated local review workbench",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(command *cobra.Command, args []string) error {
			identity, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("web: %w", err)
			}
			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("web: initialize private state: %w", err)
			}
			defer closeStore()
			options := server.Options{WorkingDir: state.config.WorkingDir, StateDir: state.config.StateDir}
			if len(args) == 1 {
				sessionID := strings.TrimSpace(args[0])
				session, getErr := store.GetSession(command.Context(), sessionID)
				if getErr != nil {
					return fmt.Errorf("web: %w", getErr)
				}
				if session.RepositoryIdentity != identity.CanonicalIdentity {
					return fmt.Errorf("web: %w", db.ErrSessionRepositoryMismatch)
				}
				options.SelectedSessionID = sessionID
			}
			objectStore, err := state.openObjectStore()
			if err != nil {
				return fmt.Errorf("web: initialize private object store: %w", err)
			}
			options.ObjectStore = objectStore
			webServer, err := server.New(store, options)
			if err != nil {
				return fmt.Errorf("web: create server: %w", err)
			}
			listener, err := webServer.Listen(address)
			if err != nil {
				return fmt.Errorf("web: %w", err)
			}
			launchURL, err := webServer.LaunchURL()
			if err != nil {
				_ = listener.Close()
				return fmt.Errorf("web: %w", err)
			}
			if _, err := fmt.Fprintf(command.ErrOrStderr(), "Mire web listening at %s\n", launchURL); err != nil {
				_ = listener.Close()
				return err
			}
			return webServer.Serve(command.Context(), listener)
		},
	}
	command.Flags().StringVar(&address, "addr", "127.0.0.1:0", "loopback address to serve on")
	return command
}
