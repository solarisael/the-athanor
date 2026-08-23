-- 0026: Per-room settings — operator tuning belongs to each room, not the binary.
--
-- Each value stays JSON so the typed Akasha door owns interpretation while the
-- substrate owns durable room/key identity. Missing rows mean today's defaults.
--
-- Re-applicable against fresh state. A pre-existing relation must match the
-- column contract or this migration refuses loudly.

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    IF to_regclass('room_settings') IS NULL THEN
        RETURN;
    END IF;

    expected_columns := ARRAY[
        'room_key:text:required', 'key:text:required', 'value:jsonb:required',
        'created_at:timestamp with time zone:required',
        'updated_at:timestamp with time zone:required'
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
     WHERE a.attrelid = 'room_settings'::regclass
       AND a.attnum > 0
       AND NOT a.attisdropped;

    IF actual_columns IS DISTINCT FROM expected_columns THEN
        RAISE EXCEPTION
            'room_settings exists with a different contract; expected %, found %',
            expected_columns, actual_columns;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS room_settings (
    room_key   TEXT        NOT NULL,
    key        TEXT        NOT NULL,
    value      JSONB       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT room_settings_pkey PRIMARY KEY (room_key, key),
    CONSTRAINT room_settings_room_key_check
        CHECK (room_key ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    CONSTRAINT room_settings_key_check
        CHECK (key ~ '^[a-z][a-z0-9_]*$')
);

DROP TRIGGER IF EXISTS room_settings_updated_at ON room_settings;
CREATE TRIGGER room_settings_updated_at BEFORE UPDATE ON room_settings
    FOR EACH ROW EXECUTE FUNCTION substrate_set_updated_at();

COMMIT;
