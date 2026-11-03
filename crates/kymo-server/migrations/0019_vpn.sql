-- VPN mode: instances of an event live on private lab networks that players reach through
-- their own OpenVPN tunnel, instead of through a port published on the host.
ALTER TABLE event ADD COLUMN network_mode TEXT NOT NULL DEFAULT 'published';

-- One tunnel identity per player account. `idx` picks the tunnel address and is never reused.
CREATE TABLE vpn_client (
    user_id    INTEGER PRIMARY KEY REFERENCES user(id) ON DELETE CASCADE,
    idx        INTEGER NOT NULL UNIQUE,
    serial     TEXT NOT NULL,
    issued_at  INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

-- Certificates that must no longer be accepted (replaced by a newer profile).
CREATE TABLE vpn_revoked (
    serial TEXT PRIMARY KEY,
    at     INTEGER NOT NULL
);

-- One lab network per scoring participant (a player, or a team) in an event.
CREATE TABLE vpn_lab (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    idx          INTEGER NOT NULL UNIQUE,
    event_id     INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    scorer_kind  TEXT NOT NULL,
    scorer_id    INTEGER NOT NULL,
    network      TEXT NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    UNIQUE (event_id, scorer_kind, scorer_id)
);

-- The address an instance has inside its lab.
ALTER TABLE instance ADD COLUMN lab_ip TEXT;

-- A profile is a fresh certificate; ten an hour per player is plenty.
INSERT INTO rate_rule (endpoint, max_requests, window_secs, scope, strikes_to_ban, ban_secs, created_at)
VALUES ('POST /api/v1/vpn/profile', 10, 3600, 'user', NULL, 0, 0);
