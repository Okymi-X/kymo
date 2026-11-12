-- The rules of an event, shown to players in a dialog on the event page. One row per event that
-- has rules; deleting the event deletes them.
CREATE TABLE event_rules (
    event_id   INTEGER PRIMARY KEY REFERENCES event(id) ON DELETE CASCADE,
    title      TEXT NOT NULL DEFAULT 'Rules',
    body       TEXT NOT NULL DEFAULT '',
    updated_at INTEGER NOT NULL
);
