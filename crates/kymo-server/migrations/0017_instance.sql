-- One row per run of a challenge container. A restart ends one row and starts another, so the
-- number of non failed rows is the participant's start budget for that challenge.
CREATE TABLE instance (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id      INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id  INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    scorer_kind   TEXT NOT NULL,
    scorer_id     INTEGER NOT NULL,
    user_id       INTEGER NOT NULL,
    handle        TEXT,
    host_port     INTEGER,
    state         TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    expires_at    INTEGER NOT NULL,
    stopped_at    INTEGER
);
CREATE INDEX instance_owner ON instance (challenge_id, scorer_kind, scorer_id);
CREATE INDEX instance_live ON instance (state, expires_at);
