ALTER TABLE event ADD COLUMN ended_at INTEGER;

UPDATE event
SET ended_at = ends_at
WHERE state IN ('ended', 'archived')
  AND ends_at IS NOT NULL
  AND ends_at <= CAST(strftime('%s', 'now') AS INTEGER);
