-- Type catalog overrides (V1.1, SPEC §19.6): only types that differ from the defaults, and every custom type.
CREATE TABLE type_settings (
  code TEXT PRIMARY KEY,
  color_id TEXT,
  label_fr TEXT NOT NULL, label_plural_fr TEXT NOT NULL,
  label_en TEXT NOT NULL, label_plural_en TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Resumable color migrations (SPEC §19.5).
CREATE TABLE type_migrations (
  id TEXT PRIMARY KEY, started_at TEXT NOT NULL, finished_at TEXT,
  status TEXT NOT NULL CHECK (status IN ('running','done','failed')),
  target TEXT NOT NULL,
  mapping TEXT NOT NULL,
  total INTEGER, patched INTEGER, error TEXT
);
