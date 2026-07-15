package db

import (
	"context"
	"errors"
	"path/filepath"
	"testing"
	"time"
)

func TestRepositoryStoreSessionLifecycleSurvivesRestart(t *testing.T) {
	t.Parallel()

	stateDir := t.TempDir()
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}

	createdAt := time.Date(2026, time.July, 14, 12, 30, 0, 123, time.UTC)
	ids := []string{"repository-id", "session-id"}
	nextID := func() (string, error) {
		id := ids[0]
		ids = ids[1:]
		return id, nil
	}
	store := NewRepositoryStore(
		database,
		WithClock(func() time.Time { return createdAt }),
		WithIDGenerator(nextID),
	)

	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/example",
		DisplayName:       "example",
		DiscoveredGitDir:  "/workspaces/example/.git",
	}
	session, err := store.CreateSession(context.Background(), identity, "Initial review")
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	if session.ID != "session-id" {
		t.Fatalf("session ID = %q, want session-id", session.ID)
	}
	if session.RepositoryID != "repository-id" {
		t.Fatalf("repository ID = %q, want repository-id", session.RepositoryID)
	}
	if err := store.Close(); err != nil {
		t.Fatalf("close first store: %v", err)
	}

	restartedDatabase, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("reopen state: %v", err)
	}
	restartedStore := NewRepositoryStore(restartedDatabase)
	t.Cleanup(func() {
		if err := restartedStore.Close(); err != nil {
			t.Errorf("close restarted store: %v", err)
		}
	})

	listed, err := restartedStore.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions() error = %v", err)
	}
	if len(listed) != 1 {
		t.Fatalf("listed sessions = %d, want 1", len(listed))
	}
	if listed[0].ID != session.ID || listed[0].RepositoryID != session.RepositoryID {
		t.Fatalf("listed session = %#v, want %#v", listed[0], session)
	}
	if !listed[0].CreatedAt.Equal(createdAt) {
		t.Fatalf("listed CreatedAt = %s, want %s", listed[0].CreatedAt, createdAt)
	}
	if listed[0].RepositoryName != "example" || listed[0].RepositoryIdentity != identity.CanonicalIdentity {
		t.Fatalf("listed repository metadata = %#v", listed[0])
	}

	loaded, err := restartedStore.GetSession(context.Background(), session.ID)
	if err != nil {
		t.Fatalf("GetSession() error = %v", err)
	}
	if loaded.Title != "Initial review" {
		t.Fatalf("loaded title = %q, want Initial review", loaded.Title)
	}

	if err := restartedStore.DeleteSession(context.Background(), session.ID); err != nil {
		t.Fatalf("DeleteSession() error = %v", err)
	}
	if _, err := restartedStore.GetSession(context.Background(), session.ID); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("GetSession() after delete error = %v, want ErrSessionNotFound", err)
	}
	if err := restartedStore.DeleteSession(context.Background(), session.ID); !errors.Is(err, ErrSessionNotFound) {
		t.Fatalf("second DeleteSession() error = %v, want ErrSessionNotFound", err)
	}
}

func TestRepositoryStoreReusesRepositoryIdentity(t *testing.T) {
	t.Parallel()

	stateDir := filepath.Join(t.TempDir(), "state")
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	store := NewRepositoryStore(database)
	t.Cleanup(func() {
		if err := store.Close(); err != nil {
			t.Errorf("Close() error = %v", err)
		}
	})

	identity := RepositoryIdentity{
		CanonicalIdentity: "/workspaces/example",
		DisplayName:       "example",
		DiscoveredGitDir:  "/workspaces/example/.git",
	}
	first, err := store.CreateSession(context.Background(), identity, "First")
	if err != nil {
		t.Fatalf("first CreateSession() error = %v", err)
	}
	second, err := store.CreateSession(context.Background(), identity, "Second")
	if err != nil {
		t.Fatalf("second CreateSession() error = %v", err)
	}
	if first.RepositoryID != second.RepositoryID {
		t.Fatalf("repository IDs = %q and %q, want stable identity", first.RepositoryID, second.RepositoryID)
	}

	sessions, err := store.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions() error = %v", err)
	}
	if len(sessions) != 2 {
		t.Fatalf("listed sessions = %d, want 2", len(sessions))
	}
}

func TestCreateSessionRejectsIncompleteIdentityBeforeWriting(t *testing.T) {
	t.Parallel()

	database, err := OpenState(context.Background(), t.TempDir())
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	store := NewRepositoryStore(database)
	t.Cleanup(func() {
		_ = store.Close()
	})

	_, err = store.CreateSession(context.Background(), RepositoryIdentity{}, "Review")
	if !errors.Is(err, ErrInvalidRepositoryIdentity) {
		t.Fatalf("CreateSession() error = %v, want ErrInvalidRepositoryIdentity", err)
	}

	var repositories int
	if err := database.QueryRow("SELECT COUNT(*) FROM repositories").Scan(&repositories); err != nil {
		t.Fatalf("count repositories: %v", err)
	}
	if repositories != 0 {
		t.Fatalf("repositories after rejected create = %d, want 0", repositories)
	}
}
