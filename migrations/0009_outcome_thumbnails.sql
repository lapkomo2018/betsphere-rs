-- Image shown next to an outcome (team crest, candidate photo...). Like
-- `markets.thumbnail_url`, the database keeps only the URL; the bytes live in
-- file storage.
ALTER TABLE outcomes
    ADD COLUMN thumbnail_url TEXT NULL;
