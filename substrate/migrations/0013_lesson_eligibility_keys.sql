BEGIN;

ALTER TABLE lessons
  ADD COLUMN language_keys TEXT[] NOT NULL DEFAULT '{}',
  ADD COLUMN technology_keys TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX lessons_language_keys_gin
  ON lessons USING GIN (language_keys);
CREATE INDEX lessons_technology_keys_gin
  ON lessons USING GIN (technology_keys);

INSERT INTO schema_migrations (version) VALUES (13)
ON CONFLICT (version) DO NOTHING;

COMMIT;
