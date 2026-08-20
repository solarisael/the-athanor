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
-- Re-applicable against fresh state and compatible existing relation shapes.

BEGIN;
CREATE EXTENSION IF NOT EXISTS pgcrypto;


CREATE SCHEMA IF NOT EXISTS insula;

-- CREATE TABLE IF NOT EXISTS heals an absent relation. Any relation that
-- already exists must match the complete Insula contract before creation or
-- index statements proceed; a mismatched existing relation is refused loudly.
-- Fully matching existing relations remain re-applicable.
DO $$
DECLARE
    relation_name       TEXT;
    expected_columns    TEXT[];
    actual_columns      TEXT[];
    offender            TEXT;
    expected_constraints TEXT[];
    actual_constraints TEXT[];
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
        'log', 'vitals_minute', 'retention_receipts', 'log_tombstones'
    ] LOOP
        CONTINUE WHEN to_regclass(format('insula.%I', relation_name)) IS NULL;

        IF relation_name = 'log' THEN
            SELECT string_agg(a.attname, ', ' ORDER BY a.attnum)
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
        END IF;

        CASE relation_name
            WHEN 'log' THEN
                expected_columns := ARRAY[
                    'schema_version:smallint:required', 'event_id:uuid:required',
                    'span_id:uuid:required', 'trace_id:uuid:required',
                    'parent_span_id:uuid:nullable', 'writer_id:uuid:required',
                    'writer_sequence:bigint:required', 'house_id:text:required',
                    'room:text:required', 'spirit:text:required',
                    'session_id:text:required', 'component:text:required',
                    'layer:text:required', 'operation:text:required',
                    'phase:text:required', 'observed_at:timestamp with time zone:required',
                    'duration_us:bigint:nullable', 'outcome_class:text:required',
                    'error_class:text:nullable', 'bytes_in:bigint:required',
                    'bytes_out:bigint:required', 'tokens_in:bigint:required',
                    'tokens_out:bigint:required', 'tool_call_id:text:nullable',
                    'provider_request_id:text:nullable',
                    'idempotency_version:smallint:required',
                    'idempotency_scope:text:required', 'idempotency_key:text:required',
                    'receipt_kind:text:nullable', 'receipt_id:text:nullable',
                    'semantic_hash:text:required', 'drop_count:bigint:required',
                    'expires_at:timestamp with time zone:required',
                    'duplicate_count:bigint:required',
                    'last_duplicate_at:timestamp with time zone:nullable',
                    'ingested_at:timestamp with time zone:required'
                ];
                expected_constraints := ARRAY[
                    'insula_log_binding_check', 'insula_log_correlation_bound_check',
                    'insula_log_counts_check', 'insula_log_drop_count_check',
                    'insula_log_drop_phase_check', 'insula_log_duplicate_count_check',
                    'insula_log_duration_bound_check', 'insula_log_duration_shape_check',
                    'insula_log_error_class_check', 'insula_log_expiry_check',
                    'insula_log_idempotency_key', 'insula_log_idempotency_key_shape_check',
                    'insula_log_idempotency_scope_check',
                    'insula_log_idempotency_scope_not_session_check',
                    'insula_log_idempotency_version_check',
                    'insula_log_mechanical_names_check',
                    'insula_log_outcome_class_check', 'insula_log_parent_span_check',
                    'insula_log_phase_check', 'insula_log_receipt_pair_check',
                    'insula_log_schema_version_check', 'insula_log_semantic_hash_check',
                    'insula_log_writer_sequence_check', 'insula_log_writer_sequence_key',
                    'log_pkey'
                ];
            WHEN 'vitals_minute' THEN
                expected_columns := ARRAY[
                    'query_name:text:required', 'query_version:smallint:required',
                    'minute:timestamp with time zone:required', 'house_id:text:required',
                    'room:text:required', 'spirit:text:required',
                    'component:text:required', 'layer:text:required',
                    'operation:text:required', 'phase:text:required',
                    'outcome_class:text:required', 'event_count:bigint:required',
                    'duration_us_sum:bigint:required', 'duration_us_max:bigint:nullable',
                    'bytes_in_sum:bigint:required', 'bytes_out_sum:bigint:required',
                    'tokens_in_sum:bigint:required', 'tokens_out_sum:bigint:required',
                    'drop_count_sum:bigint:required',
                    'source_first_sequence:bigint:required',
                    'source_last_sequence:bigint:required',
                    'source_first_observed_at:timestamp with time zone:required',
                    'source_last_observed_at:timestamp with time zone:required',
                    'source_coverage_hash:text:required',
                    'updated_at:timestamp with time zone:required'
                ];
                expected_constraints := ARRAY[
                    'insula_vitals_minute_arithmetic_check',
                    'insula_vitals_minute_coverage_hash_check',
                    'insula_vitals_minute_outcome_class_check',
                    'insula_vitals_minute_phase_check',
                    'insula_vitals_minute_query_check',
                    'insula_vitals_minute_sequence_check',
                    'insula_vitals_minute_truncation_check',
                    'vitals_minute_pkey'
                ];
            WHEN 'retention_receipts' THEN
                expected_columns := ARRAY[
                    'receipt_id:uuid:required', 'receipt_kind:text:required',
                    'receipt_version:smallint:required', 'house_id:text:required',
                    'sweep_version:smallint:required', 'sweep_key:text:required',
                    'retention_days:smallint:required',
                    'swept_through:timestamp with time zone:required',
                    'window_start:timestamp with time zone:required',
                    'window_end:timestamp with time zone:required',
                    'event_count:bigint:required', 'writer_count:bigint:required',
                    'duplicate_count_sum:bigint:required',
                    'drop_count_sum:bigint:required',
                    'coverage_version:smallint:required',
                    'coverage_hash:text:required',
                    'rollup_query_name:text:required',
                    'rollup_query_version:smallint:required',
                    'rollup_watermark:timestamp with time zone:required',
                    'created_at:timestamp with time zone:required'
                ];
                expected_constraints := ARRAY[
                    'insula_retention_receipts_counts_check',
                    'insula_retention_receipts_coverage_hash_check',
                    'insula_retention_receipts_coverage_version_check',
                    'insula_retention_receipts_house_receipt_key',
                    'insula_retention_receipts_kind_check',
                    'insula_retention_receipts_retention_days_check',
                    'insula_retention_receipts_rollup_check',
                    'insula_retention_receipts_sweep_key',
                    'insula_retention_receipts_sweep_version_check',
                    'insula_retention_receipts_window_check',
                    'retention_receipts_pkey'
                ];
            WHEN 'log_tombstones' THEN
                expected_columns := ARRAY[
                    'tombstone_id:uuid:required', 'receipt_id:uuid:required',
                    'receipt_kind:text:required', 'house_id:text:required',
                    'writer_id:uuid:required', 'first_writer_sequence:bigint:required',
                    'last_writer_sequence:bigint:required',
                    'first_observed_at:timestamp with time zone:required',
                    'last_observed_at:timestamp with time zone:required',
                    'event_count:bigint:required', 'room_count:bigint:required',
                    'spirit_count:bigint:required', 'session_count:bigint:required',
                    'duplicate_count_sum:bigint:required',
                    'drop_count_sum:bigint:required',
                    'coverage_version:smallint:required',
                    'coverage_hash:text:required',
                    'created_at:timestamp with time zone:required'
                ];
                expected_constraints := ARRAY[
                    'insula_log_tombstones_counts_check',
                    'insula_log_tombstones_coverage_hash_check',
                    'insula_log_tombstones_kind_check',
                    'insula_log_tombstones_observed_check',
                    'insula_log_tombstones_receipt_house_fkey',
                    'insula_log_tombstones_sequence_check',
                    'insula_log_tombstones_writer_key',
                    'log_tombstones_pkey'
                ];
        END CASE;

        SELECT array_agg(
                   a.attname || ':' || format_type(a.atttypid, a.atttypmod) || ':' ||
                   CASE WHEN a.attnotnull THEN 'required' ELSE 'nullable' END
                   ORDER BY a.attnum
               )
          INTO actual_columns
          FROM pg_attribute a
         WHERE a.attrelid = format('insula.%I', relation_name)::regclass
           AND a.attnum > 0
           AND NOT a.attisdropped;

        IF actual_columns IS DISTINCT FROM expected_columns THEN
            RAISE EXCEPTION
                'refusing mismatched partial Insula relation insula.% (column contract differs)',
                relation_name;
        END IF;

        SELECT array_agg(c.conname ORDER BY c.conname)
          INTO actual_constraints
          FROM pg_constraint c
         WHERE c.conrelid = format('insula.%I', relation_name)::regclass;

        IF actual_constraints IS DISTINCT FROM expected_constraints THEN
            RAISE EXCEPTION
                'refusing mismatched partial Insula relation insula.% (constraint contract differs)',
                relation_name;
        END IF;
    END LOOP;
END $$;

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
-- Quest and attempt identifiers deliberately do not exist here. Docket's table
-- inventory and ID types remain undecided, and Insula has no fencing authority.
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
    CONSTRAINT insula_log_receipt_pair_check
        CHECK ((receipt_kind IS NULL) = (receipt_id IS NULL)),
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
            'trace_span', 'room_operation'
        )),
    CONSTRAINT insula_log_idempotency_scope_not_session_check
        CHECK (idempotency_scope <> 'session'),
    CONSTRAINT insula_log_expiry_check CHECK (expires_at = observed_at + INTERVAL '14 days'),
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
-- identities. Session drilldown exists only in the 14-day raw rows. Retention
-- preserves session cardinality, not session identity.
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
    -- Source watermarks and the versioned coverage chain survive raw expiry.
    -- They are mechanical provenance for rebuilding wider Vitals windows from
    -- these permanent rows; they are never session dimensions.
    source_first_sequence    BIGINT      NOT NULL,
    source_last_sequence     BIGINT      NOT NULL,
    source_first_observed_at TIMESTAMPTZ NOT NULL,
    source_last_observed_at  TIMESTAMPTZ NOT NULL,
    source_coverage_hash     TEXT        NOT NULL,
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
        ),
    CONSTRAINT insula_vitals_minute_sequence_check
        CHECK (
            source_first_sequence > 0
            AND source_last_sequence >= source_first_sequence
            AND source_last_observed_at >= source_first_observed_at
        ),
    CONSTRAINT insula_vitals_minute_coverage_hash_check
        CHECK (source_coverage_hash ~ '^[0-9a-f]{64}$')
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
-- Raw events expire after exactly 14 days. Deletion is not forgetting: a typed receipt
-- and its per-writer tombstones are written *before* the raw rows go, so the
-- shape of what was observed and the writer sequence ranges that were held
-- survive the sweep. RESTRICT on the tombstone reference keeps a receipt from
-- being erased out from under its own tombstones.
CREATE TABLE IF NOT EXISTS insula.retention_receipts (
    receipt_id           UUID          PRIMARY KEY,
    receipt_kind         TEXT          NOT NULL,
    receipt_version      SMALLINT      NOT NULL,
    house_id             TEXT          NOT NULL,
    sweep_version        SMALLINT      NOT NULL,
    sweep_key            TEXT          NOT NULL,
    retention_days       SMALLINT      NOT NULL,
    swept_through        TIMESTAMPTZ   NOT NULL,
    window_start         TIMESTAMPTZ   NOT NULL,
    window_end           TIMESTAMPTZ   NOT NULL,
    event_count          BIGINT        NOT NULL,
    writer_count         BIGINT        NOT NULL,
    duplicate_count_sum  BIGINT        NOT NULL,
    drop_count_sum       BIGINT        NOT NULL,
    coverage_version     SMALLINT      NOT NULL,
    coverage_hash        TEXT          NOT NULL,
    rollup_query_name    TEXT          NOT NULL,
    rollup_query_version SMALLINT      NOT NULL,
    rollup_watermark     TIMESTAMPTZ   NOT NULL,
    created_at           TIMESTAMPTZ   NOT NULL DEFAULT NOW(),

    CONSTRAINT insula_retention_receipts_house_receipt_key
        UNIQUE (receipt_id, house_id),
    CONSTRAINT insula_retention_receipts_sweep_key
        UNIQUE (house_id, sweep_version, sweep_key),
    CONSTRAINT insula_retention_receipts_kind_check
        CHECK (receipt_kind = 'insula.retention.raw_delete'),
    CONSTRAINT insula_retention_receipts_sweep_version_check
        CHECK (receipt_version = 1 AND sweep_version = 1),
    CONSTRAINT insula_retention_receipts_retention_days_check
        CHECK (retention_days = 14),
    CONSTRAINT insula_retention_receipts_window_check
        CHECK (window_end >= window_start),
    CONSTRAINT insula_retention_receipts_counts_check
        CHECK (
            event_count > 0
            AND writer_count > 0
            AND duplicate_count_sum >= 0
            AND drop_count_sum >= 0
        ),
    CONSTRAINT insula_retention_receipts_coverage_version_check
        CHECK (coverage_version = 1),
    CONSTRAINT insula_retention_receipts_coverage_hash_check
        CHECK (coverage_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT insula_retention_receipts_rollup_check
        CHECK (
            rollup_query_name = 'insula.vitals.minute'
            AND rollup_query_version = 1
        )
);

CREATE INDEX IF NOT EXISTS idx_insula_retention_receipts_house
    ON insula.retention_receipts (house_id, swept_through DESC);

-- One tombstone per writer per sweep. Only cardinalities survive for room,
-- spirit and session; no session identity is retained. The per-writer coverage
-- hash proves the exact event set rather than pretending a sequence range has
-- no gaps. The composite foreign key enforces that receipt and tombstone name
-- the same House.
CREATE TABLE IF NOT EXISTS insula.log_tombstones (
    tombstone_id          UUID          PRIMARY KEY,
    receipt_id            UUID          NOT NULL,
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
    coverage_version      SMALLINT      NOT NULL,
    coverage_hash         TEXT          NOT NULL,
    created_at            TIMESTAMPTZ   NOT NULL DEFAULT NOW(),

    CONSTRAINT insula_log_tombstones_writer_key UNIQUE (receipt_id, writer_id),
    CONSTRAINT insula_log_tombstones_receipt_house_fkey
        FOREIGN KEY (receipt_id, house_id)
        REFERENCES insula.retention_receipts (receipt_id, house_id)
        ON DELETE RESTRICT,
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
        ),
    CONSTRAINT insula_log_tombstones_coverage_hash_check
        CHECK (coverage_version = 1 AND coverage_hash ~ '^[0-9a-f]{64}$')
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
