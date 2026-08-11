BEGIN;

ALTER TABLE named_entities
  ADD COLUMN authority TEXT NOT NULL DEFAULT 'active',
  ADD COLUMN superseded_by BIGINT,
  ADD COLUMN supersedes BIGINT[] NOT NULL DEFAULT '{}',
  ADD COLUMN attributed_by TEXT NOT NULL DEFAULT 'migration:0015',
  ADD COLUMN attribution_origin TEXT NOT NULL DEFAULT 'legacy';

ALTER TABLE named_entities
  ADD CONSTRAINT named_entities_authority_check
    CHECK (authority IN ('active', 'superseded', 'archived')),
  ADD CONSTRAINT named_entities_superseded_state_check
    CHECK (
      (authority = 'active' AND superseded_by IS NULL)
      OR authority = 'archived'
      OR (authority = 'superseded' AND superseded_by IS NOT NULL)
    ),
  ADD CONSTRAINT named_entities_superseded_by_fkey
    FOREIGN KEY (superseded_by) REFERENCES named_entities(id)
    DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE named_entities
  DROP CONSTRAINT IF EXISTS named_entities_room_name_key;

CREATE UNIQUE INDEX named_entities_active_room_name_key
  ON named_entities (room, name)
  WHERE authority = 'active';
CREATE INDEX named_entities_superseded_by_idx
  ON named_entities (superseded_by)
  WHERE superseded_by IS NOT NULL;
CREATE INDEX named_entities_authority_room_idx
  ON named_entities (authority, room, updated_at DESC, id DESC);

-- The semantic vocabulary is a derived active-authority index. Replacing the
-- source function here ensures a superseded entity disappears from both direct
-- canon recall and the semantic bridge on the same refresh.
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
    WHERE entity.authority = 'active'

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

INSERT INTO schema_migrations (version) VALUES (15)
ON CONFLICT (version) DO NOTHING;

COMMIT;
