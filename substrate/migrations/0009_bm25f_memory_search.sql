BEGIN;

CREATE OR REPLACE FUNCTION substrate_bm25f_token_count(input TEXT)
RETURNS INTEGER
LANGUAGE SQL
IMMUTABLE
PARALLEL SAFE
AS $$
  SELECT count(*)::integer
  FROM regexp_matches(lower(coalesce(input, '')), '[[:alnum:]_:+#./-]+', 'g');
$$;

-- PostgreSQL marks array_to_string as stable rather than immutable. Threads are
-- stored text and this delimiter is fixed, so the result is deterministic; the
-- narrow wrapper lets generated columns state that contract explicitly.
CREATE OR REPLACE FUNCTION substrate_bm25f_array_text(input TEXT[])
RETURNS TEXT
LANGUAGE SQL
IMMUTABLE
PARALLEL SAFE
AS $$
  SELECT array_to_string(coalesce(input, '{}'::text[]), ' ');
$$;

ALTER TABLE memories
  ADD COLUMN IF NOT EXISTS bm25f_meta_tsv TSVECTOR GENERATED ALWAYS AS (
    setweight(to_tsvector('simple', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('simple', source_path), 'B') ||
    setweight(to_tsvector('simple', substrate_bm25f_array_text(threads)), 'B') ||
    setweight(to_tsvector('simple', type), 'D')
  ) STORED,
  ADD COLUMN IF NOT EXISTS bm25f_title_length INTEGER GENERATED ALWAYS AS (
    substrate_bm25f_token_count(coalesce(title, ''))
  ) STORED,
  ADD COLUMN IF NOT EXISTS bm25f_source_path_length INTEGER GENERATED ALWAYS AS (
    substrate_bm25f_token_count(source_path)
  ) STORED,
  ADD COLUMN IF NOT EXISTS bm25f_threads_length INTEGER GENERATED ALWAYS AS (
    substrate_bm25f_token_count(substrate_bm25f_array_text(threads))
  ) STORED,
  ADD COLUMN IF NOT EXISTS bm25f_type_length INTEGER GENERATED ALWAYS AS (
    substrate_bm25f_token_count(type)
  ) STORED;

CREATE INDEX IF NOT EXISTS memories_bm25f_meta_tsv_gin
  ON memories USING GIN (bm25f_meta_tsv);

ALTER TABLE memory_chunks
  ADD COLUMN IF NOT EXISTS bm25f_text_tsv TSVECTOR GENERATED ALWAYS AS (
    setweight(to_tsvector('simple', coalesce(heading_path, '')), 'A') ||
    setweight(to_tsvector('simple', body), 'D')
  ) STORED,
  ADD COLUMN IF NOT EXISTS bm25f_heading_length INTEGER GENERATED ALWAYS AS (
    substrate_bm25f_token_count(coalesce(heading_path, ''))
  ) STORED,
  ADD COLUMN IF NOT EXISTS bm25f_body_length INTEGER GENERATED ALWAYS AS (
    substrate_bm25f_token_count(body)
  ) STORED;

CREATE INDEX IF NOT EXISTS memory_chunks_bm25f_text_tsv_gin
  ON memory_chunks USING GIN (bm25f_text_tsv);

COMMIT;
