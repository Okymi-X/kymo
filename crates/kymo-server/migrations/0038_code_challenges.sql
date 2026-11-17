-- Coding challenges: a player submits source code and a lab host runs it against the tests.
-- The config (languages, tests, limits) is JSON, owned by the author; hidden tests never leave
-- the server.
CREATE TABLE challenge_code (
    challenge_id INTEGER PRIMARY KEY REFERENCES challenge(id) ON DELETE CASCADE,
    config       TEXT NOT NULL
);

-- Every judged submission, with its source, so an organizer can read what was sent.
CREATE TABLE code_submission (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id      INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id  INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    scorer_kind   TEXT NOT NULL,
    scorer_id     INTEGER NOT NULL,
    user_id       INTEGER NOT NULL,
    language      TEXT NOT NULL,
    source        TEXT NOT NULL,
    passed        INTEGER NOT NULL,
    total         INTEGER NOT NULL,
    compile_error TEXT,
    at            INTEGER NOT NULL
);
CREATE INDEX code_submission_by_challenge ON code_submission (challenge_id, at);
