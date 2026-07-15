// Package db owns MIRE's private SQLite state and its persistence boundaries.
package db

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	_ "modernc.org/sqlite"
)

const (
	// StateDirectoryEnv allows tests and explicitly isolated invocations to use a
	// temporary state directory without putting state in a reviewed repository.
	StateDirectoryEnv = "MIRE_STATE_DIR"

	// DatabaseFilename is the single global database used by the store.
	DatabaseFilename = "mire.sqlite3"

	busyTimeout = 5000
)

var (
	// ErrInvalidStateDirectory indicates that a state path is not a private
	// directory suitable for application data.
	ErrInvalidStateDirectory = errors.New("invalid private state directory")
)

// DB is an opened, migrated MIRE database.
type DB struct {
	*sql.DB
	Path string
}

// DefaultStateDirectory returns the operating system's per-user application
// configuration directory with MIRE's state directory appended.
// MIRE_STATE_DIR is intentionally an explicit override for tests and isolated
// local runs.
func DefaultStateDirectory() (string, error) {
	if stateDir := strings.TrimSpace(os.Getenv(StateDirectoryEnv)); stateDir != "" {
		absolute, err := filepath.Abs(stateDir)
		if err != nil {
			return "", fmt.Errorf("resolve %s: %w", StateDirectoryEnv, err)
		}
		return filepath.Clean(absolute), nil
	}

	configDir, err := os.UserConfigDir()
	if err != nil {
		return "", fmt.Errorf("find per-user application state directory: %w", err)
	}
	return filepath.Join(configDir, "mire"), nil
}

// OpenState opens the database in stateDir, creating and migrating it
// when necessary. An empty stateDir uses [DefaultStateDirectory].
func OpenState(ctx context.Context, stateDir string) (*DB, error) {
	if strings.TrimSpace(stateDir) == "" {
		var err error
		stateDir, err = DefaultStateDirectory()
		if err != nil {
			return nil, err
		}
	}

	stateDir, err := filepath.Abs(stateDir)
	if err != nil {
		return nil, fmt.Errorf("resolve state directory: %w", err)
	}
	if err := ensurePrivateDirectory(stateDir); err != nil {
		return nil, err
	}

	return Open(ctx, filepath.Join(stateDir, DatabaseFilename))
}

// Open opens and migrates a database at path. Callers that use a
// persistent path should ensure its parent directory is private with
// [ensurePrivateDirectory] through [OpenState].
//
// A small pool preserves concurrent read availability while avoiding an
// unbounded number of SQLite connections.
// Each connection receives the required pragmas from the DSN above.
func Open(ctx context.Context, path string) (*DB, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	path = strings.TrimSpace(path)
	if path == "" {
		return nil, fmt.Errorf("open database: path is empty")
	}

	if path != ":memory:" && !strings.HasPrefix(path, "file:") {
		if err := ensurePrivateDatabasePath(path); err != nil {
			return nil, err
		}
	}

	dsn := sqliteDSN(path)

	sqlDB, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("open SQLite database: %w", err)
	}

	sqlDB.SetMaxOpenConns(8)
	sqlDB.SetMaxIdleConns(8)

	database := &DB{DB: sqlDB, Path: path}
	if err := sqlDB.PingContext(ctx); err != nil {
		_ = sqlDB.Close()
		return nil, fmt.Errorf("ping SQLite database: %w", err)
	}
	if path != ":memory:" && !strings.HasPrefix(path, "file:") {
		if err := os.Chmod(path, 0o600); err != nil {
			_ = sqlDB.Close()
			return nil, fmt.Errorf("restrict database permissions: %w", err)
		}
	}
	if err := database.Migrate(ctx); err != nil {
		_ = sqlDB.Close()
		return nil, err
	}

	return database, nil
}

// OpenStore opens the private database and returns a repository/session store.
func OpenStore(ctx context.Context, stateDir string) (*RepositoryStore, error) {
	database, err := OpenState(ctx, stateDir)
	if err != nil {
		return nil, err
	}
	store := NewRepositoryStore(database)
	if _, err := store.RecoverExpiredOperations(ctx); err != nil {
		_ = store.Close()
		return nil, fmt.Errorf("recover expired operations: %w", err)
	}
	return store, nil
}

func ensurePrivateDirectory(path string) error {
	info, err := os.Lstat(path)
	switch {
	case err == nil:
		if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
			return fmt.Errorf("%w: %s is not a directory", ErrInvalidStateDirectory, path)
		}
	case errors.Is(err, os.ErrNotExist):
		if err := os.MkdirAll(path, 0o700); err != nil {
			return fmt.Errorf("create private state directory: %w", err)
		}
	default:
		return fmt.Errorf("inspect private state directory: %w", err)
	}

	if err := os.Chmod(path, 0o700); err != nil {
		return fmt.Errorf("restrict state directory permissions: %w", err)
	}
	return nil
}

func ensurePrivateDatabasePath(path string) error {
	path, err := filepath.Abs(path)
	if err != nil {
		return fmt.Errorf("resolve database path: %w", err)
	}

	info, statErr := os.Lstat(path)
	if statErr == nil {
		if info.Mode()&os.ModeSymlink != 0 || info.IsDir() {
			return fmt.Errorf("%w: database path is not a regular file", ErrInvalidStateDirectory)
		}
		return nil
	}
	if !errors.Is(statErr, os.ErrNotExist) {
		return fmt.Errorf("inspect database path: %w", statErr)
	}

	parent := filepath.Dir(path)
	parentInfo, err := os.Stat(parent)
	if err != nil {
		return fmt.Errorf("inspect database directory: %w", err)
	}
	if !parentInfo.IsDir() {
		return fmt.Errorf("%w: database parent is not a directory", ErrInvalidStateDirectory)
	}
	return nil
}

func sqliteDSN(path string) string {
	separator := "?"
	if strings.Contains(path, "?") {
		separator = "&"
	}
	return path + separator +
		"_pragma=busy_timeout(" + fmt.Sprint(busyTimeout) + ")" +
		"&_pragma=foreign_keys(1)" +
		"&_pragma=journal_mode(WAL)" +
		"&_pragma=synchronous(NORMAL)" +
		"&_txlock=immediate"
}
