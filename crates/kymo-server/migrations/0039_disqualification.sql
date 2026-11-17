-- Disqualifying a participant from one event: off its scoreboard and out of its play, without
-- banning the account from the platform. `disqualified_at` is when, `disqualified_reason` why.
ALTER TABLE event_participant ADD COLUMN disqualified_at INTEGER;
ALTER TABLE event_participant ADD COLUMN disqualified_reason TEXT NOT NULL DEFAULT '';
