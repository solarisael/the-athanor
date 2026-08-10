BEGIN;

ALTER TABLE giga_event_sources
  ADD COLUMN IF NOT EXISTS source_ordinal INTEGER;

WITH ordered AS (
  SELECT id, ROW_NUMBER() OVER (PARTITION BY event_id ORDER BY id) - 1 AS source_ordinal
  FROM giga_event_sources
)
UPDATE giga_event_sources AS source
SET source_ordinal = ordered.source_ordinal
FROM ordered
WHERE source.id = ordered.id
  AND source.source_ordinal IS NULL;

ALTER TABLE giga_event_sources
  ALTER COLUMN source_ordinal SET NOT NULL;

ALTER TABLE giga_event_sources
  DROP CONSTRAINT IF EXISTS giga_event_sources_source_ordinal_nonnegative;
ALTER TABLE giga_event_sources
  ADD CONSTRAINT giga_event_sources_source_ordinal_nonnegative CHECK (source_ordinal >= 0);

CREATE UNIQUE INDEX IF NOT EXISTS giga_event_sources_event_ordinal_uidx
  ON giga_event_sources(event_id, source_ordinal);

INSERT INTO schema_migrations (version) VALUES (7)
ON CONFLICT (version) DO NOTHING;

COMMIT;
