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

func newShowCommand(state *commandContext) *cobra.Command {
	return &cobra.Command{
		Use:   "show [SESSION]",
		Short: "Show review rounds and live divergence",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(command *cobra.Command, args []string) error {
			identity, err := state.currentRepository(command.Context())
			if err != nil {
				return fmt.Errorf("show: %w", err)
			}
			store, closeStore, err := state.openStore(command.Context())
			if err != nil {
				return fmt.Errorf("show: initialize private state: %w", err)
			}
			defer closeStore()

			var session db.Session
			if len(args) == 1 {
				session, err = store.GetSession(command.Context(), strings.TrimSpace(args[0]))
				if err != nil {
					return fmt.Errorf("show: %w", err)
				}
				if session.RepositoryIdentity != identity.CanonicalIdentity {
					return fmt.Errorf("show: %w", db.ErrSessionRepositoryMismatch)
				}
			} else {
				sessions, listErr := store.ListSessionsForRepository(command.Context(), identity.CanonicalIdentity)
				if listErr != nil {
					return fmt.Errorf("show: list sessions: %w", listErr)
				}
				if len(sessions) == 0 {
					return echo.RenderReviewHistory(command.OutOrStdout(), db.Session{}, nil)
				}
				session = sessions[len(sessions)-1]
			}

			rounds, err := store.ListRounds(command.Context(), session.ID)
			if err != nil {
				return fmt.Errorf("show: list rounds: %w", err)
			}
			objectStore, err := state.openObjectStore()
			if err != nil {
				return fmt.Errorf("show: initialize private object store: %w", err)
			}
			reports := make([]echo.RoundReport, 0, len(rounds))
			for _, round := range rounds {
				report := snapshot.DivergenceReport{Status: snapshot.DivergenceUnavailable, Message: "Round has no captured snapshot."}
				var persisted db.Snapshot
				if round.SnapshotID != "" {
					persisted, err = store.GetSnapshot(command.Context(), round.SnapshotID)
					if err != nil {
						return fmt.Errorf("show: load snapshot %q: %w", round.SnapshotID, err)
					}
					report, err = gitrepo.CheckDivergence(command.Context(), identity.CanonicalIdentity, store, persisted, objectStore)
					if err != nil {
						return fmt.Errorf("show: check round %d divergence: %w", round.Number, err)
					}
				}
				reports = append(reports, echo.RoundReport{Round: round, Snapshot: persisted, Divergence: report})
			}
			return echo.RenderReviewHistory(command.OutOrStdout(), session, reports)
		},
	}
}
