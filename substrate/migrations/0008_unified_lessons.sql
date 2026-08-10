BEGIN;

CREATE SEQUENCE IF NOT EXISTS lessons_id_seq;

CREATE TABLE lessons (
  lesson_key TEXT NOT NULL CHECK (lesson_key ~ '^[a-z][a-z0-9-]{0,63}$'),
  kind_path TEXT GENERATED ALWAYS AS (
    lesson_key || '/' ||
    COALESCE(
      NULLIF(BTRIM(REGEXP_REPLACE(LOWER(COALESCE(shape, '')), '[^a-z0-9]+', '-', 'g'), '-'), ''),
      'general'
    )
  ) STORED,
  id BIGINT NOT NULL DEFAULT nextval('lessons_id_seq'),
  scope TEXT NOT NULL DEFAULT 'house',
  project TEXT,
  voice TEXT,
  register TEXT[] NOT NULL DEFAULT '{}',
  shape TEXT,
  stage TEXT[] NOT NULL DEFAULT '{}',
  title TEXT NOT NULL,
  lesson TEXT NOT NULL,
  trigger_context TEXT,
  proof_pattern TEXT,
  example_text TEXT,
  example_cmd TEXT,
  writers TEXT[] NOT NULL DEFAULT '{}',
  tools TEXT[] NOT NULL DEFAULT '{}',
  negation_of BIGINT,
  tags TEXT[] NOT NULL DEFAULT '{}',
  source_memory_path TEXT,
  source_lines_start INTEGER,
  source_lines_end INTEGER,
  always_on BOOLEAN NOT NULL DEFAULT FALSE,
  meta JSONB NOT NULL DEFAULT '{}',
  lesson_tsv TSVECTOR GENERATED ALWAYS AS (
    to_tsvector(
      CASE WHEN lesson_key = 'audio' THEN 'english'::regconfig ELSE 'portuguese'::regconfig END,
      coalesce(title, '') || ' ' || coalesce(project, '') || ' ' || lesson || ' ' ||
      coalesce(trigger_context, '') || ' ' || coalesce(proof_pattern, '') || ' ' ||
      coalesce(example_text, '') || ' ' || coalesce(example_cmd, '')
    )
  ) STORED,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (lesson_key, id),
  FOREIGN KEY (lesson_key, negation_of)
    REFERENCES lessons(lesson_key, id) ON DELETE SET NULL (negation_of)
);

ALTER SEQUENCE lessons_id_seq OWNED BY lessons.id;

INSERT INTO lessons (
  lesson_key,id,scope,project,voice,shape,title,lesson,trigger_context,proof_pattern,
  negation_of,tags,source_memory_path,source_lines_start,source_lines_end,always_on,
  meta,created_at,updated_at
)
SELECT
  'coding',id,scope,project,voice,shape,title,lesson,trigger_context,proof_pattern,
  negation_of,tags,source_memory_path,source_lines_start,source_lines_end,always_on,
  meta,created_at,updated_at
FROM coding_lessons;

INSERT INTO lessons (
  lesson_key,id,scope,project,title,lesson,trigger_context,proof_pattern,tags,
  source_memory_path,source_lines_start,source_lines_end,meta,created_at,updated_at
)
SELECT
  'project',id,'project',project,title,lesson,trigger_context,proof_pattern,tags,
  source_memory_path,source_lines_start,source_lines_end,meta,created_at,updated_at
FROM project_lessons;

INSERT INTO lessons (
  lesson_key,id,scope,voice,register,shape,title,lesson,trigger_context,example_text,
  writers,negation_of,tags,source_memory_path,source_lines_start,source_lines_end,
  meta,created_at,updated_at
)
SELECT
  'writing',id,'house',voice,register,shape,title,lesson,trigger_context,example_text,
  writers,negation_of,tags,source_memory_path,source_lines_start,source_lines_end,
  meta,created_at,updated_at
FROM writing_lessons;

INSERT INTO lessons (
  lesson_key,id,scope,shape,stage,title,lesson,trigger_context,example_cmd,tools,
  negation_of,tags,source_memory_path,created_at,updated_at
)
SELECT
  'audio',id,'house',shape,stage,title,lesson,trigger_context,example_cmd,tools,
  negation_of,tags,source_memory_path,created_at,created_at
FROM audio_lessons;

DO $migration_guard$
DECLARE
  expected BIGINT;
  copied BIGINT;
BEGIN
  SELECT
    (SELECT COUNT(*) FROM coding_lessons) +
    (SELECT COUNT(*) FROM project_lessons) +
    (SELECT COUNT(*) FROM writing_lessons) +
    (SELECT COUNT(*) FROM audio_lessons)
  INTO expected;
  SELECT COUNT(*) FROM lessons INTO copied;
  IF copied <> expected THEN
    RAISE EXCEPTION 'lesson migration count mismatch: expected %, copied %', expected, copied;
  END IF;
END
$migration_guard$;

SELECT setval(
  'lessons_id_seq',
  GREATEST(COALESCE((SELECT MAX(id) FROM lessons), 0) + 1, 1),
  false
);

DROP TABLE coding_lessons;
DROP TABLE project_lessons;
DROP TABLE writing_lessons;
DROP TABLE audio_lessons;

DROP TRIGGER IF EXISTS lessons_updated_at ON lessons;
CREATE TRIGGER lessons_updated_at BEFORE UPDATE ON lessons
  FOR EACH ROW EXECUTE FUNCTION substrate_set_updated_at();

CREATE UNIQUE INDEX lessons_coding_identity_uidx
  ON lessons (scope, project, title) NULLS NOT DISTINCT
  WHERE lesson_key = 'coding';
CREATE UNIQUE INDEX lessons_project_identity_uidx
  ON lessons (project, title)
  WHERE lesson_key = 'project';
CREATE UNIQUE INDEX lessons_writing_identity_uidx
  ON lessons (voice, title) NULLS NOT DISTINCT
  WHERE lesson_key = 'writing';
CREATE UNIQUE INDEX lessons_audio_identity_uidx
  ON lessons (title)
  WHERE lesson_key = 'audio';
CREATE INDEX lessons_lesson_tsv_gin ON lessons USING GIN (lesson_tsv);
CREATE INDEX lessons_meta_gin ON lessons USING GIN (meta jsonb_path_ops);
CREATE INDEX lessons_key_scope_project_idx
  ON lessons (lesson_key, scope, project, updated_at DESC);
CREATE INDEX lessons_kind_path_idx ON lessons (kind_path, updated_at DESC);
CREATE INDEX lessons_shape_trgm ON lessons USING GIN (shape gin_trgm_ops);
CREATE INDEX lessons_tags_gin ON lessons USING GIN (tags);
CREATE INDEX lessons_title_trgm ON lessons USING GIN (title gin_trgm_ops);
CREATE INDEX lessons_register_gin ON lessons USING GIN (register);
CREATE INDEX lessons_stage_gin ON lessons USING GIN (stage);
CREATE INDEX lessons_writers_gin ON lessons USING GIN (writers);

INSERT INTO schema_migrations (version) VALUES (8)
ON CONFLICT (version) DO NOTHING;

COMMIT;
