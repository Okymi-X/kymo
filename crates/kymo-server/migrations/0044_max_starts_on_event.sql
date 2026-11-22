-- The default start budget moves from `event_jeopardy` to `event`, so it applies to a
-- tournament event's challenges too, not only a Jeopardy event's. `challenge.max_starts` (the
-- per-challenge override, added in 0043) is unaffected.
ALTER TABLE event ADD COLUMN max_starts INTEGER NOT NULL DEFAULT 5;
UPDATE event SET max_starts = (
    SELECT max_starts FROM event_jeopardy WHERE event_jeopardy.event_id = event.id
) WHERE EXISTS (SELECT 1 FROM event_jeopardy WHERE event_jeopardy.event_id = event.id);
ALTER TABLE event_jeopardy DROP COLUMN max_starts;
