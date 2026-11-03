-- Why a run ended as failed: start_failed, never_ready or exited. Shown to the player.
ALTER TABLE instance ADD COLUMN failure TEXT;
