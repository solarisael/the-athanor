BEGIN;

CREATE TABLE threads (
  id BIGSERIAL PRIMARY KEY,
  room TEXT NOT NULL,
  thread_key TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (room, thread_key)
);

CREATE TABLE thread_events (
  id BIGSERIAL PRIMARY KEY,
  thread_id BIGINT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  memory_id BIGINT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (thread_id, memory_id),
  UNIQUE (thread_id, id)
);
CREATE INDEX thread_events_memory_id_idx ON thread_events (memory_id);

INSERT INTO threads (room, thread_key)
SELECT DISTINCT m.room, mt.thread_key
FROM memory_threads mt
JOIN memories m ON m.id = mt.memory_id;

INSERT INTO thread_events (thread_id, memory_id)
SELECT DISTINCT t.id, mt.memory_id
FROM memory_threads mt
JOIN memories m ON m.id = mt.memory_id
JOIN threads t ON t.room = m.room AND t.thread_key = mt.thread_key;

ALTER TABLE memory_threads RENAME TO memory_thread_refs;
ALTER TABLE memory_thread_refs ADD COLUMN event_id BIGINT;

UPDATE memory_thread_refs ref
SET event_id = event.id
FROM memories memory
JOIN threads thread ON thread.room = memory.room
JOIN thread_events event ON event.thread_id = thread.id AND event.memory_id = memory.id
WHERE memory.id = ref.memory_id
  AND thread.thread_key = ref.thread_key;

ALTER TABLE memory_thread_refs
  ALTER COLUMN event_id SET NOT NULL,
  ADD CONSTRAINT memory_thread_refs_event_id_fkey
    FOREIGN KEY (event_id) REFERENCES thread_events(id) ON DELETE CASCADE,
  DROP COLUMN memory_id,
  DROP COLUMN thread_key;
CREATE INDEX memory_thread_refs_event_id_idx ON memory_thread_refs (event_id);

CREATE TABLE thread_event_links (
  thread_id BIGINT NOT NULL,
  previous_event_id BIGINT NOT NULL,
  next_event_id BIGINT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (thread_id, next_event_id),
  CONSTRAINT thread_event_links_previous_event_fkey
    FOREIGN KEY (thread_id, previous_event_id)
    REFERENCES thread_events(thread_id, id) ON DELETE CASCADE,
  CONSTRAINT thread_event_links_next_event_fkey
    FOREIGN KEY (thread_id, next_event_id)
    REFERENCES thread_events(thread_id, id) ON DELETE CASCADE,
  CONSTRAINT thread_event_links_not_self CHECK (previous_event_id <> next_event_id)
);
CREATE INDEX thread_event_links_previous_event_idx
  ON thread_event_links (thread_id, previous_event_id);

CREATE FUNCTION substrate_reject_thread_event_link_cycle() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  replaced_thread_id BIGINT;
  replaced_next_event_id BIGINT;
BEGIN
  -- Serialize graph changes within a thread so concurrent writes cannot each
  -- validate against a graph that omits the other write.
  PERFORM pg_advisory_xact_lock(NEW.thread_id);

  IF TG_OP = 'UPDATE' THEN
    replaced_thread_id := OLD.thread_id;
    replaced_next_event_id := OLD.next_event_id;
  END IF;

  IF EXISTS (
    WITH RECURSIVE descendants(event_id) AS (
      SELECT NEW.next_event_id
      UNION
      SELECT link.next_event_id
      FROM thread_event_links link
      JOIN descendants ON descendants.event_id = link.previous_event_id
      WHERE link.thread_id = NEW.thread_id
        AND (
          replaced_next_event_id IS NULL
          OR (link.thread_id, link.next_event_id)
             IS DISTINCT FROM (replaced_thread_id, replaced_next_event_id)
        )
    )
    SELECT 1
    FROM descendants
    WHERE event_id = NEW.previous_event_id
  ) THEN
    RAISE EXCEPTION 'thread event link would create a cycle in thread %', NEW.thread_id
      USING ERRCODE = '23514';
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER thread_event_links_reject_cycle
BEFORE INSERT OR UPDATE ON thread_event_links
FOR EACH ROW EXECUTE FUNCTION substrate_reject_thread_event_link_cycle();

INSERT INTO schema_migrations (version)
VALUES (6)
ON CONFLICT (version) DO NOTHING;

COMMIT;
