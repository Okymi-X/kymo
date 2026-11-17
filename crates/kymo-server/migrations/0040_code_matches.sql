-- Two accepted coding solutions that look copied from each other. The sources stay in
-- `code_submission`; a match only points at them. An organizer dismisses a false alarm.
CREATE TABLE code_match (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id            INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id        INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    submission_id       INTEGER NOT NULL REFERENCES code_submission(id) ON DELETE CASCADE,
    other_submission_id INTEGER NOT NULL REFERENCES code_submission(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    other_name          TEXT NOT NULL,
    score               INTEGER NOT NULL,
    status              TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'dismissed')),
    at                  INTEGER NOT NULL,
    UNIQUE (submission_id, other_submission_id)
);
CREATE INDEX code_match_by_event ON code_match (event_id, status);
