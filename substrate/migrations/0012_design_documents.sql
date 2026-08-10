BEGIN;

CREATE TABLE design_documents (
  id BIGSERIAL PRIMARY KEY,
  system TEXT NOT NULL,
  doc_type TEXT NOT NULL CHECK (doc_type IN ('token', 'component', 'contract', 'guideline')),
  name TEXT NOT NULL,
  group_name TEXT,
  "values" JSONB NOT NULL DEFAULT '{}',
  body TEXT NOT NULL,
  provenance JSONB NOT NULL DEFAULT '{}',
  tags TEXT[] NOT NULL DEFAULT '{}',
  superseded_by BIGINT NULL REFERENCES design_documents(id) ON DELETE SET NULL,
  search_tsv TSVECTOR GENERATED ALWAYS AS (
    to_tsvector('portuguese', name || ' ' || body)
  ) STORED,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER design_documents_updated_at BEFORE UPDATE ON design_documents
  FOR EACH ROW EXECUTE FUNCTION substrate_set_updated_at();

CREATE UNIQUE INDEX design_documents_current_identity_uidx
  ON design_documents (system, doc_type, name)
  WHERE superseded_by IS NULL;
CREATE INDEX design_documents_system_doc_type_idx
  ON design_documents (system, doc_type);
CREATE INDEX design_documents_search_tsv_gin
  ON design_documents USING GIN (search_tsv);
CREATE INDEX design_documents_tags_gin
  ON design_documents USING GIN (tags);

INSERT INTO schema_migrations (version) VALUES (12)
ON CONFLICT (version) DO NOTHING;

COMMIT;
