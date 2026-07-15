-- Chat rooms per market: NULL market_id is the global room, a non-NULL value
-- scopes the message to that market's discussion.
ALTER TABLE chat_messages
    ADD COLUMN market_id UUID NULL REFERENCES markets (id) ON DELETE CASCADE;

-- History is always read newest-first within one room; this covers both the
-- global room (market_id IS NULL) and per-market rooms, replacing the old
-- created_at-only index.
DROP INDEX idx_chat_messages_created_at;
CREATE INDEX idx_chat_messages_market_created_at
    ON chat_messages (market_id, created_at DESC);
