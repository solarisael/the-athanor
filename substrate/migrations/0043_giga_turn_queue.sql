BEGIN;

-- A captured turn waits for its conversation batch while waiting_since is set;
-- NULL keeps every other event claimable exactly as before.
ALTER TABLE giga_events
  ADD COLUMN IF NOT EXISTS waiting_since TIMESTAMPTZ;
ALTER TABLE giga_events DROP CONSTRAINT IF EXISTS giga_events_waiting_pending_check;
ALTER TABLE giga_events ADD CONSTRAINT giga_events_waiting_pending_check
  CHECK (waiting_since IS NULL OR queue_state = 'pending');
CREATE INDEX IF NOT EXISTS giga_events_waiting_turns_idx
  ON giga_events (room, session_id, waiting_since)
  WHERE waiting_since IS NOT NULL;

-- One row per turn identity (room + session + source). event_id names the one
-- event that owns its classification. Deleting that event (queue purge) leaves
-- a retired tombstone that still answers repeated capture as a duplicate.
CREATE TABLE IF NOT EXISTS giga_turn_identities (
  room TEXT NOT NULL CHECK (room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$'),
  session_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  conflict BOOLEAN NOT NULL DEFAULT FALSE,
  source_role TEXT CHECK (source_role IN ('user', 'assistant')),
  content_hash TEXT CHECK (content_hash ~ '^[0-9a-f]{64}$'),
  scope_project TEXT,
  event_id TEXT REFERENCES giga_events(event_id) ON DELETE SET NULL,
  state TEXT GENERATED ALWAYS AS (
    CASE WHEN conflict THEN 'conflict' WHEN event_id IS NULL THEN 'retired' ELSE 'active' END
  ) STORED,
  origin TEXT NOT NULL CHECK (origin IN ('capture', 'legacy')),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (room, session_id, source_id),
  CHECK (
    (conflict AND source_role IS NULL AND content_hash IS NULL AND scope_project IS NULL
      AND event_id IS NULL)
    OR (NOT conflict AND source_role IS NOT NULL AND content_hash IS NOT NULL)
  )
);
CREATE INDEX IF NOT EXISTS giga_turn_identities_event_idx
  ON giga_turn_identities (event_id)
  WHERE event_id IS NOT NULL;

-- A legacy window that never succeeded is archived here: its turns now belong
-- to their identities, it is never claimed or replayed, and its attempts stay.
CREATE TABLE IF NOT EXISTS giga_window_replacements (
  window_event_id TEXT PRIMARY KEY REFERENCES giga_events(event_id) ON DELETE CASCADE,
  previous_state TEXT NOT NULL CHECK (previous_state IN ('pending', 'running', 'failed')),
  replaced_at TIMESTAMPTZ NOT NULL
);

-- Legacy evidence: every turn source of a conversation window whose sources are
-- all user/assistant turns. turn_event_id is the canonical job id that
-- giga_conversation_ingest derives: sha256("giga_turn_v1" NUL room NUL session
-- NUL source), lower hex.
CREATE TEMP TABLE giga_0043_turns ON COMMIT DROP AS
SELECT e.event_id, e.queue_state, e.room, e.session_id, e.processed_at,
       e.created_at AS event_created_at, s.source_id, s.source_role, s.content_hash,
       s.scope_project, s.created_at AS turn_at,
       encode(sha256(
         convert_to('giga_turn_v1', 'UTF8') || decode('00', 'hex')
         || convert_to(e.room, 'UTF8') || decode('00', 'hex')
         || convert_to(e.session_id, 'UTF8') || decode('00', 'hex')
         || convert_to(s.source_id, 'UTF8')
       ), 'hex') AS turn_event_id
FROM giga_events e
JOIN giga_event_sources s ON s.event_id = e.event_id
WHERE e.event_type = 'conversation_window'
  AND e.room IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM giga_event_sources other
    WHERE other.event_id = e.event_id
      AND (other.source_type <> 'turn'
           OR other.source_role IS NULL
           OR other.source_role NOT IN ('user', 'assistant'))
  );
-- Canonical turn jobs are not legacy windows.
DELETE FROM giga_0043_turns WHERE event_id = turn_event_id;

-- Per identity: evidence that disagrees on hash, role, or project is a
-- conflict; otherwise the agreed evidence and where its windows stand.
CREATE TEMP TABLE giga_0043_identities ON COMMIT DROP AS
SELECT room, session_id, source_id,
       MIN(turn_event_id) AS turn_event_id,
       COUNT(DISTINCT content_hash) > 1
         OR COUNT(DISTINCT source_role) > 1
         OR COUNT(DISTINCT scope_project) > 1
         OR COUNT(scope_project) NOT IN (0, COUNT(*)) AS conflict,
       bool_or(queue_state = 'succeeded') AS processed,
       bool_or(queue_state IN ('pending', 'running')) AS unfinished,
       MIN(content_hash) AS content_hash,
       MIN(source_role) AS source_role,
       MIN(scope_project) AS scope_project,
       MIN(turn_at) AS turn_at
FROM giga_0043_turns
GROUP BY room, session_id, source_id;

-- Agreed turns that no window classified become canonical single-turn jobs:
-- pending when an unfinished window still carried them, otherwise parked as
-- failed until an operator replays them through giga_event_replay.
INSERT INTO giga_events
  (event_schema_version, event_id, event_type, room, session_id, project_keys, lifecycle,
   created_at, queue_state, last_error, processed_at, available_at, updated_at)
SELECT 1, i.turn_event_id, 'conversation_window', i.room, i.session_id,
       CASE WHEN i.scope_project IS NULL THEN '{}'::TEXT[] ELSE ARRAY[i.scope_project] END,
       '{}'::JSONB, i.turn_at,
       CASE WHEN i.unfinished THEN 'pending' ELSE 'failed' END,
       CASE WHEN i.unfinished THEN NULL ELSE 'GigaLegacyWindowFailed' END,
       CASE WHEN i.unfinished THEN NULL ELSE NOW() END,
       NOW(), NOW()
FROM giga_0043_identities i
WHERE NOT i.conflict AND NOT i.processed
  AND NOT EXISTS (
    SELECT 1 FROM giga_turn_identities known
    WHERE known.room = i.room AND known.session_id = i.session_id
      AND known.source_id = i.source_id
  )
ON CONFLICT (event_id) DO NOTHING;

INSERT INTO giga_event_sources
  (event_id, source_ordinal, source_type, source_id, source_role, content_hash, scope_room,
   scope_project, scope_visibility, publication_review_required, created_at)
SELECT i.turn_event_id, 0, 'turn', i.source_id, i.source_role, i.content_hash, i.room,
       i.scope_project, 'private', TRUE, i.turn_at
FROM giga_0043_identities i
WHERE NOT i.conflict AND NOT i.processed
  AND NOT EXISTS (
    SELECT 1 FROM giga_turn_identities known
    WHERE known.room = i.room AND known.session_id = i.session_id
      AND known.source_id = i.source_id
  )
ON CONFLICT DO NOTHING;

-- A classified turn stays owned by its earliest succeeded window, so it is
-- never classified again; a conflict gets no owner and refuses capture.
INSERT INTO giga_turn_identities
  (room, session_id, source_id, conflict, source_role, content_hash, scope_project, event_id,
   origin, recorded_at)
SELECT i.room, i.session_id, i.source_id, i.conflict,
       CASE WHEN i.conflict THEN NULL ELSE i.source_role END,
       CASE WHEN i.conflict THEN NULL ELSE i.content_hash END,
       CASE WHEN i.conflict THEN NULL ELSE i.scope_project END,
       CASE WHEN i.conflict THEN NULL
            WHEN i.processed THEN (
              SELECT t.event_id FROM giga_0043_turns t
              WHERE t.room = i.room AND t.session_id = i.session_id
                AND t.source_id = i.source_id AND t.queue_state = 'succeeded'
              ORDER BY t.processed_at NULLS LAST, t.event_created_at, t.event_id
              LIMIT 1)
            ELSE i.turn_event_id END,
       'legacy', NOW()
FROM giga_0043_identities i
ON CONFLICT (room, session_id, source_id) DO NOTHING;

-- Archive every legacy window that never succeeded. Failed windows keep their
-- state and error; unfinished windows close as failed, never as succeeded.
INSERT INTO giga_window_replacements (window_event_id, previous_state, replaced_at)
SELECT DISTINCT event_id, queue_state, NOW()
FROM giga_0043_turns
WHERE queue_state IN ('pending', 'running', 'failed')
ON CONFLICT (window_event_id) DO NOTHING;

UPDATE giga_event_attempts a
SET outcome = 'failed', error_class = 'GigaWindowReplacedByTurnJobs', finished_at = NOW()
FROM giga_window_replacements r
JOIN giga_events e ON e.event_id = r.window_event_id
WHERE a.event_id = r.window_event_id
  AND a.finished_at IS NULL
  AND e.queue_state IN ('pending', 'running');

UPDATE giga_events e
SET queue_state = 'failed', last_error = 'GigaWindowReplacedByTurnJobs',
    locked_by = NULL, locked_at = NULL, lease_expires_at = NULL,
    processed_at = NOW(), last_finished_at = NOW(), updated_at = NOW()
FROM giga_window_replacements r
WHERE r.window_event_id = e.event_id
  AND e.queue_state IN ('pending', 'running');

COMMIT;
