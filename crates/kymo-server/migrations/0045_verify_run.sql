-- Every run of a Responder's Verifier, kept by the platform. The Verifier container and its log
-- vanish with the instance, so the platform copies each run here while the instance is alive and
-- once more as it ends. Organizers read it after the instance has expired, in a Jeopardy event or
-- a tournament alike.
CREATE TABLE verify_run (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    instance_id INTEGER NOT NULL REFERENCES instance(id) ON DELETE CASCADE,
    at          INTEGER NOT NULL,
    millis      INTEGER NOT NULL,
    passed      INTEGER NOT NULL,
    total       INTEGER NOT NULL,
    checks      TEXT NOT NULL,
    -- A run is copied more than once (periodically, then when the instance ends): the Verifier
    -- allows one run at a time, so the start time identifies it.
    UNIQUE (instance_id, at)
);
