CREATE TABLE verification_runs (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    candidate_id TEXT NOT NULL,
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
    retained_output TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (candidate_id) REFERENCES review_candidates(id) ON DELETE CASCADE
);

CREATE INDEX verification_runs_round_created_at_idx
    ON verification_runs (round_id, created_at, id);

CREATE INDEX verification_runs_candidate_created_at_idx
    ON verification_runs (candidate_id, created_at, id);

CREATE TABLE verifications (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL,
    candidate_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('not_run', 'supported', 'inconclusive', 'refuted', 'blocked')),
    digest TEXT NOT NULL,
    verification_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (candidate_id) REFERENCES review_candidates(id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES verification_runs(id) ON DELETE CASCADE
);

CREATE INDEX verifications_round_created_at_idx
    ON verifications (round_id, created_at, id);

CREATE INDEX verifications_candidate_created_at_idx
    ON verifications (candidate_id, created_at, id);
