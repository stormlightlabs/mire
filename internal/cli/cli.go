// Package cli defines MIRE's Cobra command tree and owns [os.Stdout]/[Os.Stderr] boundaries.
package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
)

var (
	// [ErrNotGitRepository] is returned when a current-repository
	// operation is requested outside a Git worktree.
	ErrNotGitRepository = errors.New("not inside a Git repository")
)

// Config wires process dependencies into the CLI.
//
// Store is primarily useful for tests and application-service callers.
// Normal commands open the private state store lazily when a stateful command runs.
type Config struct {
	Stdout     io.Writer
	Stderr     io.Writer
	StateDir   string
	WorkingDir string
	Store      *db.RepositoryStore
}

type commandContext struct {
	config Config
}

func (state *commandContext) openStore(ctx context.Context) (*db.RepositoryStore, func(), error) {
	if state.config.Store != nil {
		return state.config.Store, func() {}, nil
	}
	store, err := db.OpenStore(ctx, state.config.StateDir)
	if err != nil {
		return nil, nil, err
	}
	return store, func() { _ = store.Close() }, nil
}

func (state *commandContext) currentRepository(ctx context.Context) (db.RepositoryIdentity, error) {
	return DiscoverCurrentRepository(ctx, state.config.WorkingDir)
}

// NewRootCommand builds the root [cobra.Command].
func NewRootCommand(config Config) *cobra.Command {
	if config.Stdout == nil {
		config.Stdout = os.Stdout
	}
	if config.Stderr == nil {
		config.Stderr = os.Stderr
	}

	state := &commandContext{config: config}
	root := &cobra.Command{
		Use:           "mire",
		Short:         "A local, model-independent code-review workbench",
		SilenceErrors: true,
		SilenceUsage:  true,
	}
	root.SetOut(config.Stdout)
	root.SetErr(config.Stderr)
	root.AddCommand(newSessionsCommand(state))
	return root
}

// Execute runs the process command using [os.Args] and writes diagnostics via
// the root command's configured [os.Stderr].
func Execute(ctx context.Context) error {
	return NewRootCommand(Config{}).ExecuteContext(ctx)
}

// DiscoverCurrentRepository performs only read-only Git metadata queries
// and returns the canonical identity needed by the application service.
func DiscoverCurrentRepository(ctx context.Context, directory string) (db.RepositoryIdentity, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if strings.TrimSpace(directory) == "" {
		var err error
		directory, err = os.Getwd()
		if err != nil {
			return db.RepositoryIdentity{}, fmt.Errorf("find current directory: %w", err)
		}
	}
	directory, err := filepath.Abs(directory)
	if err != nil {
		return db.RepositoryIdentity{}, fmt.Errorf("resolve current directory: %w", err)
	}

	command := exec.CommandContext(ctx, "git", "-C", directory, "rev-parse", "--show-toplevel", "--absolute-git-dir")
	output, err := command.Output()
	if err != nil {
		var exitError *exec.ExitError
		if errors.As(err, &exitError) {
			if strings.Contains(strings.ToLower(string(exitError.Stderr)), "not a git repository") {
				return db.RepositoryIdentity{}, fmt.Errorf("%w: %s", ErrNotGitRepository, directory)
			}
		}
		return db.RepositoryIdentity{}, fmt.Errorf("discover Git repository from %s: %w", directory, err)
	}

	lines := strings.Split(strings.TrimSpace(string(output)), "\n")
	if len(lines) != 2 || strings.TrimSpace(lines[0]) == "" || strings.TrimSpace(lines[1]) == "" {
		return db.RepositoryIdentity{}, fmt.Errorf("discover Git repository from %s: unexpected Git metadata output", directory)
	}

	root, err := canonicalPath(strings.TrimSpace(lines[0]))
	if err != nil {
		return db.RepositoryIdentity{}, fmt.Errorf("canonicalize Git worktree: %w", err)
	}
	gitDir, err := canonicalPath(strings.TrimSpace(lines[1]))
	if err != nil {
		return db.RepositoryIdentity{}, fmt.Errorf("canonicalize Git directory: %w", err)
	}

	displayName := filepath.Base(root)
	if displayName == string(filepath.Separator) || displayName == "." || displayName == "" {
		displayName = root
	}
	return db.RepositoryIdentity{
		CanonicalIdentity: root,
		DisplayName:       displayName,
		DiscoveredGitDir:  gitDir,
	}, nil
}

func canonicalPath(path string) (string, error) {
	absolute, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(absolute)
	if err != nil {
		return "", err
	}
	return filepath.Clean(resolved), nil
}
