BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- The boat.ready trio becomes the first lane of the general Crane delivery
-- system. Every step below is data preserving: tables, indexes and constraints
-- are renamed or widened in place so existing outbox rows, receipts and dead
-- letters keep their identities, and the migration is safe to apply against a
-- fresh, partially migrated, or already migrated database.

-- Every lookup below is pinned to the schema this migration is being applied to, so
-- a same-named relation inherited through search_path is never renamed or dropped.
DO $$
DECLARE
  here TEXT := current_schema();
  rename RECORD;
BEGIN
  FOR rename IN
    SELECT * FROM (VALUES
      ('boat_ready_outbox', 'crane_outbox'),
      ('boat_ready_receipts', 'crane_receipts'),
      ('boat_ready_dead_letters', 'crane_dead_letters')
    ) AS pair(legacy, current)
  LOOP
    IF to_regclass(format('%I.%I', here, rename.legacy)) IS NOT NULL
      AND to_regclass(format('%I.%I', here, rename.current)) IS NULL
    THEN
      EXECUTE format('ALTER TABLE %I.%I RENAME TO %I', here, rename.legacy, rename.current);
    END IF;
  END LOOP;
END $$;

-- A replayed 0016 recreates the empty boat trio beside the renamed tables. Retire
-- those leftovers only when they are provably empty; a populated leftover is a
-- split brain this migration must never resolve by guessing.
DO $$
DECLARE
  here TEXT := current_schema();
  leftover TEXT;
  rows_present BOOLEAN;
BEGIN
  FOREACH leftover IN ARRAY ARRAY[
    'boat_ready_outbox', 'boat_ready_receipts', 'boat_ready_dead_letters'
  ] LOOP
    CONTINUE WHEN to_regclass(format('%I.%I', here, leftover)) IS NULL;
    EXECUTE format('SELECT EXISTS (SELECT 1 FROM %I.%I)', here, leftover) INTO rows_present;
    IF rows_present THEN
      RAISE EXCEPTION
        'legacy table %.% still holds rows beside its crane_* replacement; reconcile the two by hand before applying 0017',
        here, leftover;
    END IF;
    EXECUTE format('DROP TABLE %I.%I', here, leftover);
  END LOOP;
END $$;

ALTER INDEX IF EXISTS boat_ready_outbox_claim_idx RENAME TO crane_outbox_claim_idx;
ALTER INDEX IF EXISTS boat_ready_outbox_lease_idx RENAME TO crane_outbox_lease_idx;
ALTER INDEX IF EXISTS boat_ready_outbox_aggregate_idx RENAME TO crane_outbox_aggregate_idx;
ALTER INDEX IF EXISTS boat_ready_receipts_aggregate_idx RENAME TO crane_receipts_aggregate_idx;
ALTER INDEX IF EXISTS boat_ready_dead_letters_observed_idx RENAME TO crane_dead_letters_observed_idx;

-- Constraint identities follow their tables so no boat_ready_* name survives on
-- the widened road.
DO $$
DECLARE
  legacy RECORD;
BEGIN
  FOR legacy IN
    SELECT conrelid::regclass AS relation, conname
    FROM pg_constraint
    WHERE conname LIKE 'boat\_ready\_%'
      AND conrelid IN (
        to_regclass(format('%I.crane_outbox', current_schema())),
        to_regclass(format('%I.crane_receipts', current_schema())),
        to_regclass(format('%I.crane_dead_letters', current_schema()))
      )
  LOOP
    EXECUTE format(
      'ALTER TABLE %s RENAME CONSTRAINT %I TO %I',
      legacy.relation,
      legacy.conname,
      'crane_' || substring(legacy.conname from 12)
    );
  END LOOP;
END $$;

ALTER TABLE crane_outbox ADD COLUMN IF NOT EXISTS crease_pattern TEXT;
ALTER TABLE crane_outbox ADD COLUMN IF NOT EXISTS recipient_kind TEXT;
ALTER TABLE crane_outbox ADD COLUMN IF NOT EXISTS recipient_key TEXT;
ALTER TABLE crane_outbox ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;
ALTER TABLE crane_outbox ADD COLUMN IF NOT EXISTS parent_intent_id UUID;
ALTER TABLE crane_outbox ADD COLUMN IF NOT EXISTS correlation_id UUID;

-- The boat-shaped constraints of 0016 are unnamed, so they are identified by
-- their definitions. Constraints this migration owns are excluded and rebuilt
-- explicitly below.
DO $$
DECLARE
  boat_shaped RECORD;
BEGIN
  FOR boat_shaped IN
    SELECT conrelid::regclass AS relation, conname
    FROM pg_constraint
    WHERE contype = 'c'
      AND conrelid IN (
        to_regclass(format('%I.crane_outbox', current_schema())),
        to_regclass(format('%I.crane_receipts', current_schema())),
        to_regclass(format('%I.crane_dead_letters', current_schema()))
      )
      AND conname NOT IN (
        'crane_outbox_boat_lane_key_check',
        'crane_outbox_boat_lane_payload_check',
        'crane_outbox_envelope_keys_check',
        'crane_outbox_recipient_shape_check',
        'crane_outbox_recipient_payload_check',
        'crane_outbox_optional_payload_check',
        'crane_dead_letters_reason_code_check'
      )
      AND (
        pg_get_constraintdef(oid) LIKE '%event_kind = ''boat.ready''%'
        OR pg_get_constraintdef(oid) LIKE '%aggregate_kind = ''memory''%'
        OR pg_get_constraintdef(oid) LIKE '%''boat.ready:memory:''%'
        OR pg_get_constraintdef(oid) LIKE '%payload ?& ARRAY%'
        OR pg_get_constraintdef(oid) LIKE '%payload - ARRAY%'
        OR pg_get_constraintdef(oid) LIKE '%''malformed_payload''%'
      )
  LOOP
    EXECUTE format(
      'ALTER TABLE %s DROP CONSTRAINT %I',
      boat_shaped.relation,
      boat_shaped.conname
    );
  END LOOP;
END $$;

-- boat.ready keeps its exact idempotency key and its exact seven-key envelope;
-- every other lane is only held to the generalized envelope.
ALTER TABLE crane_outbox DROP CONSTRAINT IF EXISTS crane_outbox_boat_lane_key_check;
ALTER TABLE crane_outbox ADD CONSTRAINT crane_outbox_boat_lane_key_check CHECK (
  event_kind <> 'boat.ready'
  OR (
    aggregate_kind = 'memory'
    AND idempotency_key = 'boat.ready:memory:' || aggregate_id::text
  )
);

ALTER TABLE crane_outbox DROP CONSTRAINT IF EXISTS crane_outbox_boat_lane_payload_check;
ALTER TABLE crane_outbox ADD CONSTRAINT crane_outbox_boat_lane_payload_check CHECK (
  event_kind <> 'boat.ready'
  OR (payload - ARRAY[
    'created_at', 'event_id', 'event_kind', 'integrity_sha256',
    'record_id', 'room', 'schema_version'
  ]) = '{}'::jsonb
);

ALTER TABLE crane_outbox DROP CONSTRAINT IF EXISTS crane_outbox_envelope_keys_check;
ALTER TABLE crane_outbox ADD CONSTRAINT crane_outbox_envelope_keys_check CHECK (
  payload ?& ARRAY[
    'created_at', 'event_id', 'event_kind', 'integrity_sha256',
    'record_id', 'room', 'schema_version'
  ]
  AND (payload - ARRAY[
    'created_at', 'event_id', 'event_kind', 'integrity_sha256',
    'record_id', 'room', 'schema_version',
    'crease_pattern', 'recipient_kind', 'recipient_key',
    'expires_at', 'parent_intent_id', 'correlation_id'
  ]) = '{}'::jsonb
);

-- Only boat.ready predates addressing, so every other lane must name its recipient.
-- An unroutable row can then never reach the relay.
ALTER TABLE crane_outbox DROP CONSTRAINT IF EXISTS crane_outbox_recipient_shape_check;
ALTER TABLE crane_outbox ADD CONSTRAINT crane_outbox_recipient_shape_check CHECK (
  (recipient_kind IS NULL) = (recipient_key IS NULL)
  AND (event_kind = 'boat.ready') = (recipient_kind IS NULL)
  AND (recipient_kind IS NULL OR recipient_kind IN ('worker', 'familiar', 'room', 'reviewer'))
  AND (recipient_key IS NULL OR recipient_key ~ '^[a-z0-9][a-z0-9_-]{0,63}$')
);

ALTER TABLE crane_outbox DROP CONSTRAINT IF EXISTS crane_outbox_recipient_payload_check;
ALTER TABLE crane_outbox ADD CONSTRAINT crane_outbox_recipient_payload_check CHECK (
  (payload ->> 'recipient_kind') IS NOT DISTINCT FROM recipient_kind
  AND (payload ->> 'recipient_key') IS NOT DISTINCT FROM recipient_key
);

ALTER TABLE crane_outbox DROP CONSTRAINT IF EXISTS crane_outbox_optional_payload_check;
ALTER TABLE crane_outbox ADD CONSTRAINT crane_outbox_optional_payload_check CHECK (
  (payload ? 'expires_at') = (expires_at IS NOT NULL)
  AND (payload ? 'parent_intent_id') = (parent_intent_id IS NOT NULL)
  AND (payload ? 'correlation_id') = (correlation_id IS NOT NULL)
);

UPDATE crane_outbox
SET crease_pattern = 'boat.ready.v1'
WHERE crease_pattern IS NULL
  AND event_kind = 'boat.ready';

ALTER TABLE crane_dead_letters DROP CONSTRAINT IF EXISTS crane_dead_letters_reason_code_check;
ALTER TABLE crane_dead_letters ADD CONSTRAINT crane_dead_letters_reason_code_check CHECK (
  reason_code IN (
    'malformed_payload', 'private_payload', 'unknown_event', 'integrity_mismatch',
    'record_mismatch', 'receipt_conflict', 'delivery_exhausted', 'publish_exhausted',
    'expired', 'recipient_mismatch'
  )
);

-- A publisher refusal that happens before the row could be routed to a lane has no
-- subject to record, so the observed subject becomes optional.
ALTER TABLE crane_dead_letters ALTER COLUMN subject DROP NOT NULL;

-- The paper-boat trigger installed by 0016 keeps producing byte-identical
-- boat.ready events: same generated event id, same idempotency key, same
-- seven-key payload. Only the target table name and the new lane descriptor move.
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

  INSERT INTO crane_outbox (
    event_id,
    idempotency_key,
    event_kind,
    aggregate_kind,
    aggregate_id,
    room,
    integrity_sha256,
    crease_pattern,
    payload
  ) VALUES (
    queued_event_id,
    'boat.ready:memory:' || NEW.id::text,
    'boat.ready',
    'memory',
    NEW.id,
    NEW.room,
    body_digest,
    'boat.ready.v1',
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
      jsonb_set(crane_outbox.payload, '{room}', to_jsonb(EXCLUDED.room), false),
      '{integrity_sha256}', to_jsonb(EXCLUDED.integrity_sha256), false
    ),
    updated_at = NOW()
  WHERE crane_outbox.aggregate_id = EXCLUDED.aggregate_id
    AND crane_outbox.event_kind = EXCLUDED.event_kind;

  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS memories_enqueue_boat_ready ON memories;
CREATE TRIGGER memories_enqueue_boat_ready
  AFTER INSERT ON memories
  FOR EACH ROW
  WHEN (NEW.type = 'paper-boat')
  EXECUTE FUNCTION substrate_enqueue_boat_ready();

INSERT INTO schema_migrations (version) VALUES (17)
ON CONFLICT (version) DO NOTHING;

COMMIT;
