package cli

import (
	"bytes"
	"context"
	"io"
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

func TestReviewThreeDotPersistsRequestedAndEffectiveBaseProvenance(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeReviewFile(t, repositoryPath, "common.txt", "common\n")
	addReviewFile(t, worktree, "common.txt")
	base := commitReview(t, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	if err := repository.Storer.SetReference(plumbing.NewHashReference(
		plumbing.NewBranchReferenceName("base"), base)); err != nil {
		t.Fatalf("create base branch: %v", err)
	}
	writeReviewFile(t, repositoryPath, "target.txt", "target\n")
	addReviewFile(t, worktree, "target.txt")
	target := commitReview(t, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	stateDir := filepath.Join(t.TempDir(), "state")
	var output, diagnostics bytes.Buffer
	command := NewRootCommand(Config{
		Stdout: &output, Stderr: &diagnostics, StateDir: stateDir, WorkingDir: repositoryPath,
	})
	command.SetArgs([]string{"review", "--range", "base...HEAD"})
	if err := command.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("review command error = %v", err)
	}
	if !strings.Contains(output.String(), "Kind: three_dot") || !strings.Contains(output.String(), "Merge base: "+base.String()) {
		t.Fatalf("stdout = %q", output.String())
	}
	if diagnostics.Len() != 0 {
		t.Fatalf("stderr = %q", diagnostics.String())
	}
	writeReviewFile(t, repositoryPath, "target.txt", "moved after capture\n")
	addReviewFile(t, worktree, "target.txt")
	commitReview(t, worktree, "move target ref", time.Date(2026, time.July, 14, 12, 0, 0, 0, time.UTC))

	store, err := db.OpenStore(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenStore() error = %v", err)
	}
	defer store.Close()
	sessions, err := store.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions() error = %v", err)
	}
	round, err := store.GetRound(context.Background(), sessions[0].CurrentRoundID)
	if err != nil {
		t.Fatalf("GetRound() error = %v", err)
	}
	persisted, err := store.GetSnapshot(context.Background(), round.SnapshotID)
	if err != nil {
		t.Fatalf("GetSnapshot() error = %v", err)
	}
	if persisted.Kind != snapshot.ComparisonThreeDot || persisted.BaseOID != base.String() ||
		persisted.EffectiveBaseOID != base.String() || persisted.MergeBaseOID != base.String() ||
		persisted.TargetOID != target.String() {
		t.Fatalf("persisted provenance = %#v", persisted)
	}
}

func TestReviewWorktreePersistsAllThreeLayersAndIgnoredPolicy(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeReviewFile(t, repositoryPath, "shared.txt", "head\n")
	writeReviewFile(t, repositoryPath, ".gitignore", "ignored.txt\n")
	addReviewFile(t, worktree, "shared.txt")
	addReviewFile(t, worktree, ".gitignore")
	commitReview(t, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	writeReviewFile(t, repositoryPath, "shared.txt", "index\n")
	addReviewFile(t, worktree, "shared.txt")
	writeReviewFile(t, repositoryPath, "shared.txt", "final\n")
	writeReviewFile(t, repositoryPath, "new.txt", "untracked\n")
	writeReviewFile(t, repositoryPath, "ignored.txt", "ignored\n")

	stateDir := filepath.Join(t.TempDir(), "state")
	var output, diagnostics bytes.Buffer
	command := NewRootCommand(Config{
		Stdout: &output, Stderr: &diagnostics, StateDir: stateDir, WorkingDir: repositoryPath,
	})
	command.SetArgs([]string{"review", "--worktree"})
	if err := command.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("worktree review command error = %v", err)
	}
	if !strings.Contains(output.String(), "Kind: worktree") || !strings.Contains(output.String(), "Index:") {
		t.Fatalf("stdout = %q", output.String())
	}
	if diagnostics.Len() != 0 {
		t.Fatalf("stderr = %q", diagnostics.String())
	}

	store, err := db.OpenStore(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenStore() error = %v", err)
	}
	defer store.Close()
	sessions, err := store.ListSessions(context.Background())
	if err != nil || len(sessions) != 1 {
		t.Fatalf("sessions = %#v, error = %v", sessions, err)
	}
	round, err := store.GetRound(context.Background(), sessions[0].CurrentRoundID)
	if err != nil {
		t.Fatalf("GetRound() error = %v", err)
	}
	persisted, err := store.GetSnapshot(context.Background(), round.SnapshotID)
	if err != nil {
		t.Fatalf("GetSnapshot() error = %v", err)
	}
	if persisted.Kind != snapshot.ComparisonWorktree || persisted.BaseOID == "" || persisted.IndexOID == "" || persisted.TargetOID == "" || persisted.IgnorePolicy == "" {
		t.Fatalf("worktree snapshot = %#v", persisted)
	}
	if len(persisted.Layers) != 3 {
		t.Fatalf("snapshot layers = %#v", persisted.Layers)
	}
	headEntries, err := store.ListSnapshotEntries(context.Background(), persisted.ID, snapshot.TreeSideHead)
	if err != nil {
		t.Fatalf("ListSnapshotEntries(head) error = %v", err)
	}
	indexEntries, err := store.ListSnapshotEntries(context.Background(), persisted.ID, snapshot.TreeSideIndex)
	if err != nil {
		t.Fatalf("ListSnapshotEntries(index) error = %v", err)
	}
	worktreeEntries, err := store.ListSnapshotEntries(context.Background(), persisted.ID, snapshot.TreeSideWorktree)
	if err != nil {
		t.Fatalf("ListSnapshotEntries(worktree) error = %v", err)
	}
	if findReviewEntry(headEntries, "new.txt").Path != "" || findReviewEntry(indexEntries, "new.txt").Path != "" {
		t.Fatalf("untracked file leaked into prior layers: head=%#v index=%#v", headEntries, indexEntries)
	}
	if findReviewEntry(worktreeEntries, "new.txt").Path == "" || findReviewEntry(worktreeEntries, "ignored.txt").Path != "" {
		t.Fatalf("final worktree entries = %#v", worktreeEntries)
	}
	objectStore, err := snapshot.OpenObjectStore(stateDir)
	if err != nil {
		t.Fatalf("OpenObjectStore() error = %v", err)
	}
	finalObject, err := objectStore.Open(findReviewEntry(worktreeEntries, "shared.txt").ContentDigest)
	if err != nil {
		t.Fatalf("open final shared object: %v", err)
	}
	finalContent, err := io.ReadAll(finalObject)
	_ = finalObject.Close()
	if err != nil || string(finalContent) != "final\n" {
		t.Fatalf("final shared object = %q, error = %v", finalContent, err)
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

func TestReviewSessionAppendsWorktreeRoundAndShowReportsHistory(t *testing.T) {
	t.Parallel()

	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeReviewFile(t, repositoryPath, "file.txt", "base\n")
	addReviewFile(t, worktree, "file.txt")
	base := commitReview(t, worktree, "base", time.Date(2026, time.July, 14, 10, 0, 0, 0, time.UTC))
	writeReviewFile(t, repositoryPath, "file.txt", "target\n")
	addReviewFile(t, worktree, "file.txt")
	target := commitReview(t, worktree, "target", time.Date(2026, time.July, 14, 11, 0, 0, 0, time.UTC))

	stateDir := filepath.Join(t.TempDir(), "state")
	var firstOutput bytes.Buffer
	first := NewRootCommand(Config{Stdout: &firstOutput, Stderr: &bytes.Buffer{}, StateDir: stateDir, WorkingDir: repositoryPath})
	first.SetArgs([]string{"review", "--range", base.String() + ".." + target.String()})
	if err := first.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("initial review error = %v", err)
	}
	var sessionID string
	for _, line := range strings.Split(firstOutput.String(), "\n") {
		if strings.HasPrefix(line, "Session: ") {
			sessionID = strings.TrimSpace(strings.TrimPrefix(line, "Session: "))
		}
	}
	if sessionID == "" {
		t.Fatalf("initial output = %q, missing session ID", firstOutput.String())
	}

	writeReviewFile(t, repositoryPath, "file.txt", "working tree\n")
	writeReviewFile(t, repositoryPath, "new.txt", "new\n")
	var appendOutput bytes.Buffer
	appendCommand := NewRootCommand(Config{Stdout: &appendOutput, Stderr: &bytes.Buffer{}, StateDir: stateDir, WorkingDir: repositoryPath})
	appendCommand.SetArgs([]string{"review", "--session", sessionID, "--worktree"})
	if err := appendCommand.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("append review error = %v", err)
	}
	if !strings.Contains(appendOutput.String(), "Round:") || !strings.Contains(appendOutput.String(), "Kind: worktree") {
		t.Fatalf("append output = %q", appendOutput.String())
	}

	store, err := db.OpenStore(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenStore() error = %v", err)
	}
	defer store.Close()
	session, err := store.GetSession(context.Background(), sessionID)
	if err != nil {
		t.Fatalf("GetSession() error = %v", err)
	}
	rounds, err := store.ListRounds(context.Background(), session.ID)
	if err != nil {
		t.Fatalf("ListRounds() error = %v", err)
	}
	if len(rounds) != 2 || rounds[1].PredecessorRoundID != rounds[0].ID || session.CurrentRoundID != rounds[1].ID {
		t.Fatalf("session history = %#v, session = %#v", rounds, session)
	}

	var showOutput bytes.Buffer
	show := NewRootCommand(Config{Stdout: &showOutput, Stderr: &bytes.Buffer{}, StateDir: stateDir, WorkingDir: repositoryPath})
	show.SetArgs([]string{"show", sessionID})
	if err := show.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("show error = %v", err)
	}
	if strings.Count(showOutput.String(), "Divergence:") != 2 || !strings.Contains(showOutput.String(), "Round 2") {
		t.Fatalf("show output = %q", showOutput.String())
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
