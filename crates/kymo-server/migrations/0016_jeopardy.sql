-- Jeopardy challenges. Solves never store points: they are recomputed from solve counts.
CREATE TABLE category (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    UNIQUE (event_id, name)
);

CREATE TABLE challenge (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id          INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    category_id       INTEGER NOT NULL REFERENCES category(id),
    title             TEXT NOT NULL,
    description       TEXT NOT NULL DEFAULT '',
    scoring           TEXT NOT NULL,
    flag              TEXT NOT NULL,
    flag_prefix       TEXT NOT NULL DEFAULT '',
    flag_suffix       TEXT NOT NULL DEFAULT '',
    case_sensitive    INTEGER NOT NULL DEFAULT 1,
    max_attempts      INTEGER,
    state             TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'published', 'retired')),
    release_at        INTEGER,
    retire_at         INTEGER,
    position          INTEGER NOT NULL DEFAULT 0,
    requires_instance INTEGER NOT NULL DEFAULT 0,
    instance_config   TEXT,
    created_at        INTEGER NOT NULL
);
CREATE INDEX challenge_by_event ON challenge (event_id, position, id);

CREATE TABLE hint (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    challenge_id INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    content      TEXT NOT NULL,
    cost         INTEGER NOT NULL DEFAULT 0,
    position     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX hint_by_challenge ON hint (challenge_id, position, id);

CREATE TABLE attachment (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    challenge_id INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    mime         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    sha256       TEXT NOT NULL,
    data         BLOB NOT NULL
);
CREATE INDEX attachment_by_challenge ON attachment (challenge_id, id);

-- participant_key is "user:<id>" or "team:<id>", the scoring participant of the event.
CREATE TABLE hint_unlock (
    participant_key TEXT NOT NULL,
    hint_id         INTEGER NOT NULL REFERENCES hint(id) ON DELETE CASCADE,
    at              INTEGER NOT NULL,
    PRIMARY KEY (participant_key, hint_id)
);

-- Append only history of every judged submission (rate limited ones are not recorded).
-- `value` is the submitted text cut to 200 characters.
CREATE TABLE submission (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id         INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id     INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    participant_kind TEXT NOT NULL CHECK (participant_kind IN ('user', 'team')),
    participant_id   INTEGER NOT NULL,
    user_id          INTEGER NOT NULL,
    ip               TEXT NOT NULL,
    value            TEXT NOT NULL,
    verdict          TEXT NOT NULL CHECK (verdict IN ('correct', 'incorrect', 'bad_format')),
    at               INTEGER NOT NULL
);
CREATE INDEX submission_by_event ON submission (event_id, id);
CREATE INDEX submission_by_participant ON submission (challenge_id, participant_kind, participant_id);

CREATE TABLE solve (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id       INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id   INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    participant_id INTEGER NOT NULL,
    user_id        INTEGER NOT NULL,
    at             INTEGER NOT NULL,
    hints_cost     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (event_id, challenge_id, participant_id)
);

-- Scoring and submission settings of one Jeopardy event. A missing row means the defaults.
CREATE TABLE event_jeopardy (
    event_id          INTEGER PRIMARY KEY REFERENCES event(id) ON DELETE CASCADE,
    tie_breaks        TEXT NOT NULL DEFAULT '["earliest_last_solve"]',
    rate_max_attempts INTEGER NOT NULL DEFAULT 10,
    rate_window_secs  INTEGER NOT NULL DEFAULT 60,
    rate_lockout_secs INTEGER NOT NULL DEFAULT 60
);
