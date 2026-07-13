CREATE TABLE chat_messages (
    id         UUID PRIMARY KEY,
    author_id  UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Recent-messages queries page by newest first.
CREATE INDEX idx_chat_messages_created_at ON chat_messages (created_at DESC);
