-- A challenge can require another one of the same event: players see it locked until they have
-- solved the parent. Off by default. The server refuses a chain that loops back on itself.
ALTER TABLE challenge ADD COLUMN requires_challenge_id INTEGER REFERENCES challenge(id) ON DELETE SET NULL;
