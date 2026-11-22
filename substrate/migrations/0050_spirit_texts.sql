-- 0050: Each spirit keeps every revision of its texts. The highest revision
-- for one room, spirit, and kind is the current text.

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    IF to_regclass('spirit_texts') IS NULL THEN
        RETURN;
    END IF;

    expected_columns := ARRAY[
        'room:text:required', 'spirit:text:required', 'kind:text:required',
        'revision:integer:required', 'body:text:required', 'sha256:text:required',
        'written_by:text:required', 'created_at:timestamp with time zone:required'
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
     WHERE a.attrelid = 'spirit_texts'::regclass
       AND a.attnum > 0
       AND NOT a.attisdropped;

    IF actual_columns IS DISTINCT FROM expected_columns THEN
        RAISE EXCEPTION
            'spirit_texts exists with a different contract; expected %, found %',
            expected_columns, actual_columns;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS spirit_texts (
    room       TEXT        NOT NULL,
    spirit     TEXT        NOT NULL,
    kind       TEXT        NOT NULL,
    revision   INTEGER     NOT NULL,
    body       TEXT        NOT NULL,
    sha256     TEXT        NOT NULL,
    written_by TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT spirit_texts_pkey PRIMARY KEY (room, spirit, kind, revision),
    CONSTRAINT spirit_texts_room_check CHECK (room ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    CONSTRAINT spirit_texts_spirit_check CHECK (length(btrim(spirit)) > 0 AND spirit = btrim(spirit)),
    CONSTRAINT spirit_texts_kind_check CHECK (kind IN ('identity', 'presence', 'spirit')),
    CONSTRAINT spirit_texts_revision_check CHECK (revision >= 1),
    CONSTRAINT spirit_texts_body_check CHECK (length(body) > 0),
    CONSTRAINT spirit_texts_sha256_check CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT spirit_texts_written_by_check CHECK (length(btrim(written_by)) > 0)
);

COMMIT;
