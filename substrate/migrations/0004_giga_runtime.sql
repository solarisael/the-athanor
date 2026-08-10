BEGIN;

ALTER TABLE giga_events
  ADD COLUMN IF NOT EXISTS locked_by TEXT,
  ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count BETWEEN 0 AND 5),
  ADD COLUMN IF NOT EXISTS candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (candidate_count BETWEEN 0 AND 1),
  ADD COLUMN IF NOT EXISTS last_finished_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS replay_count INTEGER NOT NULL DEFAULT 0 CHECK (replay_count >= 0);

ALTER TABLE giga_event_sources
  ADD COLUMN IF NOT EXISTS source_role TEXT;
ALTER TABLE giga_candidate_sources
  ADD COLUMN IF NOT EXISTS source_role TEXT;

UPDATE giga_events e
SET queue_state = 'failed',
    last_error = 'source_role_missing_after_0004',
    processed_at = NOW(),
    updated_at = NOW()
WHERE EXISTS (
  SELECT 1 FROM giga_event_sources s
  WHERE s.event_id = e.event_id AND s.source_role IS NULL
);

UPDATE giga_events
SET lifecycle = (lifecycle - 'proof_contract')
  || jsonb_build_object('acceptance', lifecycle->'proof_contract')
WHERE event_type = 'subagent_dispatched'
  AND lifecycle ? 'proof_contract'
  AND NOT (lifecycle ? 'acceptance');

CREATE TABLE IF NOT EXISTS giga_event_attempts (
  event_id TEXT NOT NULL REFERENCES giga_events(event_id) ON DELETE CASCADE,
  replay_count INTEGER NOT NULL DEFAULT 0 CHECK (replay_count >= 0),
  attempt_count INTEGER NOT NULL CHECK (attempt_count BETWEEN 1 AND 5),
  room TEXT NOT NULL,
  worker_id TEXT NOT NULL,
  claimed_at TIMESTAMPTZ NOT NULL,
  lease_expires_at TIMESTAMPTZ NOT NULL,
  outcome TEXT CHECK (outcome IN ('succeeded', 'retry', 'failed', 'lease_expired')),
  candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (candidate_count BETWEEN 0 AND 1),
  error_class TEXT,
  available_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  PRIMARY KEY (event_id, replay_count, attempt_count),
  CHECK (lease_expires_at > claimed_at),
  CHECK (finished_at IS NULL OR outcome IS NOT NULL),
  CHECK (room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$')
);

CREATE TABLE IF NOT EXISTS giga_event_replays (
  event_id TEXT NOT NULL REFERENCES giga_events(event_id) ON DELETE CASCADE,
  replay_count INTEGER NOT NULL CHECK (replay_count > 0),
  room TEXT NOT NULL,
  operator_identity TEXT NOT NULL,
  authorization_basis TEXT NOT NULL,
  previous_state TEXT NOT NULL CHECK (previous_state = 'failed'),
  replayed_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (event_id, replay_count),
  UNIQUE (event_id, operator_identity, authorization_basis, replayed_at),
  CHECK (room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$')
);

CREATE INDEX IF NOT EXISTS giga_events_room_claim_idx
  ON giga_events (room, available_at, created_at, event_id)
  WHERE queue_state = 'pending';
CREATE INDEX IF NOT EXISTS giga_events_room_expired_lease_idx
  ON giga_events (room, lease_expires_at, created_at, event_id)
  WHERE queue_state = 'running';
CREATE INDEX IF NOT EXISTS giga_event_attempts_diagnostics_idx
  ON giga_event_attempts (room, event_id, attempt_count DESC);

ALTER TABLE giga_reviews
  ADD COLUMN IF NOT EXISTS operator_identity TEXT,
  ADD COLUMN IF NOT EXISTS promotion_request_digest TEXT,
  ADD COLUMN IF NOT EXISTS publication_consent JSONB,
  ADD COLUMN IF NOT EXISTS committed_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS giga_reviews_one_promotion_idx
  ON giga_reviews (candidate_id)
  WHERE action = 'promote';
CREATE UNIQUE INDEX IF NOT EXISTS giga_reviews_promotion_digest_idx
  ON giga_reviews (promotion_request_digest)
  WHERE promotion_request_digest IS NOT NULL;

INSERT INTO schema_migrations (version)
VALUES (4)
ON CONFLICT (version) DO NOTHING;

COMMIT;
