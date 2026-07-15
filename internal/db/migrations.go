package db

import (
	"context"
	"fmt"
	"time"
)

// Migration is one forward-only schema change.
type Migration struct {
	Version int
	Name    string
	SQL     string
}

var migrations = []Migration{
	{
		Version: 1,
		Name:    "repositories and sessions",
		SQL: `
CREATE TABLE repositories (
    id TEXT PRIMARY KEY NOT NULL,
    canonical_identity TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    discovered_git_dir TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL,
    current_round_id TEXT,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX sessions_repository_created_at_idx
    ON sessions (repository_id, created_at, id);
`,
	},
}

// LatestMigrationVersion returns the highest schema version known to this
// binary.
func LatestMigrationVersion() int {
	if len(migrations) == 0 {
		return 0
	}
	return migrations[len(migrations)-1].Version
}

// Migrate applies all pending migrations in one transaction. A migration is
// recorded only after its SQL has completed successfully.
func Migrate(ctx context.Context, database *DB) error {
	if database == nil || database.DB == nil {
		return fmt.Errorf("migrate database: database is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if err := validateMigrations(); err != nil {
		return err
	}

	tx, err := database.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin database migration: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	if _, err := tx.ExecContext(ctx, `
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    applied_at TEXT NOT NULL
)`); err != nil {
		return fmt.Errorf("create migration table: %w", err)
	}

	var current int
	if err := tx.QueryRowContext(ctx, `SELECT COALESCE(MAX(version), 0) FROM schema_migrations`).Scan(&current); err != nil {
		return fmt.Errorf("read migration version: %w", err)
	}
	if current > LatestMigrationVersion() {
		return fmt.Errorf("database schema version %d is newer than this binary supports (latest %d)", current, LatestMigrationVersion())
	}

	for _, migration := range migrations {
		if migration.Version <= current {
			continue
		}
		if _, err := tx.ExecContext(ctx, migration.SQL); err != nil {
			return fmt.Errorf("apply migration %d (%s): %w", migration.Version, migration.Name, err)
		}
		if _, err := tx.ExecContext(ctx,
			`INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)`,
			migration.Version,
			timestampString(time.Now()),
		); err != nil {
			return fmt.Errorf("record migration %d (%s): %w", migration.Version, migration.Name, err)
		}
		current = migration.Version
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("commit database migrations: %w", err)
	}
	return nil
}

// Migrate applies pending migrations to database.
func (database *DB) Migrate(ctx context.Context) error {
	return Migrate(ctx, database)
}

func validateMigrations() error {
	previous := 0
	for _, migration := range migrations {
		if migration.Version <= previous {
			return fmt.Errorf("invalid migration sequence at version %d", migration.Version)
		}
		if migration.SQL == "" {
			return fmt.Errorf("migration %d (%s) has no SQL", migration.Version, migration.Name)
		}
		previous = migration.Version
	}
	return nil
}
