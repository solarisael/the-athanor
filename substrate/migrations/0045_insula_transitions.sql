-- 0045: Insula transitions — one row for each move a Hearth state machine makes.
--
-- `insula.log` measures what an operation cost. A transition records what happened
-- to one subject: which machine, which row, from, event, to, and who caused it.
-- The shapes share almost no columns, so transitions get their own table.
--
-- Raw rows keep 30 days; the native retention job deletes older rows.
-- `transitions_daily` counts each edge per UTC day and is never deleted.
-- The ingest statement writes both in one statement, so the counts need no
-- coverage proof before a raw row goes.
--
-- Like `insula.log`, both tables are body free: mechanical names and one opaque
-- subject id, no JSON and no prose.
--
-- Re-applicable against fresh state.

BEGIN;

DO $$
BEGIN
    IF to_regnamespace('insula') IS NULL THEN
        RAISE EXCEPTION 'schema insula is missing; apply 0022 first';
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS insula.transitions (
    event_id        UUID          PRIMARY KEY,
    writer_id       UUID          NOT NULL,
    writer_sequence BIGINT        NOT NULL CHECK (writer_sequence > 0),
    house_id        TEXT          NOT NULL,
    room            TEXT          NOT NULL,
    spirit          TEXT          NOT NULL,
    session_id      TEXT          NOT NULL,
    machine         TEXT          NOT NULL,
    subject_id      TEXT          NOT NULL,
    from_state      TEXT          NOT NULL,
    event           TEXT          NOT NULL,
    to_state        TEXT          NOT NULL,
    observed_at     TIMESTAMPTZ   NOT NULL,
    UNIQUE (writer_id, writer_sequence),
    CONSTRAINT insula_transitions_names_check CHECK (
        machine ~ '^[a-z][a-z0-9_]{0,63}$'
        AND from_state ~ '^[a-z][a-z0-9_]{0,63}$'
        AND event ~ '^[a-z][a-z0-9_]{0,63}$'
        AND to_state ~ '^[a-z][a-z0-9_]{0,63}$'
    ),
    -- Postgres caps a regex repetition count at 255, so the length is its own predicate.
    CONSTRAINT insula_transitions_subject_check CHECK (
        octet_length(subject_id) BETWEEN 1 AND 256
        AND subject_id ~ '^[A-Za-z0-9_.:/@-]+$'
    )
);

-- Retention deletes by house and age.
CREATE INDEX IF NOT EXISTS idx_insula_transitions_retention
    ON insula.transitions (house_id, observed_at);

CREATE TABLE IF NOT EXISTS insula.transitions_daily (
    day              DATE     NOT NULL,
    house_id         TEXT     NOT NULL,
    machine          TEXT     NOT NULL,
    from_state       TEXT     NOT NULL,
    event            TEXT     NOT NULL,
    to_state         TEXT     NOT NULL,
    transition_count BIGINT   NOT NULL CHECK (transition_count > 0),
    PRIMARY KEY (house_id, machine, day, from_state, event, to_state)
);

COMMIT;
