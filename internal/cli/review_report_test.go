package cli

import (
	"bytes"
	"context"
	"errors"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/review"
)

func TestReviewRunsStaticReportWithProgressOnStderr(t *testing.T) {
	t.Setenv("NO_COLOR", "1")
	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeReviewFile(t, repositoryPath, "changed.txt", "old value\n")
	addReviewFile(t, worktree, "changed.txt")
	base := commitReview(t, worktree, "base", time.Date(2026, time.July, 15, 10, 0, 0, 0, time.UTC))
	writeReviewFile(t, repositoryPath, "changed.txt", "new value\n")
	addReviewFile(t, worktree, "changed.txt")
	target := commitReview(t, worktree, "target", time.Date(2026, time.July, 15, 11, 0, 0, 0, time.UTC))

	stateDir := filepath.Join(t.TempDir(), "state")
	var output, diagnostics, progress bytes.Buffer
	command := NewRootCommand(Config{
		Stdout: &output, Stderr: &diagnostics, Progress: &progress,
		StateDir: stateDir, WorkingDir: repositoryPath,
	})
	command.SetArgs([]string{"review", "--range", base.String() + ".." + target.String(), "--width", "48"})
	if err := command.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("review command error = %v", err)
	}
	if diagnostics.Len() != 0 {
		t.Fatalf("unexpected command diagnostics = %q", diagnostics.String())
	}
	for _, expected := range []string{
		"review: captured immutable snapshot",
		"review: assembling frozen change model",
		"review: persisted review ledger",
		"review: complete — 0 verified finding(s)",
	} {
		if !strings.Contains(progress.String(), expected) {
			t.Fatalf("progress missing %q: %q", expected, progress.String())
		}
	}
	for _, expected := range []string{"Review summary", "Review totals", "Changed files: 1", "Verified findings: 0", "Coverage summary"} {
		if !strings.Contains(output.String(), expected) {
			t.Fatalf("stdout missing %q: %q", expected, output.String())
		}
	}
	for _, hidden := range []string{"Diff", "-old value", "+new value"} {
		if strings.Contains(output.String(), hidden) {
			t.Fatalf("default output exposed verbose content %q: %q", hidden, output.String())
		}
	}
	if strings.Contains(output.String(), "review: captured") || strings.Contains(output.String(), "\x1b[") {
		t.Fatalf("stdout leaked progress or color: %q", output.String())
	}

	var verboseOutput bytes.Buffer
	verboseCommand := NewRootCommand(Config{
		Stdout: &verboseOutput, Stderr: &bytes.Buffer{}, Progress: &bytes.Buffer{},
		StateDir: filepath.Join(t.TempDir(), "verbose-state"), WorkingDir: repositoryPath,
	})
	verboseCommand.SetArgs(
		[]string{"review", "--range", base.String() + ".." + target.String(), "--verbose", "--width", "48"},
	)
	if err := verboseCommand.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("verbose review command error = %v", err)
	}
	for _, expected := range []string{"Review report", "Diff", "-old value", "+new value", "Verified findings"} {
		if !strings.Contains(verboseOutput.String(), expected) {
			t.Fatalf("verbose stdout missing %q: %q", expected, verboseOutput.String())
		}
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
	rounds, err := store.ListRounds(context.Background(), sessions[0].ID)
	if err != nil {
		t.Fatalf("ListRounds() error = %v", err)
	}
	if len(rounds) != 1 {
		t.Fatalf("rounds = %#v, want one", rounds)
	}
	round := rounds[0]
	if round.Status != db.RoundStatusComplete {
		t.Fatalf("round status = %q, want complete", round.Status)
	}
	coverage, err := store.GetReviewCoverage(context.Background(), round.ID)
	if err != nil || len(coverage.Passes) == 0 {
		t.Fatalf("coverage = %#v, error = %v", coverage, err)
	}
}

func TestReviewProviderFailureIsIncompleteAnalysis(t *testing.T) {
	repositoryPath := t.TempDir()
	repository := initReviewRepository(t, repositoryPath)
	worktree, err := repository.Worktree()
	if err != nil {
		t.Fatalf("Worktree() error = %v", err)
	}
	writeReviewFile(t, repositoryPath, "changed.txt", "old\n")
	addReviewFile(t, worktree, "changed.txt")
	base := commitReview(t, worktree, "base", time.Date(2026, time.July, 15, 12, 0, 0, 0, time.UTC))
	writeReviewFile(t, repositoryPath, "changed.txt", "new\n")
	addReviewFile(t, worktree, "changed.txt")
	target := commitReview(t, worktree, "target", time.Date(2026, time.July, 15, 13, 0, 0, 0, time.UTC))

	stateDir := filepath.Join(t.TempDir(), "state")
	var output, progress bytes.Buffer
	command := NewRootCommand(Config{
		Stdout: &output, Stderr: &bytes.Buffer{}, Progress: &progress,
		StateDir: stateDir, WorkingDir: repositoryPath, Model: failingReviewModel{},
	})
	command.SetArgs([]string{"review", "--range", base.String() + ".." + target.String(), "--width", "48"})
	if err := command.ExecuteContext(context.Background()); err != nil {
		t.Fatalf("review command error = %v", err)
	}
	for _, expected := range []string{"Status: incomplete", "Incomplete analysis:", "No review passes were persisted."} {
		if !strings.Contains(output.String(), expected) {
			t.Fatalf("stdout missing %q: %q", expected, output.String())
		}
	}
	if !strings.Contains(progress.String(), "review: incomplete analysis") {
		t.Fatalf("progress = %q", progress.String())
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
	rounds, err := store.ListRounds(context.Background(), sessions[0].ID)
	if err != nil {
		t.Fatalf("ListRounds() error = %v", err)
	}
	if len(rounds) != 1 {
		t.Fatalf("rounds = %#v, want one", rounds)
	}
	round := rounds[0]
	if round.Status != db.RoundStatusIncomplete {
		t.Fatalf("round status = %q, want incomplete", round.Status)
	}
}

type failingReviewModel struct{}

func (failingReviewModel) Complete(context.Context, review.ModelRequest) (review.ModelResponse, error) {
	return review.ModelResponse{}, errors.New("provider unavailable")
}
