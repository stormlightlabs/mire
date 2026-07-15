CREATE TABLE rounds (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    repository_id TEXT NOT NULL,
    number INTEGER NOT NULL CHECK (number > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'incomplete', 'cancelled')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (session_id, number),
    UNIQUE (id, session_id, repository_id),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX rounds_session_number_idx
    ON rounds (session_id, number);

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
