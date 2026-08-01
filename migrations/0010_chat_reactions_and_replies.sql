-- Replies: a message may quote an earlier one. The reply's room is not
-- re-stated here — it is the parent's, enforced when the reply is written —
-- so the column carries nothing but the link.
--
-- SET NULL rather than CASCADE: deleting a quoted message must not take the
-- replies to it with it; they simply stop being replies.
ALTER TABLE chat_messages
    ADD COLUMN reply_to_id UUID NULL REFERENCES chat_messages (id) ON DELETE SET NULL;

-- Nothing reads by parent, but the SET NULL above does: without this index
-- every delete scans the whole table looking for replies to clear.
CREATE INDEX idx_chat_messages_reply_to
    ON chat_messages (reply_to_id) WHERE reply_to_id IS NOT NULL;

-- Reactions: one row per (message, user, emoji), so a user holds each emoji on
-- a message at most once and the primary key is the whole uniqueness rule.
-- Its leading column is also what tallying a page of messages groups by.
CREATE TABLE chat_message_reactions (
    message_id UUID NOT NULL REFERENCES chat_messages (id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    emoji      TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, user_id, emoji)
);

-- The user cascade above needs this for the same reason as the reply index:
-- `user_id` is not a prefix of the primary key.
CREATE INDEX idx_chat_message_reactions_user ON chat_message_reactions (user_id);
