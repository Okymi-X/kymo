CREATE TABLE tournament (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    slug       TEXT NOT NULL UNIQUE,
    format     TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'draft',
    bracket    TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE team (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournament(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    seed          INTEGER NOT NULL,
    UNIQUE (tournament_id, name),
    UNIQUE (tournament_id, seed)
);
