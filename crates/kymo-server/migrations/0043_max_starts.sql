-- The start budget (starts plus restarts) a participant gets on a challenge: an event-wide
-- default, overridable per challenge for one that needs more (or fewer) attempts than most.
ALTER TABLE event_jeopardy ADD COLUMN max_starts INTEGER NOT NULL DEFAULT 5;
ALTER TABLE challenge ADD COLUMN max_starts INTEGER;
