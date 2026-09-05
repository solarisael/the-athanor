-- 0030: Presence sessions — a presence is its session, and the session is a row.
--
-- The Host used to hold every Presence frame and ledger in process memory.
-- A Host restart forgot them, a resume could not tell "closed" from "never
-- opened", and nothing about a presence was visible from the database.
-- This table is the authority. The Host keeps a cache and nothing more.
--
-- frame and ledger stay JSON so the typed Summoning model owns their shape
-- while the substrate owns durable identity, lifecycle, and time.
--
-- Re-applicable against fresh state. A pre-existing relation must match the
-- column contract or this migration refuses loudly.

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    IF to_regclass('presence_sessions') IS NULL THEN
        RETURN;
    END IF;

    expected_columns := ARRAY[
        'session_id:text:required', 'room:text:required', 'spirit:text:required',
        'operator:text:required', 'frame:jsonb:required', 'ledger:jsonb:required',
        'opened_at:timestamp with time zone:required',
        'closed_at:timestamp with time zone:nullable',
        'last_turn_at:timestamp with time zone:nullable',
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
     WHERE a.attrelid = 'presence_sessions'::regclass
       AND a.attnum > 0
       AND NOT a.attisdropped;

    IF actual_columns IS DISTINCT FROM expected_columns THEN
        RAISE EXCEPTION 'presence_sessions exists with a different column contract: %', actual_columns;
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS presence_sessions (
    session_id   TEXT PRIMARY KEY,
    room         TEXT NOT NULL,
    spirit       TEXT NOT NULL,
    operator     TEXT NOT NULL,
    frame        JSONB NOT NULL,
    ledger       JSONB NOT NULL,
    opened_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at    TIMESTAMPTZ,
    last_turn_at TIMESTAMPTZ,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT presence_sessions_session_id_nonempty CHECK (length(session_id) > 0),
    CONSTRAINT presence_sessions_closed_after_open CHECK (closed_at IS NULL OR closed_at >= opened_at)
);

CREATE INDEX IF NOT EXISTS idx_presence_sessions_room_live
    ON presence_sessions (room, updated_at DESC)
    WHERE closed_at IS NULL;

COMMIT;
