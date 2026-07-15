CREATE TABLE snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind = 'two_dot'),
    requested_comparison TEXT NOT NULL,
    effective_base_oid TEXT NOT NULL,
    target_oid TEXT NOT NULL,
    object_format TEXT NOT NULL CHECK (object_format IN ('sha1', 'sha256')),
    context_policy_hash TEXT NOT NULL,
    base_manifest_digest TEXT NOT NULL,
    target_manifest_digest TEXT NOT NULL,
    manifest_digest TEXT NOT NULL UNIQUE,
    complete INTEGER NOT NULL CHECK (complete = 1),
    created_at TEXT NOT NULL,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX snapshots_repository_created_at_idx
    ON snapshots (repository_id, created_at, id);

CREATE TABLE snapshot_entries (
    snapshot_id TEXT NOT NULL,
    tree_side TEXT NOT NULL CHECK (tree_side IN ('base', 'target')),
    path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('file', 'symlink', 'submodule')),
    mode INTEGER NOT NULL,
    size INTEGER NOT NULL CHECK (size >= 0),
    content_digest TEXT NOT NULL DEFAULT '',
    git_oid TEXT NOT NULL,
    symlink_target TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (snapshot_id, tree_side, path),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
);

CREATE INDEX snapshot_entries_path_idx
    ON snapshot_entries (snapshot_id, tree_side, path);

CREATE TABLE snapshot_changes (
    snapshot_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('added', 'deleted', 'modified', 'renamed', 'unchanged')),
    base_path TEXT NOT NULL DEFAULT '',
    target_path TEXT NOT NULL DEFAULT '',
    base_digest TEXT NOT NULL DEFAULT '',
    target_digest TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (snapshot_id, base_path, target_path),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
);

CREATE INDEX snapshot_changes_status_idx
    ON snapshot_changes (snapshot_id, status, base_path, target_path);

ALTER TABLE rounds
    ADD COLUMN snapshot_id TEXT REFERENCES snapshots(id);

CREATE INDEX rounds_snapshot_id_idx
    ON rounds (snapshot_id);
