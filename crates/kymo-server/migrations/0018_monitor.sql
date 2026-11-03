-- Request log, per endpoint rate rules, bans and the moderation audit trail.
CREATE TABLE request_log (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    at      INTEGER NOT NULL,
    method  TEXT NOT NULL,
    route   TEXT NOT NULL,
    path    TEXT NOT NULL,
    status  INTEGER NOT NULL,
    ip      TEXT NOT NULL,
    user_id INTEGER,
    ms      INTEGER NOT NULL
);
CREATE INDEX request_log_at ON request_log (at);
CREATE INDEX request_log_ip ON request_log (ip, at);
CREATE INDEX request_log_user ON request_log (user_id, at);

CREATE TABLE rate_rule (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint       TEXT NOT NULL,
    max_requests   INTEGER NOT NULL,
    window_secs    INTEGER NOT NULL,
    scope          TEXT NOT NULL DEFAULT 'ip',
    strikes_to_ban INTEGER,
    ban_secs       INTEGER NOT NULL DEFAULT 0,
    enabled        INTEGER NOT NULL DEFAULT 1,
    created_at     INTEGER NOT NULL DEFAULT 0
);

INSERT INTO rate_rule (endpoint, max_requests, window_secs, scope, strikes_to_ban, ban_secs) VALUES
    ('POST /api/v1/auth/player-login', 30, 60, 'ip', 10, 900),
    ('POST /api/v1/auth/register', 20, 60, 'ip', 10, 900),
    ('*', 1200, 60, 'ip', NULL, 0);

CREATE TABLE ban (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,
    subject    TEXT NOT NULL,
    reason     TEXT NOT NULL,
    auto       INTEGER NOT NULL DEFAULT 0,
    created_by INTEGER,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    lifted_at  INTEGER,
    lifted_by  INTEGER
);
CREATE INDEX ban_subject ON ban (kind, subject);

CREATE TABLE audit (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    at            INTEGER NOT NULL,
    actor_user_id INTEGER,
    actor_role    TEXT NOT NULL,
    action        TEXT NOT NULL,
    target_kind   TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    detail        TEXT NOT NULL DEFAULT ''
);
CREATE INDEX audit_at ON audit (at);
