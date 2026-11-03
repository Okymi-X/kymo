-- Lab hosts: other machines that run challenge containers for this platform, reached over an
-- authenticated API. An event picks one (or none, meaning this machine's own runtime).
CREATE TABLE lab_host (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL UNIQUE,
    api_url         TEXT NOT NULL,
    -- The secret the lab host expects. Never shown again once saved.
    token           TEXT NOT NULL,
    -- SHA-256 of the lab host's certificate when it is pinned (a self-signed one).
    fingerprint     TEXT,
    -- What players are told to connect to: the lab host's public name or address.
    public_host     TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    last_checked_at INTEGER,
    last_ok_at      INTEGER,
    last_error      TEXT
);

ALTER TABLE event ADD COLUMN lab_host_id INTEGER REFERENCES lab_host(id) ON DELETE SET NULL;

-- Where each run was started, so it is stopped and checked on the same machine even if the event
-- is moved later, and the name the platform built for it before asking.
ALTER TABLE instance ADD COLUMN lab_host_id INTEGER;
ALTER TABLE instance ADD COLUMN name TEXT;
ALTER TABLE instance ADD COLUMN public_host TEXT;
