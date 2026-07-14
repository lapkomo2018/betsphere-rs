CREATE TABLE markets (
    id                  UUID PRIMARY KEY,
    title               TEXT NOT NULL,
    description         TEXT NULL,
    category            TEXT NULL,
    status              TEXT NOT NULL DEFAULT 'open',
    resolved_outcome_id UUID NULL,
    total_volume        BIGINT NOT NULL DEFAULT 0,
    participants_count  INT NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    closes_at           TIMESTAMPTZ NULL
);

CREATE TABLE outcomes (
    -- `seq` preserves the order outcomes were created in (Yes/No, options...)
    -- for stable presentation; `id` is the stable external identifier.
    seq           BIGSERIAL NOT NULL,
    id            UUID PRIMARY KEY,
    market_id     UUID NOT NULL REFERENCES markets (id) ON DELETE CASCADE,
    label         TEXT NOT NULL,
    -- Price as an integer count of ten-thousandths (0..=10000 = 0.0000..1.0000),
    -- keeping prices off floating point like balances.
    current_price INT NOT NULL,
    volume        BIGINT NOT NULL DEFAULT 0
);

-- The winning outcome, set on resolve. Added after `outcomes` exists so the
-- foreign key can reference it.
ALTER TABLE markets
    ADD CONSTRAINT fk_markets_resolved_outcome
    FOREIGN KEY (resolved_outcome_id) REFERENCES outcomes (id);

CREATE TABLE price_history (
    id          BIGSERIAL PRIMARY KEY,
    outcome_id  UUID NOT NULL REFERENCES outcomes (id) ON DELETE CASCADE,
    price       INT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Listing filters and sorts.
CREATE INDEX idx_markets_status ON markets (status);
CREATE INDEX idx_markets_category ON markets (category);
CREATE INDEX idx_markets_created_at ON markets (created_at DESC);
CREATE INDEX idx_markets_total_volume ON markets (total_volume DESC);

-- Outcome and price-history lookups by parent.
CREATE INDEX idx_outcomes_market_id ON outcomes (market_id, seq);
CREATE INDEX idx_price_history_outcome ON price_history (outcome_id, recorded_at);
