-- What an event does about a player who submits a dynamic flag that was generated for somebody
-- else: `sharing_action` is off, manual (organizers decide, the player is only warned and
-- reported) or auto_ban, and `sharing_warnings` is how many such submissions only warn.
ALTER TABLE event_jeopardy ADD COLUMN sharing_action TEXT NOT NULL DEFAULT 'auto_ban';
ALTER TABLE event_jeopardy ADD COLUMN sharing_warnings INTEGER NOT NULL DEFAULT 1;

-- One row per submission of another player's dynamic flag. The flag value is never stored.
CREATE TABLE sharing_strike (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id     INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
    challenge_id INTEGER NOT NULL,
    at           INTEGER NOT NULL
);
CREATE INDEX sharing_strike_user ON sharing_strike (event_id, user_id);
