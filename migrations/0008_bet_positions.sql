-- Bet listings report the stake-weighted average price of the bettor's still-
-- open stake on each row's outcome, which aggregates that user's active bets
-- on one outcome. Without this index the lookup falls back to the (user_id,
-- created_at) index and filters the user's whole history per row.
CREATE INDEX idx_bets_user_outcome_status ON bets (user_id, outcome_id, status);
