-- Event lifecycle: when registration stops, the organizer's own registration switch, and when an
-- event actually started (so an early start can be shown as such).
ALTER TABLE event ADD COLUMN registration_policy TEXT NOT NULL DEFAULT 'at_start'
    CHECK (registration_policy IN ('at_start', 'at_time', 'manual'));
ALTER TABLE event ADD COLUMN registration_closed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE event ADD COLUMN started_at INTEGER;

-- Events that already had a closing time behaved as "at that time".
UPDATE event SET registration_policy = 'at_time' WHERE registration_closes IS NOT NULL;
UPDATE event SET started_at = starts_at
WHERE state IN ('live', 'paused', 'ended', 'archived') AND starts_at IS NOT NULL;
