package cli

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
)

func TestExportCommandWritesExplicitFormatAndProtectsDestination(t *testing.T) {
	t.Parallel()
	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatal(err)
	}
	writeReviewFile(t, repositoryPath, "export.txt", "base\n")
	addReviewFile(t, worktree, "export.txt")
	base := commitReview(t, worktree, "base", time.Date(2026, time.July, 15, 9, 0, 0, 0, time.UTC))
	writeReviewFile(t, repositoryPath, "export.txt", "target\n")
	addReviewFile(t, worktree, "export.txt")
	target := commitReview(t, worktree, "target", time.Date(2026, time.July, 15, 10, 0, 0, 0, time.UTC))
	stateDir := filepath.Join(t.TempDir(), "state")
	var reviewOutput, reviewDiagnostics bytes.Buffer
	reviewCommand := NewRootCommand(
		Config{Stdout: &reviewOutput, Stderr: &reviewDiagnostics, StateDir: stateDir, WorkingDir: repositoryPath},
	)
	reviewCommand.SetArgs([]string{"review", "--range", base.String() + ".." + target.String()})
	if err := reviewCommand.ExecuteContext(context.Background()); err != nil {
		t.Fatal(err)
	}
	store, err := db.OpenStore(context.Background(), stateDir)
	if err != nil {
		t.Fatal(err)
	}
	sessions, err := store.ListSessions(context.Background())
	_ = store.Close()
	if err != nil || len(sessions) != 1 {
		t.Fatalf("sessions = %#v, error = %v", sessions, err)
	}
	destination := filepath.Join(t.TempDir(), "review.json")
	var output, diagnostics bytes.Buffer
	command := NewRootCommand(
		Config{Stdout: &output, Stderr: &diagnostics, StateDir: stateDir, WorkingDir: repositoryPath},
	)
	command.SetArgs([]string{"export", sessions[0].ID, "--format", "json", "--output", destination})
	if err := command.ExecuteContext(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(destination); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(diagnostics.String(), "sensitive") ||
		!strings.Contains(output.String(), "Exported json review") {
		t.Fatalf("stdout=%q stderr=%q", output.String(), diagnostics.String())
	}
	command = NewRootCommand(
		Config{Stdout: &output, Stderr: &diagnostics, StateDir: stateDir, WorkingDir: repositoryPath},
	)
	command.SetArgs([]string{"export", sessions[0].ID, "--format", "json", "--output", destination})
	if err := command.ExecuteContext(context.Background()); err == nil {
		t.Fatal("export overwrote existing destination")
	}
}
