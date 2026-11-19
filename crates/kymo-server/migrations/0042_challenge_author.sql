-- Who made a challenge. Optional: an empty string means nobody is credited. Players see it on the
-- challenge as a recognition line.
ALTER TABLE challenge ADD COLUMN author TEXT NOT NULL DEFAULT '';
