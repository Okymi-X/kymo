-- Dynamic flags, several flags per challenge, and Responder challenges
-- (docs/DYNAMIC_FLAGS_AND_RESPONDER.md). Everything is additive: a challenge that never uses
-- these columns behaves exactly as before.
ALTER TABLE challenge ADD COLUMN flag_mode TEXT NOT NULL DEFAULT 'static' CHECK (flag_mode IN ('static', 'dynamic'));
ALTER TABLE challenge ADD COLUMN multi_flag INTEGER NOT NULL DEFAULT 0;
ALTER TABLE challenge ADD COLUMN responder INTEGER NOT NULL DEFAULT 0;

-- The flags of a dynamic or multi flag challenge. For a static flag `value` is the whole flag;
-- for a dynamic one it is the prefix, and a random part of `random_len` hex characters plus
-- `closing` is added for every run.
CREATE TABLE challenge_flag (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    challenge_id INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0,
    name         TEXT NOT NULL,
    label        TEXT NOT NULL DEFAULT '',
    value        TEXT NOT NULL,
    random_len   INTEGER NOT NULL DEFAULT 8,
    closing      TEXT NOT NULL DEFAULT '',
    weight       INTEGER NOT NULL DEFAULT 1,
    UNIQUE (challenge_id, name)
);
CREATE INDEX challenge_flag_by_challenge ON challenge_flag (challenge_id, position, id);

-- The generated value of one flag for one run. Kept after the run ends: a player may submit
-- after the instance expired, and a flag from an earlier run stays valid.
CREATE TABLE instance_flag (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    instance_id INTEGER NOT NULL REFERENCES instance(id) ON DELETE CASCADE,
    flag_id     INTEGER NOT NULL REFERENCES challenge_flag(id) ON DELETE CASCADE,
    value       TEXT NOT NULL,
    UNIQUE (instance_id, flag_id)
);
CREATE INDEX instance_flag_by_value ON instance_flag (value);

-- One row per flag a participant found on a multi flag challenge, like item_solve.
CREATE TABLE flag_solve (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id       INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id   INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    flag_id        INTEGER NOT NULL REFERENCES challenge_flag(id) ON DELETE CASCADE,
    participant_id INTEGER NOT NULL,
    user_id        INTEGER NOT NULL,
    instance_id    INTEGER,
    points         INTEGER NOT NULL,
    at             INTEGER NOT NULL,
    UNIQUE (flag_id, participant_id)
);
CREATE INDEX flag_solve_by_event ON flag_solve (event_id, participant_id);

ALTER TABLE submission ADD COLUMN flag_id INTEGER REFERENCES challenge_flag(id) ON DELETE SET NULL;

-- What the lab host returned for a run (machine, Verifier), as JSON, and the lab address Kymo
-- allocated for the run's Verifier.
ALTER TABLE instance ADD COLUMN endpoints TEXT;
ALTER TABLE instance ADD COLUMN verifier_ip TEXT;
