-- 0036: The live process instance that asked for a restart.
--
-- requester_session is the logical harness session (the conversation), which a
-- resumed successor shares and which two processes in one workspace never
-- disambiguate. The instance row names the exact process incarnation, so a
-- keeper or Host acts only on its own child's intent and never on a sibling's.
-- Absent row: a legacy request that names no instance; nothing destructive may
-- target it by inference.
--
-- Separate from restart.intents because migration 0026 freezes and re-checks
-- that relation's exact column contract (same reason as 0027).

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[] := ARRAY[
        'intent_id:uuid:required',
        'requester_instance:text:required',
        'created_at:timestamp with time zone:required'
    ];
    actual_columns TEXT[];
BEGIN
    IF to_regclass('restart.intent_instances') IS NOT NULL THEN
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
        WHERE a.attrelid = 'restart.intent_instances'::regclass
          AND a.attnum > 0
          AND NOT a.attisdropped;

        IF actual_columns IS DISTINCT FROM expected_columns THEN
            RAISE EXCEPTION
                'restart.intent_instances exists with a different contract; expected %, found %',
                expected_columns,
                actual_columns;
        END IF;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS restart.intent_instances (
    intent_id UUID PRIMARY KEY
        REFERENCES restart.intents(intent_id) ON DELETE CASCADE,
    requester_instance TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT restart_intent_instances_bounded_check
        CHECK (length(requester_instance) BETWEEN 1 AND 256)
);

INSERT INTO schema_migrations (version) VALUES (36) ON CONFLICT (version) DO NOTHING;

COMMIT;
