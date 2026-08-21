-- 0023: Docket v1 — the House's cooperation plane: goals, quests, claims,
-- attempts, acceptance, events, receipts.
--
-- Law carried by this schema, from candidate #3765 as amended by
-- guild-hall #144/#145/#146:
--   * A post is a DRAFT/OFFER. Activation is the explicit transition that
--     freezes intent authority, acceptance policy, review class, importance,
--     deadline, and recurrence. Observation mints nothing.
--   * Claims are spirit-level consent receipts. The claimant principal is a
--     (room, spirit) pair by construction; a familiar has no arm here.
--     Familiar hands stay visible on receipts as performed_by.
--   * Docket offers, never assigns. declined/refused/blocked are distinct
--     states and never enter failure counters.
--   * Executor confidence never self-settles an acceptance item.
--   * Reclaim is fenced by quest id, attempt id, claim epoch, lease expiry,
--     revision, and idempotency key. A stale worker cannot publish.
--   * quest_events is append-only. The ledger records; it never judges.
--   * Recurrence re-arms on completion under the frozen activation grant.
--     REFUSED opens a steward replanning door, never a silent re-offer.
--
-- Re-applicable against fresh state. A pre-existing docket relation must
-- match the column contract below or this migration refuses loudly.

BEGIN;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE SCHEMA IF NOT EXISTS docket;

-- CREATE TABLE IF NOT EXISTS heals an absent relation. Any relation that
-- already exists must match the complete column contract before creation
-- statements proceed; a mismatched existing relation is refused loudly.
-- enough: column contract only; add constraint-name arrays like 0022 when a
-- second independent applier of this schema exists.
DO $$
DECLARE
    relation_name    TEXT;
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
        'goals', 'quests', 'quest_dependencies', 'quest_attempts',
        'quest_acceptance_items', 'quest_events', 'quest_receipts'
    ] LOOP
        CONTINUE WHEN to_regclass(format('docket.%I', relation_name)) IS NULL;

        CASE relation_name
            WHEN 'goals' THEN
                expected_columns := ARRAY[
                    'goal_id:uuid:required', 'house_id:text:required',
                    'title:text:required', 'intent:text:required',
                    'intent_authority_principal:text:nullable',
                    'steward_room:text:nullable', 'steward_spirit:text:nullable',
                    'acceptance_policy:jsonb:nullable',
                    'failure_policy:jsonb:nullable',
                    'escalation_budget:integer:required',
                    'escalation_used:integer:required',
                    'priority:integer:required',
                    'recurrence_interval:interval:nullable',
                    'status:text:required', 'revision:bigint:required',
                    'created_at:timestamp with time zone:required',
                    'activated_at:timestamp with time zone:nullable',
                    'retired_at:timestamp with time zone:nullable',
                    'updated_at:timestamp with time zone:required',
                    'idempotency_key:text:nullable'
                ];
            WHEN 'quests' THEN
                expected_columns := ARRAY[
                    'quest_id:uuid:required', 'house_id:text:required',
                    'goal_id:uuid:nullable', 'parent_quest_id:uuid:nullable',
                    'kind:text:required', 'title:text:required',
                    'body:text:required',
                    'authority_ceiling:text:nullable',
                    'required_capabilities:text[]:required',
                    'acceptance_policy:jsonb:nullable',
                    'acceptance_policy_digest:text:nullable',
                    'review_class:text:nullable',
                    'settlement_policy:jsonb:nullable',
                    'importance:text:required',
                    'deadline_at:timestamp with time zone:nullable',
                    'intent_authority_principal:text:nullable',
                    'posted_by_room:text:required',
                    'posted_by_spirit:text:required',
                    'state:text:required', 'revision:bigint:required',
                    'claim_epoch:bigint:required',
                    'cancel_requested:boolean:required',
                    'cancel_requested_by:text:nullable',
                    'created_at:timestamp with time zone:required',
                    'activated_at:timestamp with time zone:nullable',
                    'settled_at:timestamp with time zone:nullable',
                    'updated_at:timestamp with time zone:required'
                ];
            WHEN 'quest_dependencies' THEN
                expected_columns := ARRAY[
                    'quest_id:uuid:required',
                    'depends_on_quest_id:uuid:required',
                    'created_at:timestamp with time zone:required'
                ];
            WHEN 'quest_attempts' THEN
                expected_columns := ARRAY[
                    'attempt_id:uuid:required', 'quest_id:uuid:required',
                    'claim_epoch:bigint:required',
                    'quest_revision:bigint:required',
                    'claimant_room:text:required',
                    'claimant_spirit:text:required',
                    'session_id:text:required', 'runtime:text:nullable',
                    'lease_token_hash:text:required',
                    'lease_expires_at:timestamp with time zone:required',
                    'idempotency_key:text:required',
                    'state:text:required',
                    'started_at:timestamp with time zone:required',
                    'heartbeat_at:timestamp with time zone:nullable',
                    'ended_at:timestamp with time zone:nullable'
                ];
            WHEN 'quest_acceptance_items' THEN
                expected_columns := ARRAY[
                    'item_id:uuid:required', 'quest_id:uuid:required',
                    'position:integer:required', 'criterion:text:required',
                    'verdict:text:required',
                    'settled_by_role:text:nullable',
                    'settled_by_room:text:nullable',
                    'settled_by_spirit:text:nullable',
                    'settled_at:timestamp with time zone:nullable'
                ];
            WHEN 'quest_events' THEN
                expected_columns := ARRAY[
                    'event_id:uuid:required', 'quest_id:uuid:nullable',
                    'goal_id:uuid:nullable',
                    'attempt_id:uuid:nullable', 'event_kind:text:required',
                    'principal:text:required', 'detail:jsonb:nullable',
                    'idempotency_key:text:nullable',
                    'created_at:timestamp with time zone:required'
                ];
            WHEN 'quest_receipts' THEN
                expected_columns := ARRAY[
                    'receipt_id:uuid:required', 'quest_id:uuid:required',
                    'attempt_id:uuid:nullable', 'kind:text:required',
                    'body:text:required', 'digest:text:required',
                    'submitted_by_room:text:required',
                    'submitted_by_spirit:text:required',
                    'performed_by:text:nullable',
                    'authored_role:text:required',
                    'idempotency_key:text:nullable',
                    'created_at:timestamp with time zone:required'
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
         WHERE a.attrelid = format('docket.%I', relation_name)::regclass
           AND a.attnum > 0
           AND NOT a.attisdropped;

        IF actual_columns IS DISTINCT FROM expected_columns THEN
            RAISE EXCEPTION
                'docket.% exists with a different contract; expected %, found %',
                relation_name, expected_columns, actual_columns;
        END IF;
    END LOOP;
END $$;

-- ---------------------------------------------------------------------------
-- goals — standing intent. Activation freezes authority; recurrence lives in
-- the frozen tuple and re-arms deadline work under the same grant.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.goals (
    goal_id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    house_id                   TEXT        NOT NULL,
    title                      TEXT        NOT NULL,
    intent                     TEXT        NOT NULL,
    intent_authority_principal TEXT,
    steward_room               TEXT,
    steward_spirit             TEXT,
    acceptance_policy          JSONB,
    failure_policy             JSONB,
    escalation_budget          INTEGER     NOT NULL DEFAULT 0,
    escalation_used            INTEGER     NOT NULL DEFAULT 0,
    priority                   INTEGER     NOT NULL DEFAULT 0,
    recurrence_interval        INTERVAL,
    status                     TEXT        NOT NULL DEFAULT 'draft',
    revision                   BIGINT      NOT NULL DEFAULT 0,
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    activated_at               TIMESTAMPTZ,
    retired_at                 TIMESTAMPTZ,
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    idempotency_key            TEXT,

    CONSTRAINT docket_goals_status_check
        CHECK (status IN ('draft', 'offered', 'active', 'blocked', 'retired', 'cancelled')),
    -- Activation freezes authority: an active goal names its authority and steward.
    CONSTRAINT docket_goals_activation_freeze_check
        CHECK (
            status IN ('draft', 'offered')
            OR (
                intent_authority_principal IS NOT NULL
                AND steward_room IS NOT NULL
                AND steward_spirit IS NOT NULL
                AND activated_at IS NOT NULL
            )
        ),
    CONSTRAINT docket_goals_escalation_check
        CHECK (escalation_budget >= 0 AND escalation_used >= 0),
    CONSTRAINT docket_goals_revision_check
        CHECK (revision >= 0)
);

CREATE INDEX IF NOT EXISTS idx_docket_goals_board
    ON docket.goals (house_id, status, priority DESC);
-- goalDraft replay: one goal per (house, mint key); replay returns the
-- existing goal instead of minting a twin.
CREATE UNIQUE INDEX IF NOT EXISTS idx_docket_goals_idempotency
    ON docket.goals (house_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- ---------------------------------------------------------------------------
-- quests — the unit of delegated work. DRAFT/OFFER until activation freezes
-- the tuple. Docket offers; a claim is a separate consent receipt.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.quests (
    quest_id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    house_id                   TEXT        NOT NULL,
    goal_id                    UUID        REFERENCES docket.goals (goal_id),
    parent_quest_id            UUID        REFERENCES docket.quests (quest_id),
    kind                       TEXT        NOT NULL,
    title                      TEXT        NOT NULL,
    body                       TEXT        NOT NULL,
    authority_ceiling          TEXT,
    required_capabilities      TEXT[]      NOT NULL DEFAULT '{}',
    acceptance_policy          JSONB,
    acceptance_policy_digest   TEXT,
    review_class               TEXT,
    settlement_policy          JSONB,
    importance                 TEXT        NOT NULL DEFAULT 'hint',
    deadline_at                TIMESTAMPTZ,
    intent_authority_principal TEXT,
    posted_by_room             TEXT        NOT NULL,
    posted_by_spirit           TEXT        NOT NULL,
    state                      TEXT        NOT NULL DEFAULT 'draft',
    revision                   BIGINT      NOT NULL DEFAULT 0,
    claim_epoch                BIGINT      NOT NULL DEFAULT 0,
    cancel_requested           BOOLEAN     NOT NULL DEFAULT FALSE,
    cancel_requested_by        TEXT,
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    activated_at               TIMESTAMPTZ,
    settled_at                 TIMESTAMPTZ,
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT docket_quests_state_check
        CHECK (state IN (
            'draft', 'offered', 'claimed', 'submitted', 'settled',
            'refused', 'blocked', 'quarantined', 'cancelled'
        )),
    CONSTRAINT docket_quests_importance_check
        CHECK (importance IN ('hint', 'blocker')),
    CONSTRAINT docket_quests_review_class_check
        CHECK (review_class IN ('R0', 'R1', 'R2', 'R3')),
    -- Activation freezes the tuple: past draft, the quest carries its
    -- authority, acceptance snapshot + digest, and review class.
    CONSTRAINT docket_quests_activation_freeze_check
        CHECK (
            state = 'draft'
            OR (
                intent_authority_principal IS NOT NULL
                AND acceptance_policy IS NOT NULL
                AND acceptance_policy_digest IS NOT NULL
                AND review_class IS NOT NULL
                AND activated_at IS NOT NULL
            )
        ),
    CONSTRAINT docket_quests_digest_shape_check
        CHECK (
            acceptance_policy_digest IS NULL
            OR acceptance_policy_digest ~ '^[0-9a-f]{64}$'
        ),
    CONSTRAINT docket_quests_epoch_check
        CHECK (claim_epoch >= 0 AND revision >= 0)
);

CREATE INDEX IF NOT EXISTS idx_docket_quests_board
    ON docket.quests (house_id, state, deadline_at ASC NULLS LAST);
CREATE INDEX IF NOT EXISTS idx_docket_quests_goal
    ON docket.quests (goal_id);

-- ---------------------------------------------------------------------------
-- quest_dependencies — acyclic wiring between quests.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.quest_dependencies (
    quest_id            UUID        NOT NULL REFERENCES docket.quests (quest_id),
    depends_on_quest_id UUID        NOT NULL REFERENCES docket.quests (quest_id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT docket_quest_dependencies_pkey
        PRIMARY KEY (quest_id, depends_on_quest_id),
    CONSTRAINT docket_quest_dependencies_self_check
        CHECK (quest_id <> depends_on_quest_id)
);

-- ---------------------------------------------------------------------------
-- quest_attempts — one claim consent, one lease. The claimant is a
-- (room, spirit) principal by construction; there is no familiar arm.
-- Reclaim fencing: (quest_id, claim_epoch) is unique, the lease token is
-- stored only as a hash, and the idempotency key refuses replay.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.quest_attempts (
    attempt_id       UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    quest_id         UUID        NOT NULL REFERENCES docket.quests (quest_id),
    claim_epoch      BIGINT      NOT NULL,
    quest_revision   BIGINT      NOT NULL,
    claimant_room    TEXT        NOT NULL,
    claimant_spirit  TEXT        NOT NULL,
    session_id       TEXT        NOT NULL,
    runtime          TEXT,
    lease_token_hash TEXT        NOT NULL,
    lease_expires_at TIMESTAMPTZ NOT NULL,
    idempotency_key  TEXT        NOT NULL,
    state            TEXT        NOT NULL DEFAULT 'active',
    started_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    heartbeat_at     TIMESTAMPTZ,
    ended_at         TIMESTAMPTZ,

    CONSTRAINT docket_quest_attempts_epoch_key
        UNIQUE (quest_id, claim_epoch),
    CONSTRAINT docket_quest_attempts_idempotency_key
        UNIQUE (quest_id, idempotency_key),
    CONSTRAINT docket_quest_attempts_state_check
        CHECK (state IN ('active', 'yielded', 'finished', 'failed', 'abandoned', 'reclaimed')),
    CONSTRAINT docket_quest_attempts_lease_hash_check
        CHECK (lease_token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT docket_quest_attempts_epoch_bounds_check
        CHECK (claim_epoch >= 1 AND quest_revision >= 0),
    CONSTRAINT docket_quest_attempts_ended_check
        CHECK (ended_at IS NULL OR ended_at >= started_at)
);

CREATE INDEX IF NOT EXISTS idx_docket_quest_attempts_quest
    ON docket.quest_attempts (quest_id, claim_epoch DESC);
CREATE INDEX IF NOT EXISTS idx_docket_quest_attempts_session
    ON docket.quest_attempts (session_id);

-- ---------------------------------------------------------------------------
-- quest_acceptance_items — the named criteria. Verdicts are fail-closed and
-- an executor can never settle its own item: the fence is structural.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.quest_acceptance_items (
    item_id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    quest_id          UUID        NOT NULL REFERENCES docket.quests (quest_id),
    position          INTEGER     NOT NULL,
    criterion         TEXT        NOT NULL,
    verdict           TEXT        NOT NULL DEFAULT 'pending',
    settled_by_role   TEXT,
    settled_by_room   TEXT,
    settled_by_spirit TEXT,
    settled_at        TIMESTAMPTZ,

    CONSTRAINT docket_quest_acceptance_items_position_key
        UNIQUE (quest_id, position),
    CONSTRAINT docket_quest_acceptance_items_verdict_check
        CHECK (verdict IN (
            'pending', 'met', 'not_met', 'unknown',
            'inconclusive', 'not_applicable', 'refused'
        )),
    CONSTRAINT docket_quest_acceptance_items_role_check
        CHECK (settled_by_role IN ('reviewer', 'steward', 'operator')),
    -- Executor confidence never self-settles: a settled verdict names a
    -- non-executor role and principal, or the verdict stays pending.
    CONSTRAINT docket_quest_acceptance_items_settlement_check
        CHECK (
            verdict = 'pending'
            OR (
                settled_by_role IS NOT NULL
                AND settled_by_room IS NOT NULL
                AND settled_by_spirit IS NOT NULL
                AND settled_at IS NOT NULL
            )
        ),
    CONSTRAINT docket_quest_acceptance_items_position_check
        CHECK (position >= 1)
);

-- ---------------------------------------------------------------------------
-- quest_events — append-only ledger for quests AND goals: every transition
-- writes a row, goal transitions included. The clock is a legitimate
-- principal here; ping receipts attribute to the clock, never to a spirit's
-- silence.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.quest_events (
    event_id        UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    quest_id        UUID        REFERENCES docket.quests (quest_id),
    goal_id         UUID        REFERENCES docket.goals (goal_id),
    attempt_id      UUID        REFERENCES docket.quest_attempts (attempt_id),
    event_kind      TEXT        NOT NULL,
    principal       TEXT        NOT NULL,
    detail          JSONB,
    idempotency_key TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT docket_quest_events_scope_check
        CHECK (quest_id IS NOT NULL OR goal_id IS NOT NULL)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_docket_quest_events_idempotency
    ON docket.quest_events (quest_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL AND quest_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_docket_goal_events_idempotency
    ON docket.quest_events (goal_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL AND goal_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_docket_quest_events_quest
    ON docket.quest_events (quest_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_docket_quest_events_goal
    ON docket.quest_events (goal_id, created_at DESC);

-- Structural, not advisory: the ledger accepts INSERT only.
CREATE OR REPLACE FUNCTION docket.refuse_event_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'docket.quest_events is append-only; % refused', TG_OP;
END $$;

DROP TRIGGER IF EXISTS docket_quest_events_append_only ON docket.quest_events;
CREATE TRIGGER docket_quest_events_append_only
    BEFORE UPDATE OR DELETE ON docket.quest_events
    FOR EACH ROW EXECUTE FUNCTION docket.refuse_event_mutation();

-- ---------------------------------------------------------------------------
-- quest_receipts — evidence. submitted_by is the accountable spirit;
-- performed_by keeps a familiar's hand visible instead of laundering it.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS docket.quest_receipts (
    receipt_id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    quest_id           UUID        NOT NULL REFERENCES docket.quests (quest_id),
    attempt_id         UUID        REFERENCES docket.quest_attempts (attempt_id),
    kind               TEXT        NOT NULL,
    body               TEXT        NOT NULL,
    digest             TEXT        NOT NULL,
    submitted_by_room  TEXT        NOT NULL,
    submitted_by_spirit TEXT       NOT NULL,
    performed_by       TEXT,
    authored_role      TEXT        NOT NULL,
    idempotency_key    TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT docket_quest_receipts_role_check
        CHECK (authored_role IN ('executor', 'reviewer')),
    CONSTRAINT docket_quest_receipts_digest_check
        CHECK (digest ~ '^[0-9a-f]{64}$')
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_docket_quest_receipts_idempotency
    ON docket.quest_receipts (quest_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_docket_quest_receipts_quest
    ON docket.quest_receipts (quest_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Residual-free verification: after this migration, exactly these relations
-- exist in the docket schema. Run to verify; an empty result is a pass.
--
--   SELECT c.relname
--     FROM pg_class c
--     JOIN pg_namespace n ON n.oid = c.relnamespace
--    WHERE n.nspname = 'docket' AND c.relkind = 'r'
--      AND c.relname NOT IN (
--          'goals', 'quests', 'quest_dependencies', 'quest_attempts',
--          'quest_acceptance_items', 'quest_events', 'quest_receipts'
--      );
-- ---------------------------------------------------------------------------

COMMIT;
