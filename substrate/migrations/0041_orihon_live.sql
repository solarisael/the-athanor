BEGIN;

-- Semantic fold input and exact child identities on new records. Old rows keep NULL and their digests.
ALTER TABLE orihon ADD COLUMN IF NOT EXISTS fold_text TEXT;
ALTER TABLE orihon ADD COLUMN IF NOT EXISTS children JSONB;

CREATE OR REPLACE FUNCTION orihon_children_valid(children JSONB) RETURNS BOOLEAN
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN jsonb_typeof(children) IS DISTINCT FROM 'array' THEN false
        ELSE jsonb_array_length(children) BETWEEN 1 AND 16 AND NOT EXISTS (
            SELECT 1 FROM jsonb_array_elements(children) AS child(value)
            WHERE jsonb_typeof(value) <> 'object'
                OR value - ARRAY['name', 'session'] <> '{}'::jsonb
                OR jsonb_typeof(value->'name') IS DISTINCT FROM 'string'
                OR length(btrim(value->>'name')) = 0
                OR jsonb_typeof(value->'session') IS DISTINCT FROM 'string'
                OR length(btrim(value->>'session')) = 0)
    END
$$;

ALTER TABLE orihon DROP CONSTRAINT IF EXISTS orihon_fold_text_kind;
ALTER TABLE orihon ADD CONSTRAINT orihon_fold_text_kind CHECK (
    fold_text IS NULL OR (kind = 'tool' AND octet_length(fold_text) BETWEEN 1 AND 1048576));
ALTER TABLE orihon DROP CONSTRAINT IF EXISTS orihon_children_shape;
ALTER TABLE orihon ADD CONSTRAINT orihon_children_shape CHECK (
    children IS NULL OR (kind = 'work' AND orihon_children_valid(children)));
CREATE INDEX IF NOT EXISTS idx_orihon_children ON orihon USING gin (children jsonb_path_ops)
    WHERE children IS NOT NULL;

-- A session opened by a door message (restart, chat, knock) records a page-less prelude until its
-- first user page. Users still never name a parent; later non-user records still need one.
-- 0037 declared the rule as the table's first unnamed multi-column CHECK, so PostgreSQL named it
-- orihon_check (orihon_check1 and orihon_check2 are the echo rules and stay). The name alone is
-- not trusted: a different orihon_check stops the migration instead of being dropped.
DO $$
DECLARE
    definition TEXT;
BEGIN
    SELECT pg_get_constraintdef(oid) INTO definition
    FROM pg_constraint
    WHERE conrelid = 'orihon'::regclass AND conname = 'orihon_check';
    IF FOUND AND definition <> 'CHECK (((kind = ''user''::text) = (parent_user_id IS NULL)))' THEN
        RAISE EXCEPTION 'orihon_check is %, not the 0037 user-parent rule; inspect before migrating',
            definition;
    END IF;
END $$;
ALTER TABLE orihon DROP CONSTRAINT IF EXISTS orihon_check;
ALTER TABLE orihon DROP CONSTRAINT IF EXISTS orihon_user_parent;
ALTER TABLE orihon ADD CONSTRAINT orihon_user_parent CHECK (kind <> 'user' OR parent_user_id IS NULL);

CREATE OR REPLACE FUNCTION orihon_require_parent() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.kind = 'user' THEN
        RETURN NEW;
    END IF;
    IF NEW.parent_user_id IS NULL THEN
        IF EXISTS (
            SELECT 1 FROM orihon earlier
            WHERE earlier.room = NEW.room AND earlier.harness = NEW.harness
              AND earlier.source_session = NEW.source_session AND earlier.kind = 'user'
        ) OR EXISTS (
            SELECT 1 FROM orihon page
            WHERE page.room = NEW.room AND page.turn_no = NEW.turn_no
              AND (page.kind = 'user' OR page.parent_user_id IS NOT NULL
                OR page.harness <> NEW.harness OR page.source_session <> NEW.source_session)
        ) THEN
            RAISE EXCEPTION 'Orihon prelude needs a session without user pages and a page of its own';
        END IF;
        RETURN NEW;
    END IF;
    IF NOT EXISTS (
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

-- One message unit per stable contiguous chunk of a record. Units tile every record in seq order.
CREATE TABLE IF NOT EXISTS orihon_units (
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    unit BIGINT NOT NULL CHECK (unit >= 0),
    seq BIGINT NOT NULL CHECK (seq >= 0),
    part INTEGER NOT NULL CHECK (part >= 0),
    parts INTEGER NOT NULL CHECK (parts >= 1),
    byte_start INTEGER NOT NULL CHECK (byte_start >= 0),
    byte_end INTEGER NOT NULL,
    semantic BOOLEAN NOT NULL,
    record_digest TEXT NOT NULL CHECK (record_digest ~ '^[0-9a-f]{64}$'),
    digest TEXT NOT NULL CHECK (digest ~ '^[0-9a-f]{64}$'),
    PRIMARY KEY (room, unit),
    UNIQUE (room, seq, part),
    FOREIGN KEY (room, seq) REFERENCES orihon(room, seq),
    CHECK (part < parts AND byte_end >= byte_start)
);

CREATE OR REPLACE FUNCTION orihon_unit_require_source() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
DECLARE
    source orihon;
    body BYTEA;
    previous orihon_units;
BEGIN
    SELECT * INTO source FROM orihon WHERE room = NEW.room AND seq = NEW.seq;
    IF NOT FOUND OR source.content_digest <> NEW.record_digest
        OR NEW.semantic <> (source.fold_text IS NOT NULL)
    THEN
        RAISE EXCEPTION 'Orihon unit does not match its source record';
    END IF;
    body := convert_to(COALESCE(source.fold_text, source.text), 'UTF8');
    IF NEW.byte_end > octet_length(body)
        OR (NEW.part = 0 AND NEW.byte_start <> 0)
        OR (NEW.part = NEW.parts - 1 AND NEW.byte_end <> octet_length(body))
        OR encode(sha256(substring(body FROM NEW.byte_start + 1 FOR NEW.byte_end - NEW.byte_start)), 'hex')
            <> NEW.digest
    THEN
        RAISE EXCEPTION 'Orihon unit bytes do not tile its source record';
    END IF;
    IF NEW.unit = 0 THEN
        IF NEW.seq <> 0 OR NEW.part <> 0 THEN
            RAISE EXCEPTION 'Orihon unit 0 must start record 0';
        END IF;
        RETURN NEW;
    END IF;
    SELECT * INTO previous FROM orihon_units WHERE room = NEW.room AND unit = NEW.unit - 1;
    IF NOT FOUND
        OR (NEW.part = 0 AND (previous.seq <> NEW.seq - 1 OR previous.part <> previous.parts - 1))
        OR (NEW.part > 0 AND (previous.seq <> NEW.seq OR previous.part <> NEW.part - 1
            OR previous.parts <> NEW.parts OR previous.byte_end <> NEW.byte_start))
    THEN
        RAISE EXCEPTION 'Orihon units must be dense and contiguous in record order';
    END IF;
    RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS orihon_unit_source ON orihon_units;
CREATE TRIGGER orihon_unit_source BEFORE INSERT ON orihon_units
    FOR EACH ROW EXECUTE FUNCTION orihon_unit_require_source();

-- One row per finished compaction job, including failed and cancelled attempts.
CREATE TABLE IF NOT EXISTS orihon_jobs (
    id BIGSERIAL PRIMARY KEY,
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    level SMALLINT NOT NULL CHECK (level BETWEEN 0 AND 62),
    idx BIGINT NOT NULL CHECK (idx >= 0),
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'failed', 'cancelled')),
    backend TEXT NOT NULL CHECK (backend IN ('local', 'hosted')),
    provider TEXT NOT NULL CHECK (length(btrim(provider)) > 0),
    model TEXT NOT NULL CHECK (length(btrim(model)) > 0),
    cache_scope TEXT NOT NULL CHECK (cache_scope ~ '^[0-9a-f]{64}$'),
    prefix_digest TEXT NOT NULL CHECK (prefix_digest ~ '^[0-9a-f]{64}$'),
    context_digest TEXT NOT NULL CHECK (context_digest ~ '^[0-9a-f]{64}$'),
    context_lines INTEGER NOT NULL CHECK (context_lines >= 0),
    context_bytes INTEGER NOT NULL CHECK (context_bytes >= 0),
    input_bytes INTEGER NOT NULL CHECK (input_bytes >= 0),
    attempts JSONB NOT NULL CHECK (jsonb_typeof(attempts) = 'array'),
    input_tokens BIGINT CHECK (input_tokens >= 0),
    output_tokens BIGINT CHECK (output_tokens >= 0),
    cache_read_tokens BIGINT CHECK (cache_read_tokens >= 0),
    cache_write_tokens BIGINT CHECK (cache_write_tokens >= 0),
    cost_usd NUMERIC CHECK (cost_usd >= 0),
    cost_status TEXT NOT NULL CHECK (cost_status IN ('reported', 'unavailable')),
    cost_reason TEXT,
    error TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL,
    CHECK ((cost_status = 'reported') = (cost_usd IS NOT NULL)),
    CHECK ((cost_status = 'unavailable') = (cost_reason IS NOT NULL)),
    CHECK ((outcome = 'accepted') = (error IS NULL))
);
CREATE INDEX IF NOT EXISTS idx_orihon_jobs_node ON orihon_jobs(room, level, idx, id);

-- Immutable message-tree nodes bound to exact unit or child digests.
CREATE TABLE IF NOT EXISTS orihon_nodes (
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    level SMALLINT NOT NULL CHECK (level BETWEEN 0 AND 62),
    idx BIGINT NOT NULL CHECK (idx >= 0),
    text TEXT NOT NULL CHECK (octet_length(text) >= 1),
    bytes INTEGER GENERATED ALWAYS AS (octet_length(text)) STORED,
    method TEXT NOT NULL CHECK (method IN ('verbatim', 'joined', 'model')),
    source_digest TEXT NOT NULL CHECK (source_digest ~ '^[0-9a-f]{64}$'),
    digest TEXT NOT NULL CHECK (digest ~ '^[0-9a-f]{64}$'),
    job BIGINT REFERENCES orihon_jobs(id),
    built_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (room, level, idx),
    CHECK ((method = 'model') = (job IS NOT NULL)),
    CHECK (method <> 'verbatim' OR level = 0),
    CHECK (method <> 'joined' OR level > 0),
    -- 512 bytes is the target. After its size corrections the compactor keeps its shortest
    -- line, up to the 16 KiB reply ceiling; verbatim and joined lines fit the target.
    CONSTRAINT orihon_nodes_text_ceiling
        CHECK (octet_length(text) <= CASE WHEN method = 'model' THEN 16384 ELSE 512 END)
);

CREATE OR REPLACE FUNCTION orihon_node_require_sources() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
DECLARE
    left_digest TEXT;
    right_digest TEXT;
    expected TEXT;
BEGIN
    IF NEW.level = 0 THEN
        SELECT digest INTO expected FROM orihon_units WHERE room = NEW.room AND unit = NEW.idx;
    ELSE
        SELECT digest INTO left_digest FROM orihon_nodes
        WHERE room = NEW.room AND level = NEW.level - 1 AND idx = NEW.idx * 2;
        SELECT digest INTO right_digest FROM orihon_nodes
        WHERE room = NEW.room AND level = NEW.level - 1 AND idx = NEW.idx * 2 + 1;
        IF left_digest IS NOT NULL AND right_digest IS NOT NULL THEN
            expected := encode(sha256(convert_to(left_digest || ':' || right_digest, 'UTF8')), 'hex');
        END IF;
    END IF;
    IF expected IS NULL OR expected <> NEW.source_digest THEN
        RAISE EXCEPTION 'Orihon node does not match its exact unit or children';
    END IF;
    IF encode(sha256(convert_to(NEW.level::text || ':' || NEW.idx::text || ':' || NEW.source_digest
        || ':' || encode(sha256(convert_to(NEW.text, 'UTF8')), 'hex'), 'UTF8')), 'hex') <> NEW.digest
    THEN
        RAISE EXCEPTION 'Orihon node digest does not match its geometry, sources and text';
    END IF;
    IF NEW.job IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM orihon_jobs job
        WHERE job.id = NEW.job AND job.room = NEW.room AND job.level = NEW.level
            AND job.idx = NEW.idx AND job.outcome = 'accepted'
    ) THEN
        RAISE EXCEPTION 'Orihon model node must name its accepted job';
    END IF;
    RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS orihon_node_sources ON orihon_nodes;
CREATE TRIGGER orihon_node_sources BEFORE INSERT ON orihon_nodes
    FOR EACH ROW EXECUTE FUNCTION orihon_node_require_sources();

-- Exact stable prompt/tool prefixes supplied by the harness, addressed by the digest of their body.
CREATE TABLE IF NOT EXISTS orihon_prefixes (
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    digest TEXT NOT NULL CHECK (digest ~ '^[0-9a-f]{64}$'),
    body TEXT NOT NULL CHECK (encode(sha256(convert_to(body, 'UTF8')), 'hex') = digest),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (room, digest)
);

-- The room's live compactor configuration. The current prefix is the latest turn prefix.
CREATE TABLE IF NOT EXISTS orihon_live (
    room TEXT PRIMARY KEY CHECK (length(btrim(room)) > 0),
    folder JSONB CHECK (folder IS NULL OR jsonb_typeof(folder) = 'object'),
    prefix_digest TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (room, prefix_digest) REFERENCES orihon_prefixes(room, digest)
);

-- The persisted chat and compactor views: exact lines and batch state, never rebuilt from the tree.
CREATE TABLE IF NOT EXISTS orihon_views (
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    name TEXT NOT NULL CHECK (name IN ('chat', 'compactor')),
    revision BIGINT NOT NULL CHECK (revision >= 1),
    units BIGINT NOT NULL CHECK (units >= 0),
    merging BOOLEAN NOT NULL,
    lines JSONB NOT NULL CHECK (jsonb_typeof(lines) = 'array'),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (room, name)
);

CREATE OR REPLACE FUNCTION orihon_live_refuse_mutation() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Orihon live evidence is append-only';
END $$;

DROP TRIGGER IF EXISTS orihon_units_append_only ON orihon_units;
CREATE TRIGGER orihon_units_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON orihon_units
    FOR EACH STATEMENT EXECUTE FUNCTION orihon_live_refuse_mutation();
DROP TRIGGER IF EXISTS orihon_nodes_append_only ON orihon_nodes;
CREATE TRIGGER orihon_nodes_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON orihon_nodes
    FOR EACH STATEMENT EXECUTE FUNCTION orihon_live_refuse_mutation();
DROP TRIGGER IF EXISTS orihon_jobs_append_only ON orihon_jobs;
CREATE TRIGGER orihon_jobs_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON orihon_jobs
    FOR EACH STATEMENT EXECUTE FUNCTION orihon_live_refuse_mutation();
DROP TRIGGER IF EXISTS orihon_prefixes_append_only ON orihon_prefixes;
CREATE TRIGGER orihon_prefixes_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON orihon_prefixes
    FOR EACH STATEMENT EXECUTE FUNCTION orihon_live_refuse_mutation();

INSERT INTO schema_migrations(version) VALUES (41) ON CONFLICT(version) DO NOTHING;

COMMIT;
