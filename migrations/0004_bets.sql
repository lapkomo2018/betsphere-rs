-- Bets: user stakes on market outcomes. `price` is the outcome price fixed at
-- placement, in integer ten-thousandths (see 0003_markets.sql); `amount` and
-- `payout` are minimal currency units like users.balance.
CREATE TABLE bets (
    id         UUID PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users (id),
    market_id  UUID NOT NULL REFERENCES markets (id),
    outcome_id UUID NOT NULL REFERENCES outcomes (id),
    amount     BIGINT NOT NULL CHECK (amount > 0),
    price      INTEGER NOT NULL CHECK (price > 0 AND price <= 10000),
    status     TEXT NOT NULL DEFAULT 'active',
    payout     BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A user's history and a market's bet list, newest first.
CREATE INDEX idx_bets_user_created ON bets (user_id, created_at DESC);
CREATE INDEX idx_bets_market_created ON bets (market_id, created_at DESC);
-- The global feed, newest first.
CREATE INDEX idx_bets_created ON bets (created_at DESC);
-- Settlement scans a market's still-active bets.
CREATE INDEX idx_bets_market_status ON bets (market_id, status);
