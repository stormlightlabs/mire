CREATE TABLE review_runs (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL CHECK (role = 'reviewer'),
    pass_name TEXT NOT NULL,
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

CREATE INDEX review_runs_round_created_at_idx
    ON review_runs (round_id, created_at, id);

CREATE TABLE review_passes (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    run_id TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('completed', 'failed', 'skipped', 'truncated', 'unsupported')),
    applicable INTEGER NOT NULL CHECK (applicable IN (0, 1)),
    reason TEXT NOT NULL DEFAULT '',
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 0),
    pass_json TEXT NOT NULL,
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES review_runs(id) ON DELETE SET NULL
);

CREATE INDEX review_passes_round_name_idx
    ON review_passes (round_id, name);

CREATE TABLE review_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    pass_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    pass_name TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    fingerprint TEXT NOT NULL,
    candidate_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (pass_id) REFERENCES review_passes(id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES review_runs(id) ON DELETE CASCADE
);

CREATE INDEX review_candidates_round_created_at_idx
    ON review_candidates (round_id, created_at, id);

CREATE INDEX review_candidates_round_pass_idx
    ON review_candidates (round_id, pass_name, ordinal, id);

CREATE TABLE review_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL DEFAULT '',
    snapshot_id TEXT NOT NULL DEFAULT '',
    pass_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    pass_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    path TEXT NOT NULL DEFAULT '',
    relation TEXT NOT NULL,
    hunk_ids_json TEXT NOT NULL DEFAULT '[]',
    digest TEXT NOT NULL,
    size INTEGER NOT NULL CHECK (size >= 0),
    content TEXT NOT NULL DEFAULT '',
    excluded INTEGER NOT NULL CHECK (excluded IN (0, 1)),
    exclusion_reason TEXT NOT NULL DEFAULT '',
    truncated INTEGER NOT NULL CHECK (truncated IN (0, 1)),
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (pass_id) REFERENCES review_passes(id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES review_runs(id) ON DELETE CASCADE
);

CREATE INDEX review_artifacts_round_pass_idx
    ON review_artifacts (round_id, pass_name, created_at, id);

CREATE TABLE review_coverage (
    round_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL DEFAULT '',
    coverage_digest TEXT NOT NULL,
    coverage_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
