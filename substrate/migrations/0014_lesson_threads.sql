BEGIN;

ALTER TABLE lessons
  ADD COLUMN thread_keys TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX lessons_thread_keys_gin
  ON lessons USING GIN (thread_keys);

INSERT INTO schema_migrations (version) VALUES (14)
ON CONFLICT (version) DO NOTHING;

COMMIT;
