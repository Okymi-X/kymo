-- Dynamic flags for a challenge that starts nothing. The value is made for a participant the
-- first time one of the challenge's template files is given to them, and kept, so a second
-- download shows the same flag and a submission can be checked against it.
CREATE TABLE participant_flag (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    challenge_id INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    flag_id      INTEGER NOT NULL REFERENCES challenge_flag(id) ON DELETE CASCADE,
    scorer_kind  TEXT NOT NULL,
    scorer_id    INTEGER NOT NULL,
    value        TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    UNIQUE (flag_id, scorer_kind, scorer_id)
);
CREATE INDEX participant_flag_by_value ON participant_flag (challenge_id, value);

-- A text file whose `{{flag}}` tokens are replaced with the downloader's own flag.
ALTER TABLE attachment ADD COLUMN template INTEGER NOT NULL DEFAULT 0;
