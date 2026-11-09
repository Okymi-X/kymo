-- A challenge may run on its own lab host; none follows the event's choice.
ALTER TABLE challenge ADD COLUMN lab_host_id INTEGER REFERENCES lab_host(id) ON DELETE SET NULL;
