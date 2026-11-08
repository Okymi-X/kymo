-- Challenge-based tournament matches (see ARCHITECTURE, Challenge-based tournament matches).
ALTER TABLE tournament ADD COLUMN challenge_assignment TEXT NOT NULL DEFAULT 'none'
    CHECK (challenge_assignment IN ('none', 'manual', 'per_match', 'per_round'));
ALTER TABLE tournament ADD COLUMN challenges_solo INTEGER NOT NULL DEFAULT 1;
ALTER TABLE tournament ADD COLUMN challenges_team INTEGER NOT NULL DEFAULT 3;
ALTER TABLE tournament ADD COLUMN challenge_seed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tournament ADD COLUMN challenge_time_limit INTEGER;

-- Which account or player team plays for a bracket team.
ALTER TABLE team ADD COLUMN user_id INTEGER REFERENCES user(id) ON DELETE SET NULL;
ALTER TABLE team ADD COLUMN player_team_id INTEGER REFERENCES player_team(id) ON DELETE SET NULL;

ALTER TABLE challenge ADD COLUMN kind TEXT NOT NULL DEFAULT 'flag' CHECK (kind IN ('flag', 'dfir'));

-- The questions of a DFIR challenge. `answer` is a kymo_jeopardy FlagSpec as JSON.
CREATE TABLE dfir_item (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    challenge_id   INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    position       INTEGER NOT NULL DEFAULT 0,
    question       TEXT NOT NULL,
    answer         TEXT NOT NULL,
    case_sensitive INTEGER NOT NULL DEFAULT 0,
    points         INTEGER NOT NULL,
    max_attempts   INTEGER
);
CREATE INDEX dfir_item_by_challenge ON dfir_item (challenge_id, position, id);

CREATE TABLE duel_assignment (
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    match_id      INTEGER NOT NULL,
    challenge_id  INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tournament_id, match_id, challenge_id)
);

-- One row per match that has been opened or finished.
CREATE TABLE duel_match (
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    match_id      INTEGER NOT NULL,
    opened_at     INTEGER,
    finalized_at  INTEGER,
    PRIMARY KEY (tournament_id, match_id)
);

-- Append only. A correct answer is unique per side, challenge and item.
CREATE TABLE duel_answer (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    match_id      INTEGER NOT NULL,
    challenge_id  INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    item_id       INTEGER REFERENCES dfir_item(id) ON DELETE CASCADE,
    side_team_id  INTEGER NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    user_id       INTEGER NOT NULL,
    verdict       TEXT NOT NULL CHECK (verdict IN ('correct', 'incorrect')),
    points        INTEGER NOT NULL DEFAULT 0,
    value         TEXT NOT NULL,
    ip            TEXT NOT NULL,
    at            INTEGER NOT NULL
);
CREATE INDEX duel_answer_by_match ON duel_answer (tournament_id, match_id, side_team_id);
CREATE UNIQUE INDEX duel_answer_once ON duel_answer (tournament_id, match_id, side_team_id, challenge_id, COALESCE(item_id, 0))
    WHERE verdict = 'correct';
