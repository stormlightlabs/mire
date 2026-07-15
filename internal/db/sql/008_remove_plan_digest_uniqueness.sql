-- A content digest may legitimately repeat when identical frozen input is
-- planned again in a later run. Keep run identity as the plan key.
ALTER TABLE review_plans RENAME TO review_plans_v8;
DROP INDEX review_plans_session_created_at_idx;

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

INSERT INTO review_plans (
    run_id, session_id, round_id, snapshot_id, change_model_digest,
    plan_digest, plan_json, created_at
)
SELECT run_id, session_id, round_id, snapshot_id, change_model_digest,
       plan_digest, plan_json, created_at
FROM review_plans_v8;

DROP TABLE review_plans_v8;
