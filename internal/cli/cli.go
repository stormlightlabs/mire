// Package cli defines MIRE's Cobra command tree and owns [os.Stdout]/[Os.Stderr] boundaries.
package cli

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"
	"github.com/stormlightlabs/mire/internal/db"
	"github.com/stormlightlabs/mire/internal/gitrepo"
	"github.com/stormlightlabs/mire/internal/snapshot"
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
	Stdout      io.Writer
	Stderr      io.Writer
	StateDir    string
	WorkingDir  string
	Store       *db.RepositoryStore
	ObjectStore *snapshot.ObjectStore
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

func (state *commandContext) openObjectStore() (*snapshot.ObjectStore, error) {
	if state.config.ObjectStore != nil {
		return state.config.ObjectStore, nil
	}
	stateDir := state.config.StateDir
	if stateDir == "" {
		var err error
		stateDir, err = db.DefaultStateDirectory()
		if err != nil {
			return nil, err
		}
	}
	return snapshot.OpenObjectStore(stateDir)
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
	root.AddCommand(newReviewCommand(state))
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
	identity, err := gitrepo.Discover(ctx, directory)
	if errors.Is(err, gitrepo.ErrNotGitRepository) {
		return db.RepositoryIdentity{}, fmt.Errorf("%w: %s", ErrNotGitRepository, directory)
	}
	if err != nil {
		return db.RepositoryIdentity{}, fmt.Errorf("discover Git repository: %w", err)
	}
	return identity, nil
}
