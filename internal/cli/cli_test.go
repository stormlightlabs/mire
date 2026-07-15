package cli

import (
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stormlightlabs/mire/internal/db"
)

func TestSessionsCommandsPersistListAndDeleteMetadata(t *testing.T) {
	t.Parallel()

	stateDir := filepath.Join(t.TempDir(), "private-state")
	workingDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("get working directory: %v", err)
	}
	identity, err := DiscoverCurrentRepository(context.Background(), workingDir)
	if err != nil {
		t.Fatalf("discover test repository: %v", err)
	}
	var firstOutput, firstDiagnostics bytes.Buffer
	firstCommand := NewRootCommand(Config{
		Stdout:     &firstOutput,
		Stderr:     &firstDiagnostics,
		StateDir:   stateDir,
		WorkingDir: workingDir,
	})
	firstCommand.SetArgs([]string{"sessions", "list"})
	if err := firstCommand.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("initial sessions list: %v", err)
	}
	if !strings.Contains(firstOutput.String(), "No sessions found.") {
		t.Fatalf("initial stdout = %q, want empty-state message", firstOutput.String())
	}
	if firstDiagnostics.Len() != 0 {
		t.Fatalf("initial diagnostics = %q, want empty", firstDiagnostics.String())
	}

	store, err := db.OpenStore(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("open fixture store: %v", err)
	}
	session, err := store.CreateSession(context.Background(), identity, "lifecycle")
	if err != nil {
		t.Fatalf("create fixture session: %v", err)
	}
	if err := store.Close(); err != nil {
		t.Fatalf("close fixture store: %v", err)
	}

	var listOutput, listDiagnostics bytes.Buffer
	listCommand := NewRootCommand(Config{
		Stdout:     &listOutput,
		Stderr:     &listDiagnostics,
		StateDir:   stateDir,
		WorkingDir: workingDir,
	})
	listCommand.SetArgs([]string{"sessions", "list"})
	if err := listCommand.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("persisted sessions list: %v", err)
	}
	for _, expected := range []string{session.ID, "lifecycle", "mire"} {
		if !strings.Contains(listOutput.String(), expected) {
			t.Fatalf("persisted stdout = %q, missing %q", listOutput.String(), expected)
		}
	}
	if listDiagnostics.Len() != 0 {
		t.Fatalf("persisted diagnostics = %q, want empty", listDiagnostics.String())
	}

	var deleteOutput, deleteDiagnostics bytes.Buffer
	deleteCommand := NewRootCommand(Config{
		Stdout:     &deleteOutput,
		Stderr:     &deleteDiagnostics,
		StateDir:   stateDir,
		WorkingDir: workingDir,
	})
	deleteCommand.SetArgs([]string{"sessions", "delete", session.ID})
	if err := deleteCommand.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("delete session: %v", err)
	}
	if !strings.Contains(deleteOutput.String(), "Deleted session "+session.ID) {
		t.Fatalf("delete stdout = %q", deleteOutput.String())
	}
	if deleteDiagnostics.Len() != 0 {
		t.Fatalf("delete diagnostics = %q, want empty", deleteDiagnostics.String())
	}
}

func TestSessionsDeleteUnknownReturnsClearError(t *testing.T) {
	t.Parallel()

	var output, diagnostics bytes.Buffer
	workingDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("get working directory: %v", err)
	}
	command := NewRootCommand(Config{
		Stdout:     &output,
		Stderr:     &diagnostics,
		StateDir:   filepath.Join(t.TempDir(), "state"),
		WorkingDir: workingDir,
	})
	command.SetArgs([]string{"sessions", "delete", "missing-session"})
	err = command.ExecuteContext(context.Background())
	if err == nil || !strings.Contains(err.Error(), "session does not exist") {
		t.Fatalf("unknown delete error = %v, want clear missing-session error", err)
	}
	if output.Len() != 0 || diagnostics.Len() != 0 {
		t.Fatalf("unknown delete wrote stdout=%q stderr=%q", output.String(), diagnostics.String())
	}
}

func TestDiscoverCurrentRepositoryOutsideGit(t *testing.T) {
	t.Parallel()

	_, err := DiscoverCurrentRepository(context.Background(), t.TempDir())
	if !errors.Is(err, ErrNotGitRepository) {
		t.Fatalf("DiscoverCurrentRepository() error = %v, want ErrNotGitRepository", err)
	}
}
