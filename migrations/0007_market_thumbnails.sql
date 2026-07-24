-- Cover image shown on market cards. Like `users.avatar_url`, the database
-- keeps only the URL; the bytes live in file storage.
ALTER TABLE markets
    ADD COLUMN thumbnail_url TEXT NULL;
