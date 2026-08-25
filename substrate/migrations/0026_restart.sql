-- 0026: Restart intent v1 — the leased state machine behind one self-restart.
--
-- Law carried by this schema, from the frozen wire contract (v1, 2026-08-25)
-- as amended by Kodo on the same day (the keeper only acts after omp exits, so
-- it cannot claim before the exit):
-- 1. requested -> exiting -> claimed -> relaunching -> verified. The adapter
--    arms the exit tokenless from 'requested'; the keeper claims from
--    'exiting' or, after a crash exit that never armed, from 'requested'.
--    A failure lands on 'failed' and names the stage it failed in. An
--    unclaimed request past its TTL becomes 'expired' and never fires.
--    An exiting row is therefore routinely unclaimed: claim_epoch 0 with
--    every claim column NULL, which restart_intents_claim_check allows.
-- 2. The Docket fences are mirrored, never reused: one claim per epoch, the
--    claim token stored only as a sha256 hash, an idempotency key that refuses
--    replay, and an append-only event ledger enforced by a trigger.
-- 3. Stages here are second-scale machine verification, not the Docket's
--    15-minute human lease with room capabilities and independent review.
-- 4. The keeper is a first-class principal with its own capability row. It is
--    never a room impersonating a keeper.
--
-- Where the two named claim fences live: this plane folds the Docket's separate
-- attempt row into the intent row, so UNIQUE (intent_id, claim_epoch) and
-- UNIQUE (intent_id, idempotency_key) sit on the ledger, where they still bite
-- (a second claim of one epoch and a replayed key both fail the insert).
-- On the intent itself those pairs would be degenerate: intent_id is its key.
--
-- enough: no structural fence limits a workspace to one live intent chain. The
-- storm guard bounds the hour and the status read reports the newest live
-- intent; a partial unique index over live states is the upgrade path.
--
-- Re-applicable against fresh state. A pre-existing restart relation must match
-- the column contract below before creation statements proceed; a mismatched
-- existing relation is refused loudly.

BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE SCHEMA IF NOT EXISTS restart;

DO $$
DECLARE
    relation_name    TEXT;
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
        'intents', 'intent_events', 'principal_capabilities'
    ] LOOP
        CONTINUE WHEN to_regclass(format('restart.%I', relation_name)) IS NULL;

        CASE relation_name
        WHEN 'intents' THEN
            expected_columns := ARRAY[
                'intent_id:uuid:required', 'harness:text:required',
                'workspace:text:required', 'mode:text:required',
                'session_id:text:nullable', 'reason:text:required',
                'consent_source:text:required', 'requester_room:text:required',
                'requester_spirit:text:required', 'requester_session:text:required',
                'idempotency_key:text:required', 'state:text:required',
                'failed_stage:text:nullable', 'claim_epoch:bigint:required',
                'claimant:text:nullable', 'claim_idempotency_key:text:nullable',
                'lease_token_hash:text:nullable',
                'claimed_at:timestamp with time zone:nullable',
                'expires_at:timestamp with time zone:required',
                'exiting_deadline_at:timestamp with time zone:nullable',
                'relaunching_deadline_at:timestamp with time zone:nullable',
                'relaunch_attempts:integer:required',
                'successor_session:text:nullable', 'successor_room:text:nullable',
                'successor_spirit:text:nullable',
                'verified_at:timestamp with time zone:nullable',
                'created_at:timestamp with time zone:required',
                'updated_at:timestamp with time zone:required'
            ];
        WHEN 'intent_events' THEN
            expected_columns := ARRAY[
                'event_id:uuid:required', 'intent_id:uuid:required',
                'claim_epoch:bigint:nullable', 'event_kind:text:required',
                'principal:text:required', 'detail:jsonb:nullable',
                'idempotency_key:text:nullable',
                'created_at:timestamp with time zone:required'
            ];
        WHEN 'principal_capabilities' THEN
            expected_columns := ARRAY[
                'principal:text:required', 'operation_class:text:required',
                'capability_hash:text:required',
                'created_at:timestamp with time zone:required',
                'rotated_at:timestamp with time zone:nullable'
            ];
        END CASE;

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
         WHERE a.attrelid = format('restart.%I', relation_name)::regclass
           AND a.attnum > 0
           AND NOT a.attisdropped;

        IF actual_columns IS DISTINCT FROM expected_columns THEN
            RAISE EXCEPTION
                'restart.% exists with a different contract; expected %, found %',
                relation_name, expected_columns, actual_columns;
        END IF;
    END LOOP;
END $$;

-- ---------------------------------------------------------------------------
-- intents — one requested restart. The claim columns are the folded attempt:
-- they are all NULL before a claim and all present after it, and the token
-- lives here only as a hash.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS restart.intents (
    intent_id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    harness                 TEXT        NOT NULL,
    workspace               TEXT        NOT NULL,
    mode                    TEXT        NOT NULL,
    session_id              TEXT,
    reason                  TEXT        NOT NULL,
    consent_source          TEXT        NOT NULL,
    requester_room          TEXT        NOT NULL,
    requester_spirit        TEXT        NOT NULL,
    requester_session       TEXT        NOT NULL,
    idempotency_key         TEXT        NOT NULL,
    state                   TEXT        NOT NULL DEFAULT 'requested',
    failed_stage            TEXT,
    claim_epoch             BIGINT      NOT NULL DEFAULT 0,
    claimant                TEXT,
    claim_idempotency_key   TEXT,
    lease_token_hash        TEXT,
    claimed_at              TIMESTAMPTZ,
    expires_at              TIMESTAMPTZ NOT NULL,
    exiting_deadline_at     TIMESTAMPTZ,
    relaunching_deadline_at TIMESTAMPTZ,
    relaunch_attempts       INTEGER     NOT NULL DEFAULT 0,
    successor_session       TEXT,
    successor_room          TEXT,
    successor_spirit        TEXT,
    verified_at             TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A duplicate request key returns the existing intent instead of minting a
    -- twin, so the key is unique inside the workspace that scopes the restart.
    CONSTRAINT restart_intents_idempotency_key
        UNIQUE (workspace, idempotency_key),
    CONSTRAINT restart_intents_harness_check
        CHECK (harness IN ('omp')),
    CONSTRAINT restart_intents_mode_check
        CHECK (mode IN ('resume', 'fresh')),
    CONSTRAINT restart_intents_consent_check
        CHECK (consent_source IN ('operator-standing-policy', 'operator-approval')),
    CONSTRAINT restart_intents_state_check
        CHECK (state IN (
            'requested', 'claimed', 'exiting', 'relaunching',
            'verified', 'failed', 'expired'
        )),
    -- failed:<stage> is one fact in two columns: a failed row names its stage
    -- and no other row may carry one.
    CONSTRAINT restart_intents_failed_stage_check
        CHECK (
            (state = 'failed') = (failed_stage IS NOT NULL)
            AND (
                failed_stage IS NULL
                OR failed_stage IN ('requested', 'claimed', 'exiting', 'relaunching')
            )
        ),
    CONSTRAINT restart_intents_lease_hash_check
        CHECK (lease_token_hash IS NULL OR lease_token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT restart_intents_claim_check
        CHECK (
            (
                claim_epoch = 0
                AND claimant IS NULL
                AND claim_idempotency_key IS NULL
                AND lease_token_hash IS NULL
                AND claimed_at IS NULL
            )
            OR (
                claim_epoch >= 1
                AND claimant IS NOT NULL
                AND claim_idempotency_key IS NOT NULL
                AND lease_token_hash IS NOT NULL
                AND claimed_at IS NOT NULL
            )
        ),
    -- One verify per intent: the verified state and its successor triple are
    -- written together, once, and no other state carries them.
    CONSTRAINT restart_intents_verify_check
        CHECK (
            (state = 'verified') = (verified_at IS NOT NULL)
            AND (
                verified_at IS NULL
                OR (
                    successor_session IS NOT NULL
                    AND successor_room IS NOT NULL
                    AND successor_spirit IS NOT NULL
                )
            )
        ),
    CONSTRAINT restart_intents_relaunch_attempts_check
        CHECK (relaunch_attempts >= 0)
);

CREATE INDEX IF NOT EXISTS idx_restart_intents_workspace
    ON restart.intents (workspace, state, created_at DESC);
-- The lazy expiry sweep reads only unclaimed requests against their TTL.
CREATE INDEX IF NOT EXISTS idx_restart_intents_expiry
    ON restart.intents (expires_at)
    WHERE state = 'requested';

-- One live intent per workspace, structural. The keeper reads the newest live
-- intent for a workspace and acts on it (omp-keeper decide.rs), so a second
-- live row lets a newer request stand in for an unverified successor. This index
-- makes that twin unconstructible; restart_request refuses it by name
-- (intent_pending) before it ever reaches here.
CREATE UNIQUE INDEX IF NOT EXISTS idx_restart_intents_one_live_per_workspace
    ON restart.intents (workspace)
    WHERE state IN ('requested', 'exiting', 'claimed', 'relaunching');

-- ---------------------------------------------------------------------------
-- intent_events — append-only ledger. Every transition writes one row, and
-- the two named claim fences live here (see the header).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS restart.intent_events (
    event_id        UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    intent_id       UUID        NOT NULL REFERENCES restart.intents (intent_id),
    claim_epoch     BIGINT,
    event_kind      TEXT        NOT NULL,
    principal       TEXT        NOT NULL,
    detail          JSONB,
    idempotency_key TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT restart_intent_events_kind_check
        CHECK (event_kind IN (
            'requested', 'claimed', 'exiting', 'relaunching',
            'verified', 'failed', 'expired'
        )),
    CONSTRAINT restart_intent_events_epoch_check
        CHECK (claim_epoch IS NULL OR claim_epoch >= 0)
);

-- UNIQUE (intent_id, claim_epoch): one claim per epoch, ever.
CREATE UNIQUE INDEX IF NOT EXISTS idx_restart_intent_events_claim_epoch
    ON restart.intent_events (intent_id, claim_epoch)
    WHERE event_kind = 'claimed';
-- UNIQUE (intent_id, idempotency_key): a replayed key never doubles the ledger.
CREATE UNIQUE INDEX IF NOT EXISTS idx_restart_intent_events_idempotency
    ON restart.intent_events (intent_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_restart_intent_events_intent
    ON restart.intent_events (intent_id, created_at DESC);
-- The storm guard counts intents that reached one stage inside one hour.
CREATE INDEX IF NOT EXISTS idx_restart_intent_events_kind_time
    ON restart.intent_events (event_kind, created_at DESC);

-- Structural, not advisory: the ledger accepts INSERT only.
CREATE OR REPLACE FUNCTION restart.refuse_event_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'restart.intent_events is append-only; % refused', TG_OP;
END $$;

DROP TRIGGER IF EXISTS restart_intent_events_append_only ON restart.intent_events;
CREATE TRIGGER restart_intent_events_append_only
    BEFORE UPDATE OR DELETE ON restart.intent_events
    FOR EACH ROW EXECUTE FUNCTION restart.refuse_event_mutation();

-- ---------------------------------------------------------------------------
-- principal_capabilities — every authority this plane recognizes. The column
-- is named 'principal', not 'room', because the keeper owns the terminal and
-- impersonates no room; the check is the room-key shape so one slug law gates
-- both (config.rs ROOM_KEY_RE). Provisioning is operator-side and offline,
-- exactly like substrate/provision-room-capability.ps1: the ritual mints a
-- secret, writes only its sha256 here, and places the secret in the holder's
-- runtime file. The secret rides no task packet and no tool grant.
--
-- Four classes, because the intent id is public: restart_status hands it out
-- with no capability, so the room proves itself to ask (restart_request), to
-- arm the exit (restart_exit), and to sign the successor (restart_verify),
-- while the keeper proves itself to claim (restart_claim).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS restart.principal_capabilities (
    principal       TEXT        NOT NULL,
    operation_class TEXT        NOT NULL,
    capability_hash TEXT        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rotated_at      TIMESTAMPTZ,

    CONSTRAINT restart_principal_capabilities_pkey
        PRIMARY KEY (principal, operation_class),
    CONSTRAINT restart_principal_capabilities_class_check
        CHECK (operation_class IN (
            'restart_request', 'restart_exit', 'restart_verify', 'restart_claim'
        )),
    CONSTRAINT restart_principal_capabilities_principal_check
        CHECK (principal ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    CONSTRAINT restart_principal_capabilities_hash_check
        CHECK (capability_hash ~ '^[0-9a-f]{64}$')
);

-- The class list widened after the first apply of this file (Kintsu's
-- authority verdict), and 0026 is not released yet, so it is amended in place.
-- A database that already has the one-class table converges here instead of
-- refusing every new row.
ALTER TABLE restart.principal_capabilities
    DROP CONSTRAINT IF EXISTS restart_principal_capabilities_class_check;
ALTER TABLE restart.principal_capabilities
    ADD CONSTRAINT restart_principal_capabilities_class_check
        CHECK (operation_class IN (
            'restart_request', 'restart_exit', 'restart_verify', 'restart_claim'
        ));

-- ---------------------------------------------------------------------------
-- Residual-free verification: after this migration, exactly these relations
-- exist in the restart schema. Run to verify; an empty result is a pass.
--
--   SELECT c.relname
--     FROM pg_class c
--     JOIN pg_namespace n ON n.oid = c.relnamespace
--    WHERE n.nspname = 'restart' AND c.relkind = 'r'
--      AND c.relname NOT IN (
--          'intents', 'intent_events', 'principal_capabilities'
--      );
-- ---------------------------------------------------------------------------

COMMIT;
