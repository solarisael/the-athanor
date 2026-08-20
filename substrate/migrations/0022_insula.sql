-- 0022: Insula v1 — the House's global observation core.
--
-- Insula stores observations, not work. A row in `insula.log` is one thing a
-- writer saw, once. It is never mutated into a finished span, never carries a
-- prompt, body, payload or content field, and never instructs anybody: Docket
-- alone owns lease reclaim and permitted retries.
--
-- The relation is deliberately body free. There is no JSON attribute bag and no
-- prose-shaped column, so a leak cannot be introduced by a later writer "just
-- adding a field". A guard at the end of this migration fails loudly if such a
-- column ever appears.
--
-- Re-applicable against fresh, partially-applied and fully-applied state.

BEGIN;

CREATE SCHEMA IF NOT EXISTS insula;

-- ---------------------------------------------------------------------------
-- Raw events
-- ---------------------------------------------------------------------------
-- Identity and causality live in explicit typed columns:
--   * event_id            globally stable identity of this observation
--   * span_id / trace_id  what was observed, and the walk it belongs to
--   * parent_span_id      causality; nullable because a root span has no parent
--   * writer_id + writer_sequence  transport order, monotonic per writer
--
-- Ordering is writer_sequence and parent links. `observed_at` is display and
-- windowing only: wall clocks skew, sequences do not.
--
-- house_id / room / spirit / session_id are stamped by the host from the
-- trusted binding. They are never accepted from the event payload.
--
-- quest_id / attempt_id do not exist in source yet. They are nullable imports
-- with zero fencing meaning: nothing in Insula may gate on them.
CREATE TABLE IF NOT EXISTS insula.log (
    schema_version      SMALLINT      NOT NULL DEFAULT 1,
    event_id            UUID          PRIMARY KEY,
    span_id             UUID          NOT NULL,
    trace_id            UUID          NOT NULL,
    parent_span_id      UUID,
    writer_id           UUID          NOT NULL,
    writer_sequence     BIGINT        NOT NULL,
    house_id            TEXT          NOT NULL,
    room                TEXT          NOT NULL,
    spirit              TEXT          NOT NULL,
    session_id          TEXT          NOT NULL,
    component           TEXT          NOT NULL,
    layer               TEXT          NOT NULL,
    operation           TEXT          NOT NULL,
    phase               TEXT          NOT NULL,
    observed_at         TIMESTAMPTZ   NOT NULL,
    duration_us         BIGINT,
    outcome_class       TEXT          NOT NULL,
    error_class         TEXT,
    bytes_in            BIGINT        NOT NULL DEFAULT 0,
    bytes_out           BIGINT        NOT NULL DEFAULT 0,
    tokens_in           BIGINT        NOT NULL DEFAULT 0,
    tokens_out          BIGINT        NOT NULL DEFAULT 0,
    -- Absent correlation is truthful. A writer that had no tool call, provider
    -- request, or natural receipt leaves those fields null; inventing a value
    -- is prohibited.
    tool_call_id        TEXT,
    provider_request_id TEXT,
    quest_id            UUID,
    attempt_id          UUID,
    -- The logical identity of the observation. `idempotency_version` names the
    -- derivation that produced `idempotency_key`, so a future derivation
    -- becomes version 2 beside version 1 instead of colliding with it.
    idempotency_version SMALLINT      NOT NULL DEFAULT 1,
    idempotency_scope   TEXT          NOT NULL,
    idempotency_key     TEXT          NOT NULL,
    receipt_kind        TEXT,
    receipt_id          TEXT,
    semantic_hash       TEXT          NOT NULL,
    -- Writer-side loss, as observed by the writer itself. The collector never
    -- invents a drop count, so a nonzero value only ever rides a drop event.
    drop_count          BIGINT        NOT NULL DEFAULT 0,
    expires_at          TIMESTAMPTZ   NOT NULL,
    -- Collapsed identical re-deliveries of this same logical event.
    duplicate_count     BIGINT        NOT NULL DEFAULT 0,
    last_duplicate_at   TIMESTAMPTZ,
    ingested_at         TIMESTAMPTZ   NOT NULL DEFAULT NOW(),

    CONSTRAINT insula_log_schema_version_check CHECK (schema_version = 1),
    CONSTRAINT insula_log_writer_sequence_check CHECK (writer_sequence > 0),
    CONSTRAINT insula_log_phase_check
        CHECK (phase IN ('start', 'end', 'point', 'drop')),
    CONSTRAINT insula_log_outcome_class_check
        CHECK (outcome_class IN (
            'ok', 'refused', 'error', 'timeout', 'cancelled', 'degraded', 'unknown'
        )),
    -- A started span has not finished, so it cannot carry a duration yet.
    CONSTRAINT insula_log_duration_shape_check
        CHECK (phase <> 'start' OR duration_us IS NULL),
    CONSTRAINT insula_log_duration_bound_check
        CHECK (duration_us IS NULL OR (duration_us >= 0 AND duration_us <= 86400000000)),
    -- error_class is a redacted class name, never a message and never a body.
    CONSTRAINT insula_log_error_class_check
        CHECK (
            error_class IS NULL
            OR (outcome_class <> 'ok' AND error_class ~ '^[a-z0-9][a-z0-9_.:-]{0,63}$')
        ),
    CONSTRAINT insula_log_counts_check
        CHECK (
            bytes_in   BETWEEN 0 AND 1099511627776
            AND bytes_out  BETWEEN 0 AND 1099511627776
            AND tokens_in  BETWEEN 0 AND 1099511627776
            AND tokens_out BETWEEN 0 AND 1099511627776
        ),
    CONSTRAINT insula_log_drop_count_check
        CHECK (drop_count >= 0 AND drop_count <= 1000000000),
    CONSTRAINT insula_log_drop_phase_check
        CHECK (drop_count = 0 OR phase = 'drop'),
    CONSTRAINT insula_log_duplicate_count_check
        CHECK (
            duplicate_count >= 0
            AND (duplicate_count = 0) = (last_duplicate_at IS NULL)
        ),
    CONSTRAINT insula_log_binding_check
        CHECK (
            house_id ~ '^[a-z0-9][a-z0-9_.:-]{0,63}$'
            AND room ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'
            AND octet_length(room) <= 64
            AND octet_length(spirit) BETWEEN 1 AND 64
            AND octet_length(session_id) BETWEEN 1 AND 128
        ),
    CONSTRAINT insula_log_mechanical_names_check
        CHECK (
            component ~ '^[a-z0-9][a-z0-9_.:-]{0,63}$'
            AND layer ~ '^[a-z0-9][a-z0-9_.:-]{0,63}$'
            AND operation ~ '^[a-z0-9][a-z0-9_.:-]{0,63}$'
            AND (receipt_kind IS NULL OR receipt_kind ~ '^[a-z0-9][a-z0-9_.:-]{0,63}$')
        ),
    CONSTRAINT insula_log_correlation_bound_check
        CHECK (
            (tool_call_id IS NULL OR octet_length(tool_call_id) BETWEEN 1 AND 256)
            AND (provider_request_id IS NULL OR octet_length(provider_request_id) BETWEEN 1 AND 256)
            AND (receipt_id IS NULL OR octet_length(receipt_id) BETWEEN 1 AND 256)
        ),
    CONSTRAINT insula_log_semantic_hash_check
        CHECK (semantic_hash ~ '^[0-9a-f]{64}$'),
    -- The v1 key is a derived lowercase sha256 over the scope's canonical
    -- components, never free caller text.
    CONSTRAINT insula_log_idempotency_key_shape_check
        CHECK (idempotency_key ~ '^[0-9a-f]{64}$'),
    CONSTRAINT insula_log_idempotency_version_check
        CHECK (idempotency_version > 0),
    -- A logical duplicate must survive session failover, so a scope naming
    -- only the session can never collapse anything. `session` is refused by
    -- name as well as by the bounded set, because the refusal is the point.
    CONSTRAINT insula_log_idempotency_scope_check
        CHECK (idempotency_scope IN (
            'writer_sequence', 'tool_call', 'provider_request',
            'trace_span', 'room_operation', 'quest_attempt'
        )),
    CONSTRAINT insula_log_idempotency_scope_not_session_check
        CHECK (idempotency_scope <> 'session'),
    CONSTRAINT insula_log_expiry_check CHECK (expires_at > observed_at),
    CONSTRAINT insula_log_parent_span_check CHECK (parent_span_id <> span_id),

    -- Three explicit uniqueness layers, each with its own meaning and its own
    -- consequence. `event_id` is stable global identity, and the writer pair is
    -- transport order; a collision on either outside the already-matched
    -- logical event is a loud conflict, never a collapse.
    CONSTRAINT insula_log_writer_sequence_key UNIQUE (writer_id, writer_sequence),
    -- Only this layer collapses: it is the logical identity of the observation
    -- and it is the one that survives session failover.
    CONSTRAINT insula_log_idempotency_key
        UNIQUE (house_id, idempotency_version, idempotency_scope, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_insula_log_window
    ON insula.log (observed_at DESC);

CREATE INDEX IF NOT EXISTS idx_insula_log_trace
    ON insula.log (trace_id, writer_id, writer_sequence);

CREATE INDEX IF NOT EXISTS idx_insula_log_parent
    ON insula.log (parent_span_id)
    WHERE parent_span_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_insula_log_vitals
    ON insula.log (
        observed_at, house_id, room, spirit,
        component, layer, operation, phase, outcome_class
    );

-- Session drilldown lives only here, in the raw window.
CREATE INDEX IF NOT EXISTS idx_insula_log_session
    ON insula.log (house_id, room, session_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS idx_insula_log_retention
    ON insula.log (expires_at, house_id);

-- ---------------------------------------------------------------------------
-- Persistent minute rollups
-- ---------------------------------------------------------------------------
-- Rollups persist arithmetic only: counts, sums and maxima. Nothing here is a
-- judgement, an average frozen at write time, or a value that cannot be
-- rebuilt. Each row names the exact query and version that produced it, so a
-- later arithmetic change becomes version 2 beside version 1 instead of
-- silently rewriting history. `query_name` is exact; `query_version` is any
-- positive version, so a v2 row is constructible beside a v1 row.
--
-- Dimensions are house, room and spirit: the operator asked for per-spirit and
-- per-room stats, and a spirit is a durable identity worth carrying forever.
--
-- A deliberate v1 bound: `session_id` is NOT a rollup dimension. Sessions are
-- ephemeral and unbounded in number, so persisting them forever would grow
-- without limit and would pin a spirit's history to disposable process
-- identities. Session drilldown therefore exists only in the 14-day raw rows
-- and, after retention, in the typed tombstone — never in a permanent rollup.
CREATE TABLE IF NOT EXISTS insula.vitals_minute (
    query_name      TEXT          NOT NULL,
    query_version   SMALLINT      NOT NULL,
    minute          TIMESTAMPTZ   NOT NULL,
    house_id        TEXT          NOT NULL,
    room            TEXT          NOT NULL,
    spirit          TEXT          NOT NULL,
    component       TEXT          NOT NULL,
    layer           TEXT          NOT NULL,
    operation       TEXT          NOT NULL,
    phase           TEXT          NOT NULL,
    outcome_class   TEXT          NOT NULL,
    event_count     BIGINT        NOT NULL DEFAULT 0,
    duration_us_sum BIGINT        NOT NULL DEFAULT 0,
    duration_us_max BIGINT,
    bytes_in_sum    BIGINT        NOT NULL DEFAULT 0,
    bytes_out_sum   BIGINT        NOT NULL DEFAULT 0,
    tokens_in_sum   BIGINT        NOT NULL DEFAULT 0,
    tokens_out_sum  BIGINT        NOT NULL DEFAULT 0,
    drop_count_sum  BIGINT        NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW(),

    PRIMARY KEY (
        query_name, query_version, minute,
        house_id, room, spirit, component, layer, operation, phase, outcome_class
    ),
    CONSTRAINT insula_vitals_minute_query_check
        CHECK (
            query_name = 'insula.vitals.minute'
            AND query_version BETWEEN 1 AND 32767
        ),
    -- Immutable by construction: date_trunc over timestamptz depends on the
    -- session TimeZone and may not appear in a CHECK, so the minute boundary
    -- is pinned to UTC.
    CONSTRAINT insula_vitals_minute_truncation_check
        CHECK (minute = date_trunc('minute', minute AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'),
    CONSTRAINT insula_vitals_minute_phase_check
        CHECK (phase IN ('start', 'end', 'point', 'drop')),
    CONSTRAINT insula_vitals_minute_outcome_class_check
        CHECK (outcome_class IN (
            'ok', 'refused', 'error', 'timeout', 'cancelled', 'degraded', 'unknown'
        )),
    CONSTRAINT insula_vitals_minute_arithmetic_check
        CHECK (
            event_count >= 0
            AND duration_us_sum >= 0
            AND (duration_us_max IS NULL OR duration_us_max >= 0)
            AND bytes_in_sum >= 0
            AND bytes_out_sum >= 0
            AND tokens_in_sum >= 0
            AND tokens_out_sum >= 0
            AND drop_count_sum >= 0
        )
);

CREATE INDEX IF NOT EXISTS idx_insula_vitals_minute_window
    ON insula.vitals_minute (house_id, minute DESC);

CREATE INDEX IF NOT EXISTS idx_insula_vitals_minute_spirit
    ON insula.vitals_minute (house_id, spirit, minute DESC);

CREATE INDEX IF NOT EXISTS idx_insula_vitals_minute_room
    ON insula.vitals_minute (house_id, room, minute DESC);

-- ---------------------------------------------------------------------------
-- Retention receipts and tombstones
-- ---------------------------------------------------------------------------
-- Raw events default to 14 days. Deletion is not forgetting: a typed receipt
-- and its per-writer tombstones are written *before* the raw rows go, so the
-- shape of what was observed and the writer sequence ranges that were held
-- survive the sweep. RESTRICT on the tombstone reference keeps a receipt from
-- being erased out from under its own tombstones.
CREATE TABLE IF NOT EXISTS insula.retention_receipts (
    receipt_id           UUID          PRIMARY KEY,
    receipt_kind         TEXT          NOT NULL,
    house_id             TEXT          NOT NULL,
    retention_days       SMALLINT      NOT NULL,
    swept_through        TIMESTAMPTZ   NOT NULL,
    window_start         TIMESTAMPTZ   NOT NULL,
    window_end           TIMESTAMPTZ   NOT NULL,
    event_count          BIGINT        NOT NULL,
    writer_count         BIGINT        NOT NULL,
    duplicate_count_sum  BIGINT        NOT NULL,
    drop_count_sum       BIGINT        NOT NULL,
    created_at           TIMESTAMPTZ   NOT NULL DEFAULT NOW(),

    CONSTRAINT insula_retention_receipts_kind_check
        CHECK (receipt_kind = 'insula.retention.raw_delete'),
    CONSTRAINT insula_retention_receipts_retention_days_check
        CHECK (retention_days BETWEEN 1 AND 3650),
    CONSTRAINT insula_retention_receipts_window_check
        CHECK (window_end >= window_start),
    CONSTRAINT insula_retention_receipts_counts_check
        CHECK (
            event_count > 0
            AND writer_count > 0
            AND duplicate_count_sum >= 0
            AND drop_count_sum >= 0
        )
);

CREATE INDEX IF NOT EXISTS idx_insula_retention_receipts_house
    ON insula.retention_receipts (house_id, swept_through DESC);

-- One tombstone per writer per sweep. The session and spirit that were held in
-- the deleted window are named here, because this is the only place session
-- shape outlives the raw rows.
CREATE TABLE IF NOT EXISTS insula.log_tombstones (
    tombstone_id          UUID          PRIMARY KEY,
    receipt_id            UUID          NOT NULL
        REFERENCES insula.retention_receipts (receipt_id) ON DELETE RESTRICT,
    receipt_kind          TEXT          NOT NULL,
    house_id              TEXT          NOT NULL,
    writer_id             UUID          NOT NULL,
    first_writer_sequence BIGINT        NOT NULL,
    last_writer_sequence  BIGINT        NOT NULL,
    first_observed_at     TIMESTAMPTZ   NOT NULL,
    last_observed_at      TIMESTAMPTZ   NOT NULL,
    event_count           BIGINT        NOT NULL,
    room_count            BIGINT        NOT NULL,
    spirit_count          BIGINT        NOT NULL,
    session_count         BIGINT        NOT NULL,
    duplicate_count_sum   BIGINT        NOT NULL,
    drop_count_sum        BIGINT        NOT NULL,
    created_at            TIMESTAMPTZ   NOT NULL DEFAULT NOW(),

    CONSTRAINT insula_log_tombstones_writer_key UNIQUE (receipt_id, writer_id),
    CONSTRAINT insula_log_tombstones_kind_check
        CHECK (receipt_kind = 'insula.retention.raw_delete'),
    CONSTRAINT insula_log_tombstones_sequence_check
        CHECK (last_writer_sequence >= first_writer_sequence AND first_writer_sequence > 0),
    CONSTRAINT insula_log_tombstones_observed_check
        CHECK (last_observed_at >= first_observed_at),
    CONSTRAINT insula_log_tombstones_counts_check
        CHECK (
            event_count > 0
            AND room_count > 0
            AND spirit_count > 0
            AND session_count > 0
            AND duplicate_count_sum >= 0
            AND drop_count_sum >= 0
        )
);

CREATE INDEX IF NOT EXISTS idx_insula_log_tombstones_writer
    ON insula.log_tombstones (house_id, writer_id, last_writer_sequence DESC);

-- ---------------------------------------------------------------------------
-- Body-free guard
-- ---------------------------------------------------------------------------
-- Structural, not advisory: if a JSON attribute bag or a prose-shaped column
-- is ever added to insula.log, the next migration run stops here.
DO $$
DECLARE
  offender TEXT;
BEGIN
  SELECT string_agg(format('%s %s', a.attname, format_type(a.atttypid, a.atttypmod)), ', ')
  INTO offender
  FROM pg_attribute a
  WHERE a.attrelid = 'insula.log'::regclass
    AND a.attnum > 0
    AND NOT a.attisdropped
    AND (
      format_type(a.atttypid, a.atttypmod) IN ('json', 'jsonb')
      OR a.attname ~ '(prompt|body|payload|content|message|prose|snippet|detail)'
    );
  IF offender IS NOT NULL THEN
    RAISE EXCEPTION
      'insula.log must stay body free; refused column(s): %', offender;
  END IF;
END $$;

COMMIT;
