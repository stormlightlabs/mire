-- Rebuild the snapshot graph so the snapshot kind constraint can accept the
-- new three-dot comparison without losing existing private review state.
ALTER TABLE activity RENAME TO activity_v3;
ALTER TABLE operations RENAME TO operations_v3;
ALTER TABLE rounds RENAME TO rounds_v3;
ALTER TABLE snapshot_entries RENAME TO snapshot_entries_v3;
ALTER TABLE snapshot_changes RENAME TO snapshot_changes_v3;
ALTER TABLE snapshots RENAME TO snapshots_v3;

DROP INDEX activity_session_id_idx;
DROP INDEX operations_active_session_idx;
DROP INDEX operations_session_created_at_idx;
DROP INDEX rounds_session_number_idx;
DROP INDEX rounds_snapshot_id_idx;
DROP INDEX snapshot_changes_status_idx;
DROP INDEX snapshot_entries_path_idx;
DROP INDEX snapshots_repository_created_at_idx;

CREATE TABLE snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    repository_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('two_dot', 'three_dot')),
    requested_comparison TEXT NOT NULL,
    base_oid TEXT NOT NULL,
    effective_base_oid TEXT NOT NULL,
    target_oid TEXT NOT NULL,
    merge_base_oid TEXT NOT NULL,
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

CREATE TABLE rounds (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    snapshot_id TEXT,
    number INTEGER NOT NULL CHECK (number > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'incomplete', 'cancelled')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (session_id, number),
    UNIQUE (id, session_id, repository_id),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(id)
);

CREATE INDEX rounds_session_number_idx
    ON rounds (session_id, number);

CREATE INDEX rounds_snapshot_id_idx
    ON rounds (snapshot_id);

CREATE TABLE operations (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    round_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'complete', 'failed', 'cancelled', 'abandoned')),
    owner_id TEXT,
    heartbeat_at TEXT,
    lease_expires_at TEXT,
    failure TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (round_id, session_id, repository_id)
        REFERENCES rounds(id, session_id, repository_id)
);

CREATE UNIQUE INDEX operations_active_session_idx
    ON operations (session_id)
    WHERE status IN ('queued', 'running');

CREATE INDEX operations_session_created_at_idx
    ON operations (session_id, created_at, id);

CREATE TABLE activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    round_id TEXT,
    operation_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    message TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (round_id, session_id, repository_id)
        REFERENCES rounds(id, session_id, repository_id),
    FOREIGN KEY (operation_id) REFERENCES operations(id) ON DELETE CASCADE
);

CREATE INDEX activity_session_id_idx
    ON activity (session_id, id);

INSERT INTO snapshots (
    id, repository_id, kind, requested_comparison, base_oid, effective_base_oid,
    target_oid, merge_base_oid, object_format, context_policy_hash,
    base_manifest_digest, target_manifest_digest, manifest_digest, complete, created_at
)
SELECT id, repository_id, kind, requested_comparison, effective_base_oid,
       effective_base_oid, target_oid, '', object_format, context_policy_hash,
       base_manifest_digest, target_manifest_digest, manifest_digest, complete, created_at
FROM snapshots_v3;

INSERT INTO snapshot_entries (
    snapshot_id, tree_side, path, kind, mode, size, content_digest, git_oid, symlink_target
)
SELECT snapshot_id, tree_side, path, kind, mode, size, content_digest, git_oid, symlink_target
FROM snapshot_entries_v3;

INSERT INTO snapshot_changes (
    snapshot_id, status, base_path, target_path, base_digest, target_digest
)
SELECT snapshot_id, status, base_path, target_path, base_digest, target_digest
FROM snapshot_changes_v3;

INSERT INTO rounds (id, session_id, repository_id, snapshot_id, number, status, created_at, updated_at)
SELECT id, session_id, repository_id, snapshot_id, number, status, created_at, updated_at
FROM rounds_v3;

INSERT INTO operations (
    id, session_id, repository_id, round_id, kind, status, owner_id, heartbeat_at,
    lease_expires_at, failure, created_at, updated_at, started_at, finished_at
)
SELECT id, session_id, repository_id, round_id, kind, status, owner_id, heartbeat_at,
       lease_expires_at, failure, created_at, updated_at, started_at, finished_at
FROM operations_v3;

INSERT INTO activity (
    id, session_id, repository_id, round_id, operation_id, kind, status, message, created_at
)
SELECT id, session_id, repository_id, round_id, operation_id, kind, status, message, created_at
FROM activity_v3;

DROP TABLE activity_v3;
DROP TABLE operations_v3;
DROP TABLE rounds_v3;
DROP TABLE snapshot_changes_v3;
DROP TABLE snapshot_entries_v3;
DROP TABLE snapshots_v3;
