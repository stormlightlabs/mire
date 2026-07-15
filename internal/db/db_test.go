package db

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestOpenStateCreatesPrivateMigratedDatabase(t *testing.T) {
	t.Parallel()

	stateDir := filepath.Join(t.TempDir(), "state")
	database, err := OpenState(context.Background(), stateDir)
	if err != nil {
		t.Fatalf("OpenState() error = %v", err)
	}
	t.Cleanup(func() {
		if err := database.Close(); err != nil {
			t.Errorf("Close() error = %v", err)
		}
	})

	stateInfo, err := os.Stat(stateDir)
	if err != nil {
		t.Fatalf("stat state directory: %v", err)
	}
	if permissions := stateInfo.Mode().Perm(); permissions != 0o700 {
		t.Fatalf("state directory permissions = %o, want 700", permissions)
	}

	databaseInfo, err := os.Stat(filepath.Join(stateDir, DatabaseFilename))
	if err != nil {
		t.Fatalf("stat database: %v", err)
	}
	if permissions := databaseInfo.Mode().Perm(); permissions != 0o600 {
		t.Fatalf("database permissions = %o, want 600", permissions)
	}

	var foreignKeys, busyTimeout, journalMode string
	if err := database.QueryRow("PRAGMA foreign_keys").Scan(&foreignKeys); err != nil {
		t.Fatalf("query foreign_keys: %v", err)
	}
	if foreignKeys != "1" {
		t.Fatalf("foreign_keys = %q, want 1", foreignKeys)
	}
	if err := database.QueryRow("PRAGMA busy_timeout").Scan(&busyTimeout); err != nil {
		t.Fatalf("query busy_timeout: %v", err)
	}
	if busyTimeout != "5000" {
		t.Fatalf("busy_timeout = %q, want 5000", busyTimeout)
	}
	if err := database.QueryRow("PRAGMA journal_mode").Scan(&journalMode); err != nil {
		t.Fatalf("query journal_mode: %v", err)
	}
	if !strings.EqualFold(journalMode, "wal") {
		t.Fatalf("journal_mode = %q, want wal", journalMode)
	}

	var migrationVersion int
	if err := database.QueryRow("SELECT MAX(version) FROM schema_migrations").Scan(&migrationVersion); err != nil {
		t.Fatalf("query migration version: %v", err)
	}
	if migrationVersion != LatestMigrationVersion() {
		t.Fatalf("migration version = %d, want %d", migrationVersion, LatestMigrationVersion())
	}
}

func TestOpenStateRejectsSymlinkedStateDirectory(t *testing.T) {
	t.Parallel()

	parent := t.TempDir()
	target := filepath.Join(parent, "target")
	if err := os.Mkdir(target, 0o700); err != nil {
		t.Fatalf("mkdir target: %v", err)
	}
	link := filepath.Join(parent, "state")
	if err := os.Symlink(target, link); err != nil {
		t.Skipf("symlinks unavailable: %v", err)
	}

	_, err := OpenState(context.Background(), link)
	if !errors.Is(err, ErrInvalidStateDirectory) {
		t.Fatalf("OpenState() error = %v, want ErrInvalidStateDirectory", err)
	}
}
