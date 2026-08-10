BEGIN;

CREATE UNIQUE INDEX lessons_design_identity_uidx
  ON lessons (voice, title) NULLS NOT DISTINCT
  WHERE lesson_key = 'design';

INSERT INTO schema_migrations (version) VALUES (11)
ON CONFLICT (version) DO NOTHING;

COMMIT;
