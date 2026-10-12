-- 0035: The operator axis. A House may name the people who write and read its
-- memories: units form an ltree hierarchy, operators sit in one unit at one
-- rank, and every memory row may carry its writer and how far it travels.
-- With no operator rows the House reads exactly as before; rows without an
-- operator predate the axis and stay visible to everyone.
--
-- memories.operator is plain text, not a foreign key: the Host validates the
-- key and stamps it, and a House must keep remembering before anyone is
-- registered. A memory's identity becomes (room, writer, source_path), so two
-- operators writing the same path each own a row and neither can overwrite
-- or probe the other's.

BEGIN;

CREATE EXTENSION IF NOT EXISTS ltree;

CREATE TABLE IF NOT EXISTS units (
  path ltree PRIMARY KEY,
  display_name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS units_path_gist ON units USING GIST (path);

CREATE TABLE IF NOT EXISTS operators (
  key TEXT PRIMARY KEY CHECK (key ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'),
  display_name TEXT NOT NULL,
  unit ltree NOT NULL REFERENCES units(path),
  rank INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE memories
  ADD COLUMN IF NOT EXISTS operator TEXT NULL,
  ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'company'
    CHECK (scope IN ('company','up','private'));

-- A draft of this migration carried the foreign key; drop it where it landed.
ALTER TABLE memories DROP CONSTRAINT IF EXISTS memories_operator_fkey;

CREATE UNIQUE INDEX IF NOT EXISTS memories_room_operator_source_path
  ON memories (room, (COALESCE(operator, '')), source_path);

-- The old identity, whatever name the House gave it.
DO $drop_room_source_path$
DECLARE
  constraint_name TEXT;
BEGIN
  FOR constraint_name IN
    SELECT c.conname
    FROM pg_constraint c
    WHERE c.conrelid = 'memories'::regclass
      AND c.contype = 'u'
      AND (
        SELECT array_agg(a.attname::text ORDER BY a.attname)
        FROM unnest(c.conkey) AS k(attnum)
        JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum
      ) = ARRAY['room','source_path']
  LOOP
    EXECUTE format('ALTER TABLE memories DROP CONSTRAINT %I', constraint_name);
  END LOOP;
END
$drop_room_source_path$;

CREATE INDEX IF NOT EXISTS memories_operator
  ON memories (operator)
  WHERE operator IS NOT NULL;

INSERT INTO schema_migrations (version) VALUES (35) ON CONFLICT (version) DO NOTHING;

COMMIT;
