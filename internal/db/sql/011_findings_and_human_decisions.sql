CREATE TABLE finding_revisions (
    finding_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    finding_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (finding_id, revision),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX finding_revisions_round_created_at_idx
    ON finding_revisions (round_id, created_at, finding_id, revision);

CREATE INDEX finding_revisions_finding_revision_idx
    ON finding_revisions (finding_id, revision, created_at);

CREATE TABLE finding_dispositions (
    id TEXT PRIMARY KEY NOT NULL,
    finding_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    session_id TEXT NOT NULL,
    round_id TEXT NOT NULL,
    disposition TEXT NOT NULL CHECK (disposition IN ('open', 'accepted', 'intentional', 'dismissed', 'deferred', 'resolved', 'accepted_risk')),
    rationale TEXT NOT NULL DEFAULT '',
    digest TEXT NOT NULL,
    disposition_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (finding_id, revision) REFERENCES finding_revisions(finding_id, revision) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX finding_dispositions_finding_created_at_idx
    ON finding_dispositions (finding_id, created_at, id);

CREATE TABLE finding_presentations (
    id TEXT PRIMARY KEY NOT NULL,
    finding_id TEXT NOT NULL,
    finding_revision INTEGER NOT NULL CHECK (finding_revision > 0),
    version INTEGER NOT NULL CHECK (version > 0),
    digest TEXT NOT NULL,
    presentation_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (finding_id, version),
    FOREIGN KEY (finding_id, finding_revision) REFERENCES finding_revisions(finding_id, revision) ON DELETE CASCADE
);

CREATE INDEX finding_presentations_finding_created_at_idx
    ON finding_presentations (finding_id, version, created_at, id);
