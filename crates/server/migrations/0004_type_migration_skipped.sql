-- Ids (JSON array) of the events a finished color pass could not recolor (SPEC §19.5).
ALTER TABLE type_migrations ADD COLUMN skipped TEXT;
