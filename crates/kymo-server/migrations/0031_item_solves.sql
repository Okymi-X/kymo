-- DFIR questions answered in a Jeopardy event. Each correct question is worth its own points, so
-- the board counts it like a small challenge of its own (see jeopardy_store::ITEM_BASE).
ALTER TABLE submission ADD COLUMN item_id INTEGER REFERENCES dfir_item(id) ON DELETE SET NULL;

CREATE TABLE item_solve (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id       INTEGER NOT NULL REFERENCES event(id) ON DELETE CASCADE,
    challenge_id   INTEGER NOT NULL REFERENCES challenge(id) ON DELETE CASCADE,
    item_id        INTEGER NOT NULL REFERENCES dfir_item(id) ON DELETE CASCADE,
    participant_id INTEGER NOT NULL,
    user_id        INTEGER NOT NULL,
    points         INTEGER NOT NULL,
    at             INTEGER NOT NULL,
    UNIQUE (item_id, participant_id)
);
CREATE INDEX item_solve_by_event ON item_solve (event_id, participant_id);
