BEGIN;

ALTER TABLE orihon_folds ADD COLUMN IF NOT EXISTS children JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE orihon_folds DROP CONSTRAINT IF EXISTS orihon_folds_level_check;
ALTER TABLE orihon_folds ADD CONSTRAINT orihon_folds_level_check CHECK (level BETWEEN 0 AND 62);

-- Retain the existing message/page source checks without flattening branch ancestry.
DROP TRIGGER IF EXISTS orihon_fold_sources ON orihon_folds;
CREATE TRIGGER orihon_fold_sources BEFORE INSERT ON orihon_folds
    FOR EACH ROW WHEN (NEW.level < 2) EXECUTE FUNCTION orihon_fold_require_sources();

CREATE OR REPLACE FUNCTION orihon_fold_require_children() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
DECLARE
    ref JSONB;
    child_level SMALLINT;
    child_index BIGINT;
    ordinal BIGINT;
    width NUMERIC;
    encoded_refs TEXT;
BEGIN
    IF NEW.level NOT BETWEEN 0 AND 62 OR NEW.fold_index < 0 THEN
        RAISE EXCEPTION 'Orihon fold level or index is invalid';
    END IF;
    IF NEW.level >= 1 THEN
        width := power(2::numeric, NEW.level - 1);
        IF NEW.fold_index::numeric * width + width - 1 > 9223372036854775807::numeric THEN
            RAISE EXCEPTION 'Orihon fold page interval overflows';
        END IF;
    END IF;
    IF jsonb_typeof(NEW.children) IS DISTINCT FROM 'array' THEN
        RAISE EXCEPTION 'Orihon fold children must be an array';
    END IF;
    IF NEW.level < 2 THEN
        IF jsonb_array_length(NEW.children) <> 0 THEN
            RAISE EXCEPTION 'Orihon message and page folds cannot have children';
        END IF;
        RETURN NEW;
    END IF;
    IF NEW.sources IS DISTINCT FROM '[]'::jsonb OR jsonb_array_length(NEW.children) <> 2 THEN
        RAISE EXCEPTION 'Orihon branches require two children and no raw sources';
    END IF;
    FOR ref, ordinal IN
        SELECT value, ord FROM jsonb_array_elements(NEW.children) WITH ORDINALITY AS refs(value, ord)
    LOOP
        IF jsonb_typeof(ref) <> 'object'
            OR ref - ARRAY['level', 'index', 'sourceDigest', 'recipeDigest'] <> '{}'::jsonb
            OR jsonb_typeof(ref->'level') IS DISTINCT FROM 'number'
            OR (ref->>'level') !~ '^[0-9]+$'
            OR jsonb_typeof(ref->'index') IS DISTINCT FROM 'number'
            OR (ref->>'index') !~ '^[0-9]+$'
            OR jsonb_typeof(ref->'sourceDigest') IS DISTINCT FROM 'string'
            OR (ref->>'sourceDigest') !~ '^[0-9a-f]{64}$'
            OR jsonb_typeof(ref->'recipeDigest') IS DISTINCT FROM 'string'
            OR (ref->>'recipeDigest') !~ '^[0-9a-f]{64}$'
        THEN
            RAISE EXCEPTION 'Orihon branch child ref is malformed';
        END IF;
        child_level := (ref->>'level')::smallint;
        child_index := (ref->>'index')::bigint;
        IF child_level <> NEW.level - 1
            OR child_index::numeric <> NEW.fold_index::numeric * 2 + ordinal - 1
        THEN
            RAISE EXCEPTION 'Orihon branch child geometry does not match its parent';
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM orihon_folds child
            WHERE child.room = NEW.room AND child.level = child_level AND child.fold_index = child_index
                AND child.source_digest = ref->>'sourceDigest'
                AND child.recipe_digest = ref->>'recipeDigest'
        ) THEN
            RAISE EXCEPTION 'Orihon branch references a missing exact child revision';
        END IF;
    END LOOP;
    SELECT string_agg('{"level":' || (value->>'level')::smallint::text
        || ',"index":' || (value->>'index')::bigint::text
        || ',"sourceDigest":"' || (value->>'sourceDigest')
        || '","recipeDigest":"' || (value->>'recipeDigest') || '"}', ',' ORDER BY ord)
    INTO encoded_refs
    FROM jsonb_array_elements(NEW.children) WITH ORDINALITY AS refs(value, ord);
    IF encode(sha256(convert_to('[' || encoded_refs || ']', 'UTF8')), 'hex') <> NEW.source_digest THEN
        RAISE EXCEPTION 'Orihon branch source digest does not match its ordered child refs';
    END IF;
    RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS orihon_fold_children ON orihon_folds;
CREATE TRIGGER orihon_fold_children BEFORE INSERT ON orihon_folds
    FOR EACH ROW EXECUTE FUNCTION orihon_fold_require_children();

-- Keep the single append owner and include ancestry in exact replay comparison.
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
    SELECT * INTO STRICT accepted FROM orihon_folds
    WHERE room = proposed.room AND level = proposed.level AND fold_index = proposed.fold_index
        AND source_digest = proposed.source_digest AND recipe_digest = proposed.recipe_digest;
    IF ROW(accepted.sources, accepted.children, accepted.text, accepted.model, accepted.tries)
        IS DISTINCT FROM ROW(proposed.sources, proposed.children, proposed.text, proposed.model, proposed.tries)
    THEN
        RAISE EXCEPTION 'Orihon fold identity was reused with different content';
    END IF;
    RETURN accepted;
END $$;

INSERT INTO schema_migrations(version) VALUES (40) ON CONFLICT(version) DO NOTHING;

COMMIT;
