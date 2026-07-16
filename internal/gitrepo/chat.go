package gitrepo

import (
	"context"
	"fmt"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/review"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

// CheckChatDivergenceForSession returns the durable chat timeline together
// with a read-time stale marker relative to the live repository. The stored
// messages and their frozen snapshot bindings are never rewritten.
func CheckChatDivergenceForSession(
	ctx context.Context,
	directory string,
	store *db.RepositoryStore,
	sessionID string,
	objectStore *snapshot.ObjectStore,
) (review.ChatTimeline, error) {
	if store == nil {
		return review.ChatTimeline{}, fmt.Errorf("check chat divergence: store is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	session, err := store.GetSession(ctx, sessionID)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	if session.CurrentRoundID == "" {
		return review.ChatTimeline{}, fmt.Errorf("check chat divergence: session has no active round")
	}
	round, err := store.GetRound(ctx, session.CurrentRoundID)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	if round.SnapshotID == "" {
		return review.ChatTimeline{}, fmt.Errorf("check chat divergence: active round has no snapshot")
	}
	frozen, err := store.GetSnapshot(ctx, round.SnapshotID)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	timeline, err := store.GetChatTimeline(ctx, session.ID)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	report, err := CheckDivergence(ctx, directory, store, frozen, objectStore)
	if err != nil {
		return review.ChatTimeline{}, err
	}
	timeline.Divergence = report
	timeline.Stale = report.Status == snapshot.DivergenceChanged
	return timeline, nil
}
