BEGIN;

CREATE TABLE semantic_vocabulary (
  room TEXT NOT NULL,
  source_kind TEXT NOT NULL CHECK (source_kind IN ('named_entity', 'active_thread', 'lesson_metadata')),
  source_key TEXT NOT NULL,
  concept TEXT NOT NULL,
  lexical_terms TEXT[] NOT NULL DEFAULT '{}',
  source_updated_at TIMESTAMPTZ NOT NULL,
  approved_source_kind TEXT NOT NULL CHECK (approved_source_kind IN ('named_entity', 'active_thread', 'lesson_metadata')),
  embedding VECTOR(2048),
  embedding_model TEXT,
  embedding_dimension INTEGER,
  embedded_at TIMESTAMPTZ,
  PRIMARY KEY (room, source_kind, source_key),
  CHECK (approved_source_kind = source_kind),
  CHECK (
    (embedding IS NULL AND embedding_model IS NULL AND embedding_dimension IS NULL AND embedded_at IS NULL)
    OR (embedding IS NOT NULL AND embedding_model IS NOT NULL AND embedding_dimension = 2048 AND embedded_at IS NOT NULL)
  )
);
CREATE INDEX semantic_vocabulary_room_embedding_idx
  ON semantic_vocabulary (room, source_kind, source_key)
  WHERE embedding IS NOT NULL;

-- Only these authority-bearing source kinds may build the semantic lexical
-- bridge. The function is deterministic and does not invent vocabulary.
CREATE OR REPLACE FUNCTION substrate_refresh_semantic_vocabulary_sources()
RETURNS VOID
LANGUAGE SQL
AS $$
  WITH source_rows AS (
    SELECT
      entity.room,
      'named_entity'::text AS source_kind,
      entity.id::text AS source_key,
      entity.name AS concept,
      array_remove(array_cat(ARRAY[entity.name, entity.kind], coalesce(entity.aliases, '{}'::text[])), '') AS lexical_terms,
      entity.updated_at AS source_updated_at
    FROM named_entities entity

    UNION ALL

    SELECT
      thread.room,
      'active_thread'::text,
      thread.id::text,
      thread.thread_key,
      ARRAY[thread.thread_key],
      max(event.created_at)
    FROM threads thread
    JOIN thread_events event ON event.thread_id = thread.id
    JOIN memories memory ON memory.id = event.memory_id
    WHERE memory.archived_at IS NULL
      AND memory.superseded_by IS NULL
    GROUP BY thread.room, thread.id, thread.thread_key

    UNION ALL

    SELECT
      CASE
        WHEN lesson.lesson_key = 'coding' AND lesson.scope <> 'house' THEN lesson.scope
        ELSE 'house'
      END,
      'lesson_metadata'::text,
      lesson.lesson_key || ':' || lesson.id::text,
      lesson.title,
      array_remove(array_cat(ARRAY[lesson.title, lesson.lesson_key, coalesce(lesson.project, ''), coalesce(lesson.shape, '')], coalesce(lesson.tags, '{}'::text[])), ''),
      lesson.updated_at
    FROM lessons lesson
  ), normalized AS (
    SELECT room, source_kind, source_key, concept,
      ARRAY(SELECT DISTINCT lower(trim(term)) FROM unnest(lexical_terms) term WHERE trim(term) <> '' ORDER BY lower(trim(term))) AS lexical_terms,
      source_updated_at
    FROM source_rows
  ), upserted AS (
    INSERT INTO semantic_vocabulary (
      room, source_kind, source_key, concept, lexical_terms, source_updated_at, approved_source_kind
    )
    SELECT room, source_kind, source_key, concept, lexical_terms, source_updated_at, source_kind
    FROM normalized
    ON CONFLICT (room, source_kind, source_key) DO UPDATE SET
      concept = EXCLUDED.concept,
      lexical_terms = EXCLUDED.lexical_terms,
      source_updated_at = EXCLUDED.source_updated_at,
      approved_source_kind = EXCLUDED.approved_source_kind,
      embedding = CASE WHEN (semantic_vocabulary.concept, semantic_vocabulary.lexical_terms, semantic_vocabulary.source_updated_at)
                         IS DISTINCT FROM (EXCLUDED.concept, EXCLUDED.lexical_terms, EXCLUDED.source_updated_at)
                       THEN NULL ELSE semantic_vocabulary.embedding END,
      embedding_model = CASE WHEN (semantic_vocabulary.concept, semantic_vocabulary.lexical_terms, semantic_vocabulary.source_updated_at)
                               IS DISTINCT FROM (EXCLUDED.concept, EXCLUDED.lexical_terms, EXCLUDED.source_updated_at)
                             THEN NULL ELSE semantic_vocabulary.embedding_model END,
      embedding_dimension = CASE WHEN (semantic_vocabulary.concept, semantic_vocabulary.lexical_terms, semantic_vocabulary.source_updated_at)
                                   IS DISTINCT FROM (EXCLUDED.concept, EXCLUDED.lexical_terms, EXCLUDED.source_updated_at)
                                 THEN NULL ELSE semantic_vocabulary.embedding_dimension END,
      embedded_at = CASE WHEN (semantic_vocabulary.concept, semantic_vocabulary.lexical_terms, semantic_vocabulary.source_updated_at)
                           IS DISTINCT FROM (EXCLUDED.concept, EXCLUDED.lexical_terms, EXCLUDED.source_updated_at)
                         THEN NULL ELSE semantic_vocabulary.embedded_at END
    RETURNING room, source_kind, source_key
  )
  DELETE FROM semantic_vocabulary vocabulary
  WHERE NOT EXISTS (
    SELECT 1 FROM normalized source
    WHERE (source.room, source.source_kind, source.source_key) =
          (vocabulary.room, vocabulary.source_kind, vocabulary.source_key)
  );
$$;

COMMIT;
