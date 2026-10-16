BEGIN;

CREATE TABLE IF NOT EXISTS orihon_cursor (
    room TEXT PRIMARY KEY CHECK (length(btrim(room)) > 0),
    next_seq BIGINT NOT NULL DEFAULT 0 CHECK (next_seq >= 0),
    next_turn BIGINT NOT NULL DEFAULT 0 CHECK (next_turn >= 0)
);

CREATE TABLE IF NOT EXISTS orihon (
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    seq BIGINT NOT NULL CHECK (seq >= 0),
    turn_no BIGINT NOT NULL CHECK (turn_no >= 0),
    session_id TEXT NOT NULL CHECK (length(btrim(session_id)) > 0),
    harness TEXT NOT NULL CHECK (length(btrim(harness)) > 0),
    source_session TEXT NOT NULL CHECK (length(btrim(source_session)) > 0),
    source_id TEXT NOT NULL CHECK (length(btrim(source_id)) > 0),
    parent_user_id TEXT CHECK (length(btrim(parent_user_id)) > 0),
    kind TEXT NOT NULL CHECK (kind IN ('user','talk','tool','echo','work','note')),
    speaker TEXT NOT NULL CHECK (length(btrim(speaker)) > 0),
    text TEXT NOT NULL,
    bytes INTEGER GENERATED ALWAYS AS (octet_length(text)) STORED,
    source_time TEXT NOT NULL,
    said_at TIMESTAMPTZ NOT NULL,
    timestamp_observed BOOLEAN NOT NULL DEFAULT false,
    written_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    truncated BOOLEAN NOT NULL DEFAULT false,
    redacted BOOLEAN NOT NULL DEFAULT false,
    content_digest TEXT NOT NULL CHECK (content_digest ~ '^[0-9a-f]{64}$'),
    PRIMARY KEY (room, seq),
    UNIQUE (room, harness, source_session, source_id),
    FOREIGN KEY (room, harness, source_session, parent_user_id)
        REFERENCES orihon(room, harness, source_session, source_id),
    CHECK ((kind = 'user') = (parent_user_id IS NULL)),
    CHECK (NOT truncated OR kind = 'echo'),
    CHECK ((kind = 'echo' AND char_length(text) <= 30000)
        OR (kind <> 'echo' AND octet_length(text) <= 1048576))
);

CREATE INDEX IF NOT EXISTS idx_orihon_page ON orihon(room, turn_no, seq);
CREATE UNIQUE INDEX IF NOT EXISTS idx_orihon_user_page ON orihon(room, turn_no) WHERE kind = 'user';
-- Source precision can exceed PostgreSQL microseconds. The source day must not round into tomorrow.
CREATE INDEX IF NOT EXISTS idx_orihon_source_day ON orihon(room, (left(source_time, 10)), seq);

CREATE OR REPLACE FUNCTION orihon_require_parent() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.kind <> 'user' AND NOT EXISTS (
        SELECT 1 FROM orihon parent
        WHERE parent.room = NEW.room
          AND parent.harness = NEW.harness
          AND parent.source_session = NEW.source_session
          AND parent.source_id = NEW.parent_user_id
          AND parent.kind = 'user'
          AND parent.seq < NEW.seq
          AND parent.turn_no = NEW.turn_no
    ) THEN
        RAISE EXCEPTION 'Orihon parent must be an earlier user on the same page';
    END IF;
    RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION orihon_refuse_mutation() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Orihon records are append-only';
END $$;

DROP TRIGGER IF EXISTS orihon_parent ON orihon;
CREATE TRIGGER orihon_parent BEFORE INSERT ON orihon
    FOR EACH ROW EXECUTE FUNCTION orihon_require_parent();

DROP TRIGGER IF EXISTS orihon_append_only ON orihon;
CREATE TRIGGER orihon_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON orihon
    FOR EACH STATEMENT EXECUTE FUNCTION orihon_refuse_mutation();

INSERT INTO schema_migrations(version) VALUES (37) ON CONFLICT(version) DO NOTHING;

COMMIT;
