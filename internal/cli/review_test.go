package cli

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	git "github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing"
	"github.com/go-git/go-git/v6/plumbing/object"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/snapshot"
)

func TestReviewRangeCreatesSessionRoundAndPrivateSnapshot(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeReviewFile(t, repositoryPath, "unchanged.txt", "unchanged\n")
	addReviewFile(t, worktree, "unchanged.txt")
	base := commitReview(t, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	writeReviewFile(t, repositoryPath, "changed.txt", "changed\n")
	addReviewFile(t, worktree, "changed.txt")
	target := commitReview(t, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	stateDir := filepath.Join(t.TempDir(), "state")
	var output, diagnostics bytes.Buffer
	command := NewRootCommand(Config{
		Stdout: &output, Stderr: &diagnostics, StateDir: stateDir, WorkingDir: repositoryPath,
	})
	command.SetArgs([]string{"review", "--range", base.String() + ".." + target.String()})
	if err := command.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("review command error = %v", err)
	}
	if !strings.Contains(output.String(), "Captured review") || !strings.Contains(output.String(), "Snapshot:") {
		t.Fatalf("stdout = %q", output.String())
	}
	if diagnostics.Len() != 0 {
		t.Fatalf("stderr = %q, want empty", diagnostics.String())
	}
	if err := os.RemoveAll(filepath.Join(repositoryPath, ".git")); err != nil {
		t.Fatalf("remove fixture Git objects: %v", err)
	}

	store, err := db.OpenStore(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenStore() error = %v", err)
	}
	sessions, err := store.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions() error = %v", err)
	}
	if len(sessions) != 1 {
		t.Fatalf("sessions = %#v, want one", sessions)
	}
	round, err := store.GetRound(context.Background(), sessions[0].CurrentRoundID)
	if err != nil {
		t.Fatalf("GetRound() error = %v", err)
	}
	persisted, err := store.GetSnapshot(context.Background(), round.SnapshotID)
	if err != nil {
		t.Fatalf("GetSnapshot() error = %v", err)
	}
	if persisted.EffectiveBaseOID != base.String() || persisted.TargetOID != target.String() || !persisted.Complete {
		t.Fatalf("snapshot = %#v", persisted)
	}
	entries, err := store.ListSnapshotEntries(context.Background(), persisted.ID, snapshot.TreeSideTarget)
	if err != nil {
		t.Fatalf("ListSnapshotEntries() error = %v", err)
	}
	if len(entries) != 2 {
		t.Fatalf("target entries = %#v, want complete two-entry tree", entries)
	}
	objectStore, err := snapshot.OpenObjectStore(stateDir)
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	object, err := objectStore.Open(findReviewEntry(entries, "changed.txt").ContentDigest)
	if err != nil {
		t.Fatalf("Open() captured object error = %v", err)
	}
	content, err := os.ReadFile(object.Name())
	_ = object.Close()
	if err != nil || string(content) != "changed\n" {
		t.Fatalf("stored object = %q, error=%v", content, err)
	}
	if err := store.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
}

func TestReviewInvalidRangeCreatesNoDatabaseRecord(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	_ = initReviewRepository(t, repositoryPath)
	stateDir := filepath.Join(t.TempDir(), "state")
	var output, diagnostics bytes.Buffer
	command := NewRootCommand(Config{
		Stdout: &output, Stderr: &diagnostics, StateDir: stateDir, WorkingDir: repositoryPath,
	})
	command.SetArgs([]string{"review", "--range", "missing..HEAD"})
	if err := command.ExecuteContext(context.Background()); err == nil || !strings.Contains(err.Error(), "resolve base revision") {
		t.Fatalf("invalid review error = %v", err)
	}
	if output.Len() != 0 || diagnostics.Len() != 0 {
		t.Fatalf("invalid review output stdout=%q stderr=%q", output.String(), diagnostics.String())
	}
	if _, err := os.Stat(filepath.Join(stateDir, db.DatabaseFilename)); !os.IsNotExist(err) {
		t.Fatalf("database after failed capture stat error = %v, want database absent", err)
	}
}

func initReviewRepository(t *testing.T, path string) *git.Repository {
	t.Helper()
	repository, err := git.PlainInit(path, false)
	if err != nil {
		t.Fatalf("PlainInit() error = %v", err)
	}
	config, err := repository.Config()
	if err != nil {
		t.Fatalf("Config() error = %v", err)
	}
	config.User.Name = "MIRE Test"
	config.User.Email = "mire@example.test"
	if err := repository.SetConfig(config); err != nil {
		t.Fatalf("SetConfig() error = %v", err)
	}
	return repository
}

func writeReviewFile(t *testing.T, root, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(root, name), []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", name, err)
	}
}

func addReviewFile(t *testing.T, worktree *git.Worktree, name string) {
	t.Helper()
	if _, err := worktree.Add(name); err != nil {
		t.Fatalf("add %s: %v", name, err)
	}
}

func commitReview(t *testing.T, worktree *git.Worktree, message string, when time.Time) plumbing.Hash {
	t.Helper()
	hash, err := worktree.Commit(message, &git.CommitOptions{
		Author:    &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: when},
		Committer: &object.Signature{Name: "MIRE Test", Email: "mire@example.test", When: when},
	})
	if err != nil {
		t.Fatalf("Commit() error = %v", err)
	}
	return hash
}

func findReviewEntry(entries []db.SnapshotEntry, name string) db.SnapshotEntry {
	for _, entry := range entries {
		if entry.Path == name {
			return entry
		}
	}
	return db.SnapshotEntry{}
}
