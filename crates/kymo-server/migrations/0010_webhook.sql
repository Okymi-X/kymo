CREATE TABLE webhook (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    label      TEXT NOT NULL,
    url        TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
