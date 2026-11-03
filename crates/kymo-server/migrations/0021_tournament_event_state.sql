-- A tournament's event used to stay a draft for ever. Bring existing ones in line with the
-- tournament they wrap; from now on saving the bracket keeps them in step.
UPDATE event SET state = CASE (SELECT status FROM tournament WHERE tournament.event_id = event.id)
    WHEN 'live' THEN 'live'
    WHEN 'done' THEN 'ended'
    ELSE 'draft'
END
WHERE id IN (SELECT event_id FROM tournament WHERE event_id IS NOT NULL);
