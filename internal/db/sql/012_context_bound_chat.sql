CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    digest TEXT NOT NULL,
    message_json TEXT NOT NULL,
    producer_run_id TEXT NOT NULL DEFAULT '',
    reply_to TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (round_id) REFERENCES rounds(id) ON DELETE CASCADE
);

CREATE INDEX chat_messages_session_created_at_idx
    ON chat_messages (session_id, created_at, id);

CREATE INDEX chat_messages_round_created_at_idx
    ON chat_messages (round_id, created_at, id);

CREATE TABLE chat_runs (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    user_message_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'complete', 'failed', 'cancelled', 'timed_out', 'budget_exhausted')),
    run_json TEXT NOT NULL,
    binding_json TEXT NOT NULL,
    input_json TEXT NOT NULL,
    response_json TEXT NOT NULL DEFAULT '',
    retained_output TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (round_id) REFERENCES rounds(id) ON DELETE CASCADE
);

CREATE INDEX chat_runs_session_created_at_idx
    ON chat_runs (session_id, created_at, id);

CREATE INDEX chat_runs_round_created_at_idx
    ON chat_runs (round_id, created_at, id);

CREATE TABLE round_diff_anchors (
    round_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    side TEXT NOT NULL,
    path TEXT NOT NULL,
    hunk_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    anchor_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (round_id, side, path, hunk_id),
    FOREIGN KEY (round_id) REFERENCES rounds(id) ON DELETE CASCADE
);

CREATE INDEX round_diff_anchors_snapshot_idx
    ON round_diff_anchors (snapshot_id, side, path, hunk_id);
