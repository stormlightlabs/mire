package db

import (
	"context"
	"errors"
	"path/filepath"
	"testing"
	"time"
)

func TestOperationLifecyclePersistsStateAndActivity(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	clock := time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC)
	database, err := OpenState(ctx, filepath.Join(t.TempDir(), "state"))
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	defer database.Close()
	store := NewRepositoryStore(
		database,
		WithClock(func() time.Time { return clock }),
		WithProcessInstanceID("process-one"),
		WithOperationLeaseDuration(time.Minute),
	)
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/example",
		DisplayName:       "example",
		DiscoveredGitDir:  "/workspaces/example/.git",
	}
	session, err := store.CreateSession(ctx, identity, "Review")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	round, err := store.CreateRound(ctx, session.ID)
	if err != nil {
		t.Fatalf("CreateRound() error = %v", err)
	}
	operation, err := store.CreateOperation(ctx, session.ID, round.ID, OperationKindReview)
	if err != nil {
		t.Fatalf("CreateOperation() error = %v", err)
	}
	if operation.Status != OperationStatusQueued {
		t.Fatalf("created operation status = %s, want queued", operation.Status)
	}
	if _, err := store.CreateOperation(
		ctx,
		session.ID,
		round.ID,
		OperationKindReview,
	); !errors.Is(
		err,
		ErrOperationActive,
	) {
		t.Fatalf("second CreateOperation() error = %v, want ErrOperationActive", err)
	}

	running, err := store.AcquireOperation(ctx, operation.ID)
	if err != nil {
		t.Fatalf("AcquireOperation() error = %v", err)
	}
	if running.Status != OperationStatusRunning || running.OwnerID != "process-one" {
		t.Fatalf("acquired operation = %#v", running)
	}
	if got, err := store.GetRound(ctx, round.ID); err != nil {
		t.Fatalf("GetRound() after acquire error = %v", err)
	} else if got.Status != RoundStatusRunning {
		t.Fatalf("round status after acquire = %s, want running", got.Status)
	}

	clock = clock.Add(10 * time.Second)
	heartbeat, err := store.RenewOperation(ctx, operation.ID)
	if err != nil {
		t.Fatalf("RenewOperation() error = %v", err)
	}
	if !heartbeat.HeartbeatAt.Equal(clock) || !heartbeat.LeaseExpiresAt.Equal(clock.Add(time.Minute)) {
		t.Fatalf(
			"heartbeat lease = %s/%s, want %s/%s",
			heartbeat.HeartbeatAt,
			heartbeat.LeaseExpiresAt,
			clock,
			clock.Add(time.Minute),
		)
	}

	clock = clock.Add(10 * time.Second)
	complete, err := store.CompleteOperation(ctx, operation.ID)
	if err != nil {
		t.Fatalf("CompleteOperation() error = %v", err)
	}
	if complete.Status != OperationStatusComplete || complete.Failure != "" {
		t.Fatalf("completed operation = %#v", complete)
	}
	if got, err := store.GetRound(ctx, round.ID); err != nil {
		t.Fatalf("GetRound() after complete error = %v", err)
	} else if got.Status != RoundStatusComplete {
		t.Fatalf("round status after complete = %s, want complete", got.Status)
	}

	activities, err := store.ListActivity(ctx, session.ID, 0)
	if err != nil {
		t.Fatalf("ListActivity() error = %v", err)
	}
	wantKinds := []string{
		"round.created",
		"operation.created",
		"round.running",
		"operation.running",
		"operation.heartbeat",
		"round.complete",
		"operation.completed",
	}
	if len(activities) != len(wantKinds) {
		t.Fatalf("activity count = %d, want %d: %#v", len(activities), len(wantKinds), activities)
	}
	for index, wantKind := range wantKinds {
		if activities[index].ID <= 0 || activities[index].Kind != wantKind {
			t.Fatalf("activity[%d] = %#v, want ID and kind %q", index, activities[index], wantKind)
		}
		if index > 0 && activities[index].ID <= activities[index-1].ID {
			t.Fatalf("activity IDs are not increasing: %d then %d", activities[index-1].ID, activities[index].ID)
		}
	}
	if _, err := store.CreateOperation(ctx, session.ID, round.ID, OperationKindVerification); err != nil {
		t.Fatalf("CreateOperation() after complete error = %v", err)
	}
}

func TestOperationRecoveryAbandonsLeaseAndMarksRoundIncomplete(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	clock := time.Date(2026, time.July, 14, 13, 0, 0, 0, time.UTC)
	database, err := OpenState(ctx, filepath.Join(t.TempDir(), "state"))
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	defer database.Close()
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/recovery",
		DisplayName:       "recovery",
		DiscoveredGitDir:  "/workspaces/recovery/.git",
	}
	first := NewRepositoryStore(
		database,
		WithClock(func() time.Time { return clock }),
		WithProcessInstanceID("process-one"),
		WithOperationLeaseDuration(time.Second),
	)
	session, err := first.CreateSession(ctx, identity, "Recovery")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	round, err := first.CreateRound(ctx, session.ID)
	if err != nil {
		t.Fatalf("CreateRound() error = %v", err)
	}
	operation, err := first.CreateOperation(ctx, session.ID, round.ID, OperationKindReview)
	if err != nil {
		t.Fatalf("CreateOperation() error = %v", err)
	}
	if _, err := first.AcquireOperation(ctx, operation.ID); err != nil {
		t.Fatalf("AcquireOperation() error = %v", err)
	}

	clock = clock.Add(2 * time.Second)
	second := NewRepositoryStore(
		database,
		WithClock(func() time.Time { return clock }),
		WithProcessInstanceID("process-two"),
		WithOperationLeaseDuration(time.Second),
	)
	recovered, err := second.RecoverExpiredOperations(ctx)
	if err != nil {
		t.Fatalf("RecoverExpiredOperations() error = %v", err)
	}
	if len(recovered) != 1 || recovered[0].Status != OperationStatusAbandoned {
		t.Fatalf("recovered operations = %#v, want one abandoned operation", recovered)
	}
	got, err := second.GetOperation(ctx, operation.ID)
	if err != nil {
		t.Fatalf("GetOperation() after recovery error = %v", err)
	}
	if got.Status != OperationStatusAbandoned || got.Failure == "" {
		t.Fatalf("recovered operation = %#v", got)
	}
	gotRound, err := second.GetRound(ctx, round.ID)
	if err != nil {
		t.Fatalf("GetRound() after recovery error = %v", err)
	}
	if gotRound.Status != RoundStatusIncomplete {
		t.Fatalf("round status after recovery = %s, want incomplete", gotRound.Status)
	}
	if _, err := first.CompleteOperation(ctx, operation.ID); !errors.Is(err, ErrInvalidOperationTransition) {
		t.Fatalf("CompleteOperation() after recovery error = %v, want invalid transition", err)
	}
	if _, err := second.CreateOperation(ctx, session.ID, round.ID, OperationKindReview); err != nil {
		t.Fatalf("CreateOperation() after recovery error = %v", err)
	}
}

func TestOpenStoreRecoversExpiredOperationsAtStartup(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	stateDir := filepath.Join(t.TempDir(), "state")
	expiredAt := time.Date(2020, time.January, 1, 0, 0, 0, 0, time.UTC)
	database, err := OpenState(ctx, stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	first := NewRepositoryStore(
		database,
		WithClock(func() time.Time { return expiredAt }),
		WithProcessInstanceID("process-one"),
		WithOperationLeaseDuration(time.Second),
	)
	session, err := first.CreateSession(ctx, RepositoryIdentity{
		CanonicalIdentity: "/workspaces/startup-recovery",
		DisplayName:       "startup-recovery",
		DiscoveredGitDir:  "/workspaces/startup-recovery/.git",
	}, "Startup recovery")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	round, err := first.CreateRound(ctx, session.ID)
	if err != nil {
		t.Fatalf("CreateRound() error = %v", err)
	}
	operation, err := first.CreateOperation(ctx, session.ID, round.ID, OperationKindReview)
	if err != nil {
		t.Fatalf("CreateOperation() error = %v", err)
	}
	if _, err := first.AcquireOperation(ctx, operation.ID); err != nil {
		t.Fatalf("AcquireOperation() error = %v", err)
	}
	if err := first.Close(); err != nil {
		t.Fatalf("close first store: %v", err)
	}

	second, err := OpenStore(ctx, stateDir)
	if err != nil {
		t.Fatalf("OpenStore() error = %v", err)
	}
	defer second.Close()
	got, err := second.GetOperation(ctx, operation.ID)
	if err != nil {
		t.Fatalf("GetOperation() after startup recovery error = %v", err)
	}
	if got.Status != OperationStatusAbandoned {
		t.Fatalf("startup-recovered operation status = %s, want abandoned", got.Status)
	}
	gotRound, err := second.GetRound(ctx, round.ID)
	if err != nil {
		t.Fatalf("GetRound() after startup recovery error = %v", err)
	}
	if gotRound.Status != RoundStatusIncomplete {
		t.Fatalf("startup-recovered round status = %s, want incomplete", gotRound.Status)
	}
}

func TestCompetingOperationLeasesRejectSecondOwnerAndKeepReadsAvailable(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	database, err := OpenState(ctx, filepath.Join(t.TempDir(), "state"))
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	defer database.Close()
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/competing",
		DisplayName:       "competing",
		DiscoveredGitDir:  "/workspaces/competing/.git",
	}
	first := NewRepositoryStore(database, WithProcessInstanceID("process-one"))
	second := NewRepositoryStore(database, WithProcessInstanceID("process-two"))
	session, err := first.CreateSession(ctx, identity, "Competing")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	operation, err := first.CreateOperation(ctx, session.ID, "", OperationKindReview)
	if err != nil {
		t.Fatalf("CreateOperation() error = %v", err)
	}
	if _, err := second.CreateOperation(ctx, session.ID, "", OperationKindReview); !errors.Is(err, ErrOperationActive) {
		t.Fatalf("competing CreateOperation() error = %v, want ErrOperationActive", err)
	}
	if _, err := first.AcquireOperation(ctx, operation.ID); err != nil {
		t.Fatalf("first AcquireOperation() error = %v", err)
	}
	if _, err := second.AcquireOperation(ctx, operation.ID); !errors.Is(err, ErrOperationAlreadyOwned) {
		t.Fatalf("second AcquireOperation() error = %v, want ErrOperationAlreadyOwned", err)
	}
	if got, err := second.GetSession(ctx, session.ID); err != nil {
		t.Fatalf("read session while operation runs: %v", err)
	} else if got.ID != session.ID {
		t.Fatalf("read session = %#v", got)
	}
	if _, err := second.CancelOperation(ctx, operation.ID); err != nil {
		t.Fatalf("CancelOperation() error = %v", err)
	}
	cancelled, err := second.CancelOperation(ctx, operation.ID)
	if err != nil {
		t.Fatalf("idempotent CancelOperation() error = %v", err)
	}
	if cancelled.Status != OperationStatusCancelled {
		t.Fatalf("cancelled operation status = %s, want cancelled", cancelled.Status)
	}
}

func TestConcurrentOperationCreationLeavesOneActiveOperation(t *testing.T) {
	t.Parallel()

	ctx := context.Background()
	database, err := OpenState(ctx, filepath.Join(t.TempDir(), "state"))
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	defer database.Close()
	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/concurrent",
		DisplayName:       "concurrent",
		DiscoveredGitDir:  "/workspaces/concurrent/.git",
	}
	first := NewRepositoryStore(database, WithProcessInstanceID("process-one"))
	second := NewRepositoryStore(database, WithProcessInstanceID("process-two"))
	session, err := first.CreateSession(ctx, identity, "Concurrent")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	start := make(chan struct{})
	type result struct {
		operation Operation
		err       error
	}
	results := make(chan result, 2)
	for _, store := range []*RepositoryStore{first, second} {
		go func(store *RepositoryStore) {
			<-start
			operation, err := store.CreateOperation(ctx, session.ID, "", OperationKindReview)
			results <- result{operation: operation, err: err}
		}(store)
	}
	close(start)
	var successes, activeErrors int
	for range 2 {
		outcome := <-results
		if outcome.err == nil {
			successes++
			continue
		}
		if errors.Is(outcome.err, ErrOperationActive) {
			activeErrors++
			continue
		}
		t.Fatalf("concurrent CreateOperation() error = %v, want ErrOperationActive", outcome.err)
	}
	if successes != 1 || activeErrors != 1 {
		t.Fatalf("concurrent creation results = successes %d, active errors %d; want 1/1", successes, activeErrors)
	}
}

func TestValidateOperationTransitions(t *testing.T) {
	t.Parallel()

	valid := [][2]OperationStatus{
		{OperationStatusQueued, OperationStatusRunning},
		{OperationStatusQueued, OperationStatusCancelled},
		{OperationStatusRunning, OperationStatusComplete},
		{OperationStatusRunning, OperationStatusFailed},
		{OperationStatusRunning, OperationStatusCancelled},
		{OperationStatusRunning, OperationStatusAbandoned},
	}
	for _, transition := range valid {
		if err := ValidateOperationTransition(transition[0], transition[1]); err != nil {
			t.Errorf("ValidateOperationTransition(%s, %s) error = %v", transition[0], transition[1], err)
		}
	}
	invalid := [][2]OperationStatus{
		{OperationStatusQueued, OperationStatusComplete},
		{OperationStatusComplete, OperationStatusRunning},
		{OperationStatusCancelled, OperationStatusRunning},
		{OperationStatusAbandoned, OperationStatusComplete},
	}
	for _, transition := range invalid {
		if err := ValidateOperationTransition(
			transition[0],
			transition[1],
		); !errors.Is(
			err,
			ErrInvalidOperationTransition,
		) {
			t.Errorf(
				"ValidateOperationTransition(%s, %s) error = %v, want invalid transition",
				transition[0],
				transition[1],
				err,
			)
		}
	}
}
