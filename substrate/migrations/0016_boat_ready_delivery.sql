BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS boat_ready_outbox (
  event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  idempotency_key TEXT NOT NULL UNIQUE,
  event_kind TEXT NOT NULL CHECK (event_kind = 'boat.ready'),
  aggregate_kind TEXT NOT NULL CHECK (aggregate_kind = 'memory'),
  aggregate_id BIGINT NOT NULL REFERENCES memories(id) ON DELETE RESTRICT,
  room TEXT NOT NULL,
  payload JSONB NOT NULL,
  integrity_sha256 TEXT NOT NULL CHECK (integrity_sha256 ~ '^[0-9a-f]{64}$'),
  state TEXT NOT NULL DEFAULT 'pending'
    CHECK (state IN ('pending', 'leased', 'published', 'dead_letter')),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  lease_owner UUID,
  lease_expires_at TIMESTAMPTZ,
  published_at TIMESTAMPTZ,
  dead_lettered_at TIMESTAMPTZ,
  last_error TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (idempotency_key = 'boat.ready:memory:' || aggregate_id::text),
  CHECK (
    (state = 'leased' AND lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL)
    OR (state <> 'leased' AND lease_owner IS NULL AND lease_expires_at IS NULL)
  ),
  CHECK ((state = 'published') = (published_at IS NOT NULL)),
  CHECK ((state = 'dead_letter') = (dead_lettered_at IS NOT NULL)),
  CHECK (jsonb_typeof(payload) = 'object'),
  CHECK (payload ->> 'event_id' = event_id::text),
  CHECK (payload ->> 'event_kind' = event_kind),
  CHECK (payload ->> 'record_id' = aggregate_id::text),
  CHECK (payload ->> 'room' = room),
  CHECK (payload ->> 'integrity_sha256' = integrity_sha256),
  CHECK (payload -> 'schema_version' = '1'::jsonb),
  CHECK (jsonb_typeof(payload -> 'created_at') = 'string'),
  CHECK (payload ?& ARRAY[
    'created_at', 'event_id', 'event_kind', 'integrity_sha256',
    'record_id', 'room', 'schema_version'
  ]),
  CHECK ((payload - ARRAY[
    'created_at', 'event_id', 'event_kind', 'integrity_sha256',
    'record_id', 'room', 'schema_version'
  ]) = '{}'::jsonb)
);

CREATE INDEX IF NOT EXISTS boat_ready_outbox_claim_idx
  ON boat_ready_outbox (available_at, created_at, event_id)
  WHERE state IN ('pending', 'leased');
CREATE INDEX IF NOT EXISTS boat_ready_outbox_lease_idx
  ON boat_ready_outbox (lease_expires_at)
  WHERE state = 'leased';
CREATE INDEX IF NOT EXISTS boat_ready_outbox_aggregate_idx
  ON boat_ready_outbox (aggregate_kind, aggregate_id);

CREATE TABLE IF NOT EXISTS boat_ready_receipts (
  consumer_name TEXT NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind = 'boat.ready'),
  aggregate_id BIGINT NOT NULL REFERENCES memories(id) ON DELETE RESTRICT,
  room TEXT NOT NULL,
  integrity_sha256 TEXT NOT NULL CHECK (integrity_sha256 ~ '^[0-9a-f]{64}$'),
  stream_sequence BIGINT NOT NULL CHECK (stream_sequence > 0),
  first_delivery_count INTEGER NOT NULL CHECK (first_delivery_count > 0),
  processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (consumer_name, event_id)
);

CREATE INDEX IF NOT EXISTS boat_ready_receipts_aggregate_idx
  ON boat_ready_receipts (aggregate_id, processed_at DESC);

CREATE TABLE IF NOT EXISTS boat_ready_dead_letters (
  dead_letter_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID,
  source TEXT NOT NULL CHECK (source IN ('publisher', 'consumer')),
  subject TEXT NOT NULL,
  reason_code TEXT NOT NULL CHECK (reason_code IN (
    'malformed_payload', 'private_payload', 'unknown_event', 'integrity_mismatch',
    'record_mismatch', 'receipt_conflict', 'delivery_exhausted', 'publish_exhausted'
  )),
  reason TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
  payload_bytes INTEGER NOT NULL CHECK (payload_bytes >= 0),
  delivery_count INTEGER CHECK (delivery_count IS NULL OR delivery_count > 0),
  observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE NULLS NOT DISTINCT (source, event_id, payload_sha256, reason_code)
);

CREATE INDEX IF NOT EXISTS boat_ready_dead_letters_observed_idx
  ON boat_ready_dead_letters (observed_at DESC, dead_letter_id);

CREATE OR REPLACE FUNCTION substrate_enqueue_boat_ready() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  queued_event_id UUID := gen_random_uuid();
  body_digest TEXT := encode(digest(convert_to(NEW.body, 'UTF8'), 'sha256'), 'hex');
  event_created_at TIMESTAMPTZ := NEW.created_at;
BEGIN
  IF NEW.type <> 'paper-boat' THEN
    RETURN NEW;
  END IF;

  INSERT INTO boat_ready_outbox (
    event_id,
    idempotency_key,
    event_kind,
    aggregate_kind,
    aggregate_id,
    room,
    integrity_sha256,
    payload
  ) VALUES (
    queued_event_id,
    'boat.ready:memory:' || NEW.id::text,
    'boat.ready',
    'memory',
    NEW.id,
    NEW.room,
    body_digest,
    jsonb_build_object(
      'schema_version', 1,
      'event_id', queued_event_id::text,
      'event_kind', 'boat.ready',
      'record_id', NEW.id::text,
      'room', NEW.room,
      'created_at', to_char(event_created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
      'integrity_sha256', body_digest
    )
  )
  ON CONFLICT (idempotency_key) DO UPDATE SET
    room = EXCLUDED.room,
    integrity_sha256 = EXCLUDED.integrity_sha256,
    payload = jsonb_set(
      jsonb_set(boat_ready_outbox.payload, '{room}', to_jsonb(EXCLUDED.room), false),
      '{integrity_sha256}', to_jsonb(EXCLUDED.integrity_sha256), false
    ),
    updated_at = NOW()
  WHERE boat_ready_outbox.aggregate_id = EXCLUDED.aggregate_id
    AND boat_ready_outbox.event_kind = EXCLUDED.event_kind;

  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS memories_enqueue_boat_ready ON memories;
CREATE TRIGGER memories_enqueue_boat_ready
  AFTER INSERT ON memories
  FOR EACH ROW
  WHEN (NEW.type = 'paper-boat')
  EXECUTE FUNCTION substrate_enqueue_boat_ready();

INSERT INTO schema_migrations (version) VALUES (16)
ON CONFLICT (version) DO NOTHING;

COMMIT;
