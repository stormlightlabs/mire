-- Preserve the immutable round chain when a session receives another capture.
ALTER TABLE rounds
    ADD COLUMN predecessor_round_id TEXT REFERENCES rounds(id);

CREATE INDEX rounds_predecessor_round_id_idx
    ON rounds (predecessor_round_id);
