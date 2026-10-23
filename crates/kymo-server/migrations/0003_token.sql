CREATE TABLE token (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    hash       TEXT NOT NULL UNIQUE,
    kind       TEXT NOT NULL CHECK (kind IN ('session', 'api')),
    label      TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
