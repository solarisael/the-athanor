BEGIN;

CREATE TABLE IF NOT EXISTS orihon_folds (
    room TEXT NOT NULL CHECK (length(btrim(room)) > 0),
    level SMALLINT NOT NULL CHECK (level IN (0, 1)),
    fold_index BIGINT NOT NULL CHECK (fold_index >= 0),
    source_digest TEXT NOT NULL CHECK (source_digest ~ '^[0-9a-f]{64}$'),
    recipe_digest TEXT NOT NULL CHECK (recipe_digest ~ '^[0-9a-f]{64}$'),
    sources JSONB NOT NULL CHECK (jsonb_typeof(sources) = 'array'),
    text TEXT NOT NULL CHECK (octet_length(text) BETWEEN 1 AND 512),
    model TEXT,
    tries SMALLINT NOT NULL,
    built_at TEXT NOT NULL,
    PRIMARY KEY (room, level, fold_index, source_digest, recipe_digest),
    CHECK ((model IS NULL AND tries = 0)
        OR (model IS NOT NULL AND length(btrim(model)) > 0 AND tries BETWEEN 1 AND 5))
);

CREATE OR REPLACE FUNCTION orihon_fold_require_sources() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
DECLARE
    ref JSONB;
    source_seq BIGINT;
    previous_seq BIGINT := -1;
    source_count INTEGER;
    frontier_count BIGINT;
    encoded_refs TEXT;
BEGIN
    IF jsonb_typeof(NEW.sources) <> 'array' THEN
        RAISE EXCEPTION 'Orihon fold sources must be an array';
    END IF;
    source_count := jsonb_array_length(NEW.sources);

    IF source_count NOT BETWEEN 1 AND 4096 THEN
        RAISE EXCEPTION 'Orihon folds require 1 through 4096 source refs';
    END IF;

    FOR ref IN SELECT value FROM jsonb_array_elements(NEW.sources) LOOP
        IF jsonb_typeof(ref) <> 'object'
            OR ref - ARRAY['seq', 'contentDigest'] <> '{}'::jsonb
            OR jsonb_typeof(ref->'seq') IS DISTINCT FROM 'number'
            OR (ref->>'seq') !~ '^[0-9]+$'
            OR jsonb_typeof(ref->'contentDigest') IS DISTINCT FROM 'string'
            OR (ref->>'contentDigest') !~ '^[0-9a-f]{64}$'
        THEN
            RAISE EXCEPTION 'Orihon fold source ref is malformed';
        END IF;
        source_seq := (ref->>'seq')::bigint;

        IF source_seq <= previous_seq THEN
            RAISE EXCEPTION 'Orihon fold source seq must be sorted, unique and nonnegative';
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM orihon source
            WHERE source.room = NEW.room AND source.seq = source_seq
                AND source.content_digest = ref->>'contentDigest'
                AND (NEW.level = 0 OR source.turn_no = NEW.fold_index)
        ) THEN
            RAISE EXCEPTION 'Orihon fold source digest or page does not match the source record';
        END IF;

        previous_seq := source_seq;
    END LOOP;

    IF NEW.level = 0 AND (source_count <> 1 OR previous_seq <> NEW.fold_index) THEN
        RAISE EXCEPTION 'Orihon message fold must reference exactly its indexed source';
    END IF;
    IF NEW.level = 1 THEN
        SELECT count(*) INTO frontier_count FROM orihon
        WHERE room = NEW.room AND turn_no = NEW.fold_index AND seq <= previous_seq;

        IF frontier_count <> source_count THEN
            RAISE EXCEPTION 'Orihon page fold must include every source through its source frontier';
        END IF;
    END IF;

    SELECT string_agg('{"seq":' || (value->>'seq')::bigint::text
        || ',"contentDigest":"' || (value->>'contentDigest') || '"}', ',' ORDER BY ordinal)
    INTO encoded_refs
    FROM jsonb_array_elements(NEW.sources) WITH ORDINALITY AS refs(value, ordinal);

    IF encode(sha256(convert_to('[' || encoded_refs || ']', 'UTF8')), 'hex') <> NEW.source_digest THEN
        RAISE EXCEPTION 'Orihon fold source digest does not match its ordered refs';
    END IF;

    RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION orihon_fold_refuse_mutation() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Orihon folds are append-only derived evidence';
END $$;

DROP TRIGGER IF EXISTS orihon_fold_sources ON orihon_folds;
CREATE TRIGGER orihon_fold_sources BEFORE INSERT ON orihon_folds
    FOR EACH ROW EXECUTE FUNCTION orihon_fold_require_sources();

DROP TRIGGER IF EXISTS orihon_folds_append_only ON orihon_folds;
CREATE TRIGGER orihon_folds_append_only BEFORE UPDATE OR DELETE OR TRUNCATE ON orihon_folds
    FOR EACH STATEMENT EXECUTE FUNCTION orihon_fold_refuse_mutation();

CREATE OR REPLACE FUNCTION orihon_fold_append(proposed orihon_folds) RETURNS orihon_folds
LANGUAGE plpgsql AS $$
DECLARE
    accepted orihon_folds;
BEGIN
    INSERT INTO orihon_folds SELECT proposed.*
    ON CONFLICT (room, level, fold_index, source_digest, recipe_digest) DO NOTHING
    RETURNING * INTO accepted;

    IF FOUND THEN
        RETURN accepted;
    END IF;

    -- The INSERT waits for a concurrent incumbent. This statement gets a new snapshot.
    SELECT * INTO STRICT accepted FROM orihon_folds
    WHERE room = proposed.room AND level = proposed.level AND fold_index = proposed.fold_index
        AND source_digest = proposed.source_digest AND recipe_digest = proposed.recipe_digest;

    IF ROW(accepted.sources, accepted.text, accepted.model, accepted.tries)
        IS DISTINCT FROM ROW(proposed.sources, proposed.text, proposed.model, proposed.tries)
    THEN
        RAISE EXCEPTION 'Orihon fold identity was reused with different content';
    END IF;

    RETURN accepted;
END $$;

INSERT INTO schema_migrations(version) VALUES (39) ON CONFLICT(version) DO NOTHING;

COMMIT;
