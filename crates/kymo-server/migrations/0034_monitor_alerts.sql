-- What the request log keeps for attack detection: the query string and user agent (both cut
-- short; never a body, password or token), and whether the caller was staff.
ALTER TABLE request_log ADD COLUMN query TEXT NOT NULL DEFAULT '';
ALTER TABLE request_log ADD COLUMN ua TEXT NOT NULL DEFAULT '';
ALTER TABLE request_log ADD COLUMN staff INTEGER NOT NULL DEFAULT 0;

-- A detector the organizer changed. A detector with no row here uses the built in settings, so
-- one added in a later version follows its default until the organizer decides otherwise.
CREATE TABLE detector (
    kind        TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 1,
    alert_at    INTEGER NOT NULL,
    ban_at      INTEGER,
    window_secs INTEGER NOT NULL,
    ban_secs    INTEGER,
    updated_at  INTEGER NOT NULL DEFAULT 0
);

-- Something a detector found. One open alert per detector and subject: it grows while the
-- behaviour goes on. `user_id` is empty for a visitor, who is known by address.
CREATE TABLE alert (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,
    user_id    INTEGER,
    ip         TEXT NOT NULL,
    count      INTEGER NOT NULL,
    first_at   INTEGER NOT NULL,
    last_at    INTEGER NOT NULL,
    evidence   TEXT NOT NULL DEFAULT '[]',
    status     TEXT NOT NULL DEFAULT 'open',
    note       TEXT NOT NULL DEFAULT '',
    ban_id     INTEGER,
    handled_by INTEGER,
    handled_at INTEGER
);
CREATE INDEX alert_status ON alert (status, last_at);
CREATE INDEX alert_subject ON alert (kind, user_id, ip);
