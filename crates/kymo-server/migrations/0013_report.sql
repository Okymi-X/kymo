ALTER TABLE team ADD COLUMN code TEXT;
CREATE UNIQUE INDEX team_code ON team (code) WHERE code IS NOT NULL;

CREATE TABLE report (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    match_id      INTEGER NOT NULL,
    team_id       INTEGER NOT NULL,
    score_a       INTEGER NOT NULL,
    score_b       INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    UNIQUE (tournament_id, match_id, team_id)
);
