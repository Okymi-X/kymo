-- The public scoreboard stops moving from this moment until the event ends. Null means no freeze.
ALTER TABLE event_jeopardy ADD COLUMN freeze_at INTEGER;
