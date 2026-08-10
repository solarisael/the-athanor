-- Migrate the semantic space from Qwen halfvec(2560) to Nemotron vector(2048).
-- Reset data only when a column still uses the old embedding type.

BEGIN;

DO $migration$
DECLARE
  memory_shape TEXT;
  anamnesis_shape TEXT;
  cluster_shape TEXT;
BEGIN
  SELECT format_type(a.atttypid, a.atttypmod)
  INTO memory_shape
  FROM pg_attribute a
  JOIN pg_class c ON c.oid = a.attrelid
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relname = 'memory_chunks'
    AND n.nspname = current_schema()
    AND a.attname = 'body_embedding'
    AND NOT a.attisdropped;

  IF memory_shape IS NULL THEN
    RAISE EXCEPTION 'memory_chunks.body_embedding is missing';
  ELSIF memory_shape <> 'vector(2048)' THEN
    DROP INDEX IF EXISTS memory_chunks_emb_hnsw;
    DELETE FROM memory_cluster_members;
    DELETE FROM memory_clusters;
    UPDATE memory_chunks
    SET body_embedding = NULL,
        embedded_at = NULL;
    ALTER TABLE memory_chunks
      ALTER COLUMN body_embedding TYPE vector(2048)
      USING NULL::vector(2048);
  END IF;

  SELECT format_type(a.atttypid, a.atttypmod)
  INTO anamnesis_shape
  FROM pg_attribute a
  JOIN pg_class c ON c.oid = a.attrelid
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relname = 'anamnesis'
    AND n.nspname = current_schema()
    AND a.attname = 'body_embedding'
    AND NOT a.attisdropped;

  IF anamnesis_shape IS NULL THEN
    RAISE EXCEPTION 'anamnesis.body_embedding is missing';
  ELSIF anamnesis_shape <> 'vector(2048)' THEN
    UPDATE anamnesis
    SET body_embedding = NULL,
        embedded_at = NULL;
    ALTER TABLE anamnesis
      ALTER COLUMN body_embedding TYPE vector(2048)
      USING NULL::vector(2048);
  END IF;

  SELECT format_type(a.atttypid, a.atttypmod)
  INTO cluster_shape
  FROM pg_attribute a
  JOIN pg_class c ON c.oid = a.attrelid
  JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relname = 'memory_clusters'
    AND n.nspname = current_schema()
    AND a.attname = 'centroid'
    AND NOT a.attisdropped;

  IF cluster_shape IS NULL THEN
    RAISE EXCEPTION 'memory_clusters.centroid is missing';
  ELSIF cluster_shape <> 'vector(2048)' THEN
    DELETE FROM memory_cluster_members;
    DELETE FROM memory_clusters;
    ALTER TABLE memory_clusters
      ALTER COLUMN centroid TYPE vector(2048)
      USING NULL::vector(2048);
  END IF;
END
$migration$;


INSERT INTO schema_migrations (version, applied_at)
VALUES (2, NOW())
ON CONFLICT (version) DO NOTHING;

COMMIT;
