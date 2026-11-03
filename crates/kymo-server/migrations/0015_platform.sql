-- The activity log of tournaments was called `event`; that name now belongs to the platform event.
ALTER TABLE event RENAME TO activity;
DROP INDEX event_by_tournament;
CREATE INDEX activity_by_tournament ON activity (tournament_id, id);

CREATE TABLE event (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    kind                TEXT NOT NULL CHECK (kind IN ('tournament', 'jeopardy')),
    mode                TEXT NOT NULL CHECK (mode IN ('solo', 'team')),
    name                TEXT NOT NULL,
    slug                TEXT NOT NULL UNIQUE,
    state               TEXT NOT NULL DEFAULT 'draft'
        CHECK (state IN ('draft', 'registration_open', 'live', 'paused', 'ended', 'archived')),
    registration_opens  INTEGER,
    registration_closes INTEGER,
    starts_at           INTEGER,
    ends_at             INTEGER,
    scoreboard_public   INTEGER NOT NULL DEFAULT 0,
    description         TEXT NOT NULL DEFAULT '',
    team_min            INTEGER NOT NULL DEFAULT 1,
    team_max            INTEGER NOT NULL DEFAULT 4,
    allow_invites       INTEGER NOT NULL DEFAULT 1,
    roster_locked       INTEGER NOT NULL DEFAULT 1,
    created_at          INTEGER NOT NULL
);

ALTER TABLE tournament ADD COLUMN event_id INTEGER REFERENCES event(id);

INSERT INTO event (kind, mode, name, slug, state, description, created_at)
SELECT 'tournament', 'team', name, slug,
       CASE status WHEN 'live' THEN 'live' WHEN 'done' THEN 'ended' ELSE 'draft' END,
       description,
       COALESCE(CAST(strftime('%s', created_at) AS INTEGER), CAST(strftime('%s', 'now') AS INTEGER))
FROM tournament
ORDER BY id;

-- Tournament slugs are unique and the event table was empty, so the slug identifies the pair.
UPDATE tournament SET event_id = (SELECT event.id FROM event WHERE event.slug = tournament.slug);

CREATE TABLE user (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    email         TEXT UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'player'
        CHECK (role IN ('player', 'referee', 'monitor', 'admin')),
    banned        INTEGER NOT NULL DEFAULT 0,
    display_name  TEXT NOT NULL DEFAULT '',
    bio           TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL
);

-- Named player_team so it does not collide with `team`, the roster of one tournament.
CREATE TABLE player_team (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL UNIQUE COLLATE NOCASE,
    owner_user_id INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    invite_code   TEXT NOT NULL UNIQUE,
    created_at    INTEGER NOT NULL
);

-- A user belongs to at most one team, hence UNIQUE (user_id).
CREATE TABLE player_team_member (
    team_id   INTEGER NOT NULL REFERENCES player_team(id) ON DELETE CASCADE,
    user_id   INTEGER NOT NULL UNIQUE REFERENCES user(id) ON DELETE CASCADE,
    role      TEXT NOT NULL CHECK (role IN ('owner', 'member')),
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, user_id)
);

-- Exactly one of user_id (solo events) or team_id (team events). The handler matches it to the mode.
CREATE TABLE event_participant (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id      INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    user_id       INTEGER REFERENCES user(id) ON DELETE CASCADE,
    team_id       INTEGER REFERENCES player_team(id) ON DELETE CASCADE,
    registered_at INTEGER NOT NULL,
    CHECK ((user_id IS NULL) <> (team_id IS NULL)),
    UNIQUE (event_id, user_id),
    UNIQUE (event_id, team_id)
);

CREATE TABLE team_invite (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id         INTEGER NOT NULL REFERENCES player_team(id) ON DELETE CASCADE,
    invited_user_id INTEGER REFERENCES user(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'revoked')),
    created_at      INTEGER NOT NULL
);

ALTER TABLE token ADD COLUMN user_id INTEGER REFERENCES user(id) ON DELETE CASCADE;
CREATE INDEX token_by_user ON token (user_id);
