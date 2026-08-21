-- 0024: Docket room capability — operation-scoped write authority.
--
-- Law: caller-supplied identity text mints nothing. A Docket write operation
-- must present a room capability secret; the substrate stores only the
-- sha256 of that secret and compares in constant time. Provisioning is
-- operator-side and offline: the deploy ritual mints secrets, writes hashes
-- here, and places secrets in each room's runtime env. Workers never hold
-- the secret; it rides no task packet and no tool grant.
--
-- enough: room-level capability gating the 'docket_write' class; spirit-level
-- binding and gating of hallway/lesson writes are later doors, opened when
-- Docket proves the pattern.
--
-- Re-applicable against fresh state. A pre-existing relation must match the
-- column contract or this migration refuses loudly.

BEGIN;

CREATE SCHEMA IF NOT EXISTS docket;

DO $$
DECLARE
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    IF to_regclass('docket.room_capabilities') IS NULL THEN
        RETURN;
    END IF;

    expected_columns := ARRAY[
        'room:text:required', 'operation_class:text:required',
        'capability_hash:text:required',
        'created_at:timestamp with time zone:required',
        'rotated_at:timestamp with time zone:nullable'
    ];

    SELECT array_agg(
               format(
                   '%s:%s:%s',
                   a.attname,
                   format_type(a.atttypid, a.atttypmod),
                   CASE WHEN a.attnotnull THEN 'required' ELSE 'nullable' END
               )
               ORDER BY a.attnum
           )
      INTO actual_columns
      FROM pg_attribute a
     WHERE a.attrelid = 'docket.room_capabilities'::regclass
       AND a.attnum > 0
       AND NOT a.attisdropped;

    IF actual_columns IS DISTINCT FROM expected_columns THEN
        RAISE EXCEPTION
            'docket.room_capabilities exists with a different contract; expected %, found %',
            expected_columns, actual_columns;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS docket.room_capabilities (
    room            TEXT        NOT NULL,
    operation_class TEXT        NOT NULL,
    capability_hash TEXT        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_at      TIMESTAMPTZ,

    CONSTRAINT docket_room_capabilities_pkey
        PRIMARY KEY (room, operation_class),
    CONSTRAINT docket_room_capabilities_class_check
        CHECK (operation_class IN ('docket_write')),
    CONSTRAINT docket_room_capabilities_hash_check
        CHECK (capability_hash ~ '^[0-9a-f]{64}$')
);

COMMIT;
