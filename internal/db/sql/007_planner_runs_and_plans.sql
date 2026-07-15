CREATE TABLE planner_runs (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL CHECK (role = 'planner'),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'complete', 'failed', 'cancelled', 'timed_out', 'budget_exhausted')),
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    error TEXT NOT NULL DEFAULT '',
    adapter TEXT NOT NULL,
    protocol TEXT NOT NULL,
    prompt_template_version TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT '',
    parameters_json TEXT NOT NULL DEFAULT '{}',
    input_manifest_digest TEXT NOT NULL,
    input_digest TEXT NOT NULL,
    output_digest TEXT NOT NULL DEFAULT '',
    usage_json TEXT NOT NULL DEFAULT '{}',
    finish_reason TEXT NOT NULL DEFAULT '',
    redactions_json TEXT NOT NULL DEFAULT '[]',
    termination_cause TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX planner_runs_session_created_at_idx
    ON planner_runs (session_id, created_at, id);

CREATE TABLE review_plans (
    run_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    change_model_digest TEXT NOT NULL,
    plan_digest TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES planner_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX review_plans_session_created_at_idx
    ON review_plans (session_id, created_at, run_id);
