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
