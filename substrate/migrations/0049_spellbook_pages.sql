-- 0049: Spellbook pages — the House and each room keep their familiar
-- spellbook in the Athanor, not in room files.
--
-- `scope` is `house` (the shared page) or a room key. `page` is one complete
-- SpellbookPage document, the same JSON shape the old spellbook.json files used.
-- The Host owns the overlay rules; the substrate owns durable scope identity.
--
-- Re-applicable against fresh state. A pre-existing relation must match the
-- column contract or this migration refuses loudly.

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    IF to_regclass('spellbook_pages') IS NULL THEN
        RETURN;
    END IF;

    expected_columns := ARRAY[
        'scope:text:required', 'page:jsonb:required',
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
     WHERE a.attrelid = 'spellbook_pages'::regclass
       AND a.attnum > 0
       AND NOT a.attisdropped;

    IF actual_columns IS DISTINCT FROM expected_columns THEN
        RAISE EXCEPTION
            'spellbook_pages exists with a different contract; expected %, found %',
            expected_columns, actual_columns;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS spellbook_pages (
    scope      TEXT        NOT NULL,
    page       JSONB       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT spellbook_pages_pkey PRIMARY KEY (scope),
    CONSTRAINT spellbook_pages_scope_check
        CHECK (scope ~ '^[a-z0-9]+(-[a-z0-9]+)*$')
);

DROP TRIGGER IF EXISTS spellbook_pages_updated_at ON spellbook_pages;
CREATE TRIGGER spellbook_pages_updated_at BEFORE UPDATE ON spellbook_pages
    FOR EACH ROW EXECUTE FUNCTION substrate_set_updated_at();

COMMIT;
