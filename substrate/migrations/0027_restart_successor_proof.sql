-- 0027: One current keeper-minted proof for the process incarnation under test.
--
-- The restart_verify room capability is shared by every process in the room, so
-- it cannot distinguish the predecessor from a keeper-launched successor. The
-- keeper receives a fresh proof from each successful relaunching transition and
-- passes it only to that attempt's child. PostgreSQL keeps only the sha256.
--
-- This table stays separate from restart.intents because migration 0026 freezes
-- and re-checks that relation's exact column contract. One row per intent is the
-- current attempt; an UPSERT rotates it, and verified/failed transitions delete
-- it in the same transaction that settles the intent.

BEGIN;

DO $$
DECLARE
    expected_columns TEXT[] := ARRAY[
        'intent_id:uuid:required',
        'claim_epoch:bigint:required',
        'relaunch_attempt:integer:required',
        'proof_hash:text:required',
        'minted_at:timestamp with time zone:required'
    ];
    actual_columns TEXT[];
BEGIN
    IF to_regclass('restart.successor_proofs') IS NOT NULL THEN
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
        WHERE a.attrelid = 'restart.successor_proofs'::regclass
          AND a.attnum > 0
          AND NOT a.attisdropped;

        IF actual_columns IS DISTINCT FROM expected_columns THEN
            RAISE EXCEPTION
                'restart.successor_proofs exists with a different contract; expected %, found %',
                expected_columns,
                actual_columns;
        END IF;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS restart.successor_proofs (
    intent_id UUID PRIMARY KEY
        REFERENCES restart.intents(intent_id) ON DELETE CASCADE,
    claim_epoch BIGINT NOT NULL,
    relaunch_attempt INTEGER NOT NULL,
    proof_hash TEXT NOT NULL,
    minted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT restart_successor_proofs_claim_epoch_check
        CHECK (claim_epoch > 0),
    CONSTRAINT restart_successor_proofs_attempt_check
        CHECK (relaunch_attempt > 0),
    CONSTRAINT restart_successor_proofs_hash_check
        CHECK (proof_hash ~ '^[0-9a-f]{64}$')
);

COMMIT;
