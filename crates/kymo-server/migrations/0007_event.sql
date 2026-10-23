CREATE TABLE event (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    at            INTEGER NOT NULL,
    kind          TEXT NOT NULL,
    match_id      INTEGER,
    data          TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX event_by_tournament ON event (tournament_id, id);
