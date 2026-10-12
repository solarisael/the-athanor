-- 0034: Worker lifecycle — task-lineage continuity for bounded workers.
--
-- One parent task invocation owns a swarm. Each task slot owns one book, and
-- successive worker instances share that book only through an explicit
-- predecessor link. Instances move monotonically through
-- working -> wrapping -> handoff_only -> retired.
--
-- The rules live in hearth::worker_lifecycle. This schema stores typed
-- metadata rows (swarm, book, instance, unresolved operation) that the store
-- rewrites per changed book under a swarm row lock, plus append-only entries
-- that are never rewritten. Enrollment tokens are stored only as sha256.
--
-- Book entries are transient: they may be deleted only by an explicit
-- whole-swarm disposal, after the swarm row records disposed_at. Accepted
-- evidence and disposal receipts are retained and append-only. Deleting rows
-- here does not erase existing backups.
--
-- Re-applicable against fresh state. A pre-existing relation must match the
-- column contract or this migration refuses loudly.

BEGIN;

CREATE SCHEMA IF NOT EXISTS worker_lifecycle;

DO $$
DECLARE
    relation_name    TEXT;
    expected_columns TEXT[];
    actual_columns   TEXT[];
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
        'swarms', 'books', 'instances', 'operations', 'entries', 'retained',
        'dispositions', 'requests'
    ] LOOP
        CONTINUE WHEN to_regclass(format('worker_lifecycle.%I', relation_name)) IS NULL;

        CASE relation_name
            WHEN 'swarms' THEN
                expected_columns := ARRAY[
                    'swarm_id:text:required', 'house_id:text:required',
                    'room:text:required', 'owner_spirit:text:required',
                    'owner_session:text:required', 'swarm_key:text:required',
                    'created_at:timestamp with time zone:required',
                    'disposed_at:timestamp with time zone:nullable'
                ];
            WHEN 'books' THEN
                expected_columns := ARRAY[
                    'book_id:text:required', 'swarm_id:text:required',
                    'slot_key:text:required', 'agent_name:text:required',
                    'task_excerpt:text:required', 'next_sequence:bigint:required',
                    'accepted_count:bigint:required',
                    'created_at:timestamp with time zone:required'
                ];
            WHEN 'instances' THEN
                expected_columns := ARRAY[
                    'instance_id:text:required', 'book_id:text:required',
                    'ordinal:bigint:required', 'predecessor_instance_id:text:nullable',
                    'reservation_key:text:required', 'token_hash:text:required',
                    'agent_name:text:required', 'bound_session:text:nullable',
                    'agent_id:text:nullable', 'transcript_path:text:nullable',
                    'state:text:required', 'warned:boolean:required',
                    'handoff_requests:bigint:required',
                    'checkpoint_sequence:bigint:nullable', 'handoff:jsonb:nullable',
                    'settled_outcome:text:nullable', 'own_entries:bigint:required',
                    'last_own_sequence:bigint:nullable', 'opened_count:bigint:required',
                    'created_at:timestamp with time zone:required',
                    'updated_at:timestamp with time zone:required'
                ];
            WHEN 'operations' THEN
                expected_columns := ARRAY[
                    'book_id:text:required', 'operation_id:text:required',
                    'instance_id:text:required', 'started_sequence:bigint:required'
                ];
            WHEN 'entries' THEN
                expected_columns := ARRAY[
                    'book_id:text:required', 'sequence:bigint:required',
                    'swarm_id:text:required', 'instance_id:text:nullable',
                    'author:text:required', 'kind:text:required', 'body:jsonb:required',
                    'created_at:timestamp with time zone:required'
                ];
            WHEN 'retained' THEN
                expected_columns := ARRAY[
                    'retained_id:text:required', 'swarm_id:text:required',
                    'book_id:text:required', 'slot_key:text:required',
                    'agent_name:text:required', 'instance_ids:jsonb:required',
                    'accepted_by_room:text:required', 'accepted_by_spirit:text:required',
                    'accepted_by_session:text:required', 'body:jsonb:required',
                    'created_at:timestamp with time zone:required'
                ];
            WHEN 'dispositions' THEN
                expected_columns := ARRAY[
                    'swarm_id:text:required', 'disposed_by_room:text:required',
                    'disposed_by_spirit:text:required', 'disposed_by_session:text:required',
                    'discard_reason:text:nullable', 'books:jsonb:required',
                    'disposed_at:timestamp with time zone:required'
                ];
            WHEN 'requests' THEN
                expected_columns := ARRAY[
                    'house_id:text:required', 'room:text:required',
                    'session:text:required', 'request_id:text:required',
                    'action:text:required', 'fingerprint:text:required',
                    'response:jsonb:required', 'swarm_id:text:nullable',
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
         WHERE a.attrelid = format('worker_lifecycle.%I', relation_name)::regclass
           AND a.attnum > 0
           AND NOT a.attisdropped;

        IF actual_columns IS DISTINCT FROM expected_columns THEN
            RAISE EXCEPTION 'worker_lifecycle.% exists with a different column contract: %',
                relation_name, actual_columns;
        END IF;
    END LOOP;
END
$$;

CREATE TABLE IF NOT EXISTS worker_lifecycle.swarms (
    swarm_id      TEXT        PRIMARY KEY,
    house_id      TEXT        NOT NULL,
    room          TEXT        NOT NULL,
    owner_spirit  TEXT        NOT NULL,
    owner_session TEXT        NOT NULL,
    swarm_key     TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL,
    disposed_at   TIMESTAMPTZ,
    CONSTRAINT worker_lifecycle_swarms_owner_key UNIQUE (house_id, room, owner_session, swarm_key)
);
CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_swarms_owner
    ON worker_lifecycle.swarms (house_id, room, owner_session, created_at DESC)
    WHERE disposed_at IS NULL;

CREATE TABLE IF NOT EXISTS worker_lifecycle.books (
    book_id        TEXT        PRIMARY KEY,
    swarm_id       TEXT        NOT NULL REFERENCES worker_lifecycle.swarms (swarm_id),
    slot_key       TEXT        NOT NULL,
    agent_name     TEXT        NOT NULL,
    task_excerpt   TEXT        NOT NULL,
    next_sequence  BIGINT      NOT NULL CHECK (next_sequence >= 1),
    accepted_count BIGINT      NOT NULL CHECK (accepted_count >= 0),
    created_at     TIMESTAMPTZ NOT NULL,
    CONSTRAINT worker_lifecycle_books_slot UNIQUE (swarm_id, slot_key)
);

CREATE TABLE IF NOT EXISTS worker_lifecycle.instances (
    instance_id             TEXT        PRIMARY KEY,
    book_id                 TEXT        NOT NULL REFERENCES worker_lifecycle.books (book_id),
    ordinal                 BIGINT      NOT NULL CHECK (ordinal >= 1),
    predecessor_instance_id TEXT,
    reservation_key         TEXT        NOT NULL,
    token_hash              TEXT        NOT NULL CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    agent_name              TEXT        NOT NULL,
    bound_session           TEXT,
    agent_id                TEXT,
    transcript_path         TEXT,
    state                   TEXT        NOT NULL
        CHECK (state IN ('working', 'wrapping', 'handoff_only', 'retired')),
    warned                  BOOLEAN     NOT NULL,
    handoff_requests        BIGINT      NOT NULL CHECK (handoff_requests BETWEEN 0 AND 2),
    checkpoint_sequence     BIGINT,
    handoff                 JSONB,
    settled_outcome         TEXT        CHECK (settled_outcome IN ('completed', 'failed', 'aborted')),
    own_entries             BIGINT      NOT NULL CHECK (own_entries >= 0),
    last_own_sequence       BIGINT,
    opened_count            BIGINT      NOT NULL CHECK (opened_count >= 0),
    created_at              TIMESTAMPTZ NOT NULL,
    updated_at              TIMESTAMPTZ NOT NULL,
    CONSTRAINT worker_lifecycle_instances_ordinal UNIQUE (book_id, ordinal),
    CONSTRAINT worker_lifecycle_instances_token UNIQUE (token_hash),
    CONSTRAINT worker_lifecycle_instances_retired_handoff
        CHECK (state <> 'retired' OR handoff IS NOT NULL)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_worker_lifecycle_instances_bound
    ON worker_lifecycle.instances (book_id, bound_session)
    WHERE bound_session IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_instances_session
    ON worker_lifecycle.instances (bound_session)
    WHERE bound_session IS NOT NULL;

CREATE TABLE IF NOT EXISTS worker_lifecycle.operations (
    book_id          TEXT   NOT NULL REFERENCES worker_lifecycle.books (book_id),
    operation_id     TEXT   NOT NULL,
    instance_id      TEXT   NOT NULL REFERENCES worker_lifecycle.instances (instance_id),
    started_sequence BIGINT NOT NULL,
    PRIMARY KEY (book_id, operation_id)
);

CREATE TABLE IF NOT EXISTS worker_lifecycle.entries (
    book_id     TEXT        NOT NULL REFERENCES worker_lifecycle.books (book_id),
    sequence    BIGINT      NOT NULL CHECK (sequence >= 1),
    swarm_id    TEXT        NOT NULL REFERENCES worker_lifecycle.swarms (swarm_id),
    instance_id TEXT,
    author      TEXT        NOT NULL CHECK (author IN ('worker', 'parent', 'lifecycle')),
    kind        TEXT        NOT NULL,
    body        JSONB       NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (book_id, sequence)
);

-- Entries are append-only. Only whole-swarm disposal, which first records
-- disposed_at on the swarm, may delete them.
CREATE OR REPLACE FUNCTION worker_lifecycle.guard_entry_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' AND EXISTS (
        SELECT 1 FROM worker_lifecycle.swarms
         WHERE swarm_id = OLD.swarm_id AND disposed_at IS NOT NULL
    ) THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'worker_lifecycle.entries is append-only; % refused', TG_OP;
END $$;

DROP TRIGGER IF EXISTS worker_lifecycle_entries_append_only ON worker_lifecycle.entries;
CREATE TRIGGER worker_lifecycle_entries_append_only
    BEFORE UPDATE OR DELETE ON worker_lifecycle.entries
    FOR EACH ROW EXECUTE FUNCTION worker_lifecycle.guard_entry_mutation();

CREATE TABLE IF NOT EXISTS worker_lifecycle.retained (
    retained_id         TEXT        PRIMARY KEY,
    swarm_id            TEXT        NOT NULL REFERENCES worker_lifecycle.swarms (swarm_id),
    book_id             TEXT        NOT NULL,
    slot_key            TEXT        NOT NULL,
    agent_name          TEXT        NOT NULL,
    instance_ids        JSONB       NOT NULL,
    accepted_by_room    TEXT        NOT NULL,
    accepted_by_spirit  TEXT        NOT NULL,
    accepted_by_session TEXT        NOT NULL,
    body                JSONB       NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_retained_swarm
    ON worker_lifecycle.retained (swarm_id, created_at);

CREATE TABLE IF NOT EXISTS worker_lifecycle.dispositions (
    swarm_id            TEXT        PRIMARY KEY REFERENCES worker_lifecycle.swarms (swarm_id),
    disposed_by_room    TEXT        NOT NULL,
    disposed_by_spirit  TEXT        NOT NULL,
    disposed_by_session TEXT        NOT NULL,
    discard_reason      TEXT,
    books               JSONB       NOT NULL,
    disposed_at         TIMESTAMPTZ NOT NULL
);

CREATE OR REPLACE FUNCTION worker_lifecycle.refuse_retained_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'worker_lifecycle.% is append-only; % refused', TG_TABLE_NAME, TG_OP;
END $$;

DROP TRIGGER IF EXISTS worker_lifecycle_retained_append_only ON worker_lifecycle.retained;
CREATE TRIGGER worker_lifecycle_retained_append_only
    BEFORE UPDATE OR DELETE ON worker_lifecycle.retained
    FOR EACH ROW EXECUTE FUNCTION worker_lifecycle.refuse_retained_mutation();

DROP TRIGGER IF EXISTS worker_lifecycle_dispositions_append_only ON worker_lifecycle.dispositions;
CREATE TRIGGER worker_lifecycle_dispositions_append_only
    BEFORE UPDATE OR DELETE ON worker_lifecycle.dispositions
    FOR EACH ROW EXECUTE FUNCTION worker_lifecycle.refuse_retained_mutation();

-- Committed write requests and their exact responses, for idempotent retry.
-- Responses never contain enrollment tokens.
CREATE TABLE IF NOT EXISTS worker_lifecycle.requests (
    house_id    TEXT        NOT NULL,
    room        TEXT        NOT NULL,
    session     TEXT        NOT NULL,
    request_id  TEXT        NOT NULL,
    action      TEXT        NOT NULL,
    fingerprint TEXT        NOT NULL,
    response    JSONB       NOT NULL,
    swarm_id    TEXT,
    created_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (house_id, room, session, request_id)
);
CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_requests_swarm
    ON worker_lifecycle.requests (swarm_id)
    WHERE swarm_id IS NOT NULL;

COMMIT;
