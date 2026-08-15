BEGIN;

ALTER TABLE lessons ADD COLUMN IF NOT EXISTS condition TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE lessons ADD COLUMN IF NOT EXISTS ast_condition TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE lessons ADD COLUMN IF NOT EXISTS trigger_scope TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE lessons ADD COLUMN IF NOT EXISTS interrupt_mode TEXT;
ALTER TABLE lessons ADD COLUMN IF NOT EXISTS repeat_cooldown_secs INTEGER;

-- NULL interrupt_mode means 'block': a trigger-bearing lesson interrupts by
-- default, and demotion to 'remind' is an explicit UPDATE.
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid = 'lessons'::regclass
      AND conname = 'lessons_interrupt_mode_check'
  ) THEN
    ALTER TABLE lessons
      ADD CONSTRAINT lessons_interrupt_mode_check
      CHECK (interrupt_mode IS NULL OR interrupt_mode IN ('block', 'remind'));
  END IF;
END
$$;

CREATE INDEX IF NOT EXISTS lessons_trigger_bearing_idx
  ON lessons (lesson_key, scope, project)
  WHERE condition <> '{}' OR ast_condition <> '{}';

CREATE TABLE IF NOT EXISTS lesson_trigger_events (
  id BIGSERIAL PRIMARY KEY,
  lesson_key TEXT NOT NULL,
  lesson_id BIGINT NOT NULL,
  room TEXT NOT NULL,
  session_id TEXT NOT NULL,
  surface TEXT NOT NULL CHECK (surface IN ('tool', 'prose')),
  tool_name TEXT,
  path TEXT,
  pattern_kind TEXT NOT NULL CHECK (pattern_kind IN ('regex', 'ast')),
  matched_pattern TEXT NOT NULL,
  urgency TEXT NOT NULL CHECK (urgency IN ('block', 'remind')),
  fired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  FOREIGN KEY (lesson_key, lesson_id)
    REFERENCES lessons(lesson_key, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS lesson_trigger_events_repeat_idx
  ON lesson_trigger_events (room, session_id, lesson_id, fired_at DESC);

INSERT INTO schema_migrations (version) VALUES (19)
ON CONFLICT (version) DO NOTHING;

COMMIT;
