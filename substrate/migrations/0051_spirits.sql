-- 0051: The spirit record. Each room of the House has one row: the room key,
-- the spirit name, and the operator. An active room has no retired_at.
--
-- Each insert, update, or delete of a row sends NOTIFY athanor_spirits with
-- the room key, so that a running Host can open, rebuild, or close the room.
-- A rename sends the old key and the new key.
--
-- Re-applicable against fresh state. A pre-existing relation must match the
-- column contract or this migration refuses loudly.

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    IF to_regclass('spirits') IS NULL THEN
        RETURN;
    END IF;

    expected_columns := ARRAY[
        'room:text:required', 'spirit:text:required', 'operator:text:required',
        'summoned_by:text:required',
        'summoned_at:timestamp with time zone:required',
        'retired_at:timestamp with time zone:nullable'
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
     WHERE a.attrelid = 'spirits'::regclass
       AND a.attnum > 0
       AND NOT a.attisdropped;

    IF actual_columns IS DISTINCT FROM expected_columns THEN
        RAISE EXCEPTION
            'spirits exists with a different contract; expected %, found %',
            expected_columns, actual_columns;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS spirits (
    room        TEXT        NOT NULL,
    spirit      TEXT        NOT NULL,
    operator    TEXT        NOT NULL,
    summoned_by TEXT        NOT NULL,
    summoned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    retired_at  TIMESTAMPTZ NULL,

    CONSTRAINT spirits_pkey PRIMARY KEY (room),
    CONSTRAINT spirits_room_check
        CHECK (room ~ '^[a-z0-9]+(-[a-z0-9]+)*$' AND room <> 'house'),
    CONSTRAINT spirits_spirit_check
        CHECK (length(btrim(spirit)) > 0 AND spirit = btrim(spirit)),
    -- The same key rule as operators.key and hearth::OperatorKey.
    CONSTRAINT spirits_operator_check
        CHECK (operator ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    CONSTRAINT spirits_summoned_by_check CHECK (length(btrim(summoned_by)) > 0)
);

CREATE OR REPLACE FUNCTION substrate_notify_spirits()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        PERFORM pg_notify('athanor_spirits', OLD.room);
    END IF;
    IF TG_OP <> 'DELETE' THEN
        PERFORM pg_notify('athanor_spirits', NEW.room);
    END IF;
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS spirits_notify ON spirits;
CREATE TRIGGER spirits_notify
    AFTER INSERT OR UPDATE OR DELETE ON spirits
    FOR EACH ROW EXECUTE FUNCTION substrate_notify_spirits();

COMMIT;
