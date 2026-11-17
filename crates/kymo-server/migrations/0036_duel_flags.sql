-- A flag of a challenge with several flags is its own thing to answer in a match. The points of
-- a tournament flag are the flag's weight (default 1), set by the organizer.
ALTER TABLE duel_answer ADD COLUMN flag_id INTEGER REFERENCES challenge_flag(id) ON DELETE CASCADE;
DROP INDEX duel_answer_once;
CREATE UNIQUE INDEX duel_answer_once ON duel_answer (tournament_id, match_id, side_team_id, challenge_id, COALESCE(item_id, 0), COALESCE(flag_id, 0))
    WHERE verdict = 'correct';
