-- Transactional outbox: domain events written in the same transaction as the
-- change they describe, delivered asynchronously by the outbox processor.
-- Rows are marked processed rather than deleted immediately (kept briefly for
-- debugging); the processor prunes old processed rows.
CREATE TABLE outbox_events (
    id           BIGSERIAL PRIMARY KEY,
    topic        TEXT NOT NULL,
    payload      JSONB NOT NULL,
    attempts     INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);

-- The processor claims pending events in insertion order.
CREATE INDEX idx_outbox_pending ON outbox_events (id) WHERE processed_at IS NULL;
