BEGIN;

CREATE TABLE IF NOT EXISTS giga_events (
  event_schema_version INTEGER NOT NULL CHECK (event_schema_version = 1),
  event_id TEXT PRIMARY KEY,
  event_type TEXT NOT NULL CHECK (event_type IN (
    'conversation_window', 'task_started', 'task_completed',
    'subagent_dispatched', 'subagent_completed', 'todo_transition',
    'tool_outcome', 'manual_reprocess'
  )),
  room TEXT,
  session_id TEXT NOT NULL,
  project_keys TEXT[] NOT NULL DEFAULT '{}',
  lifecycle JSONB NOT NULL DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL,
  queue_state TEXT NOT NULL DEFAULT 'pending' CHECK (queue_state IN ('pending', 'running', 'succeeded', 'failed')),
  retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
  available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  locked_at TIMESTAMPTZ,
  last_error TEXT,
  processed_at TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (room IS NULL OR room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$')
);

CREATE TABLE IF NOT EXISTS giga_event_sources (
  id BIGSERIAL PRIMARY KEY,
  event_id TEXT NOT NULL REFERENCES giga_events(event_id) ON DELETE CASCADE,
  source_type TEXT NOT NULL CHECK (source_type IN ('turn', 'lifecycle_event', 'tool_result_summary', 'task_contract')),
  source_id TEXT NOT NULL,
  content_hash TEXT NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
  scope_room TEXT,
  scope_project TEXT,
  scope_visibility TEXT NOT NULL CHECK (scope_visibility IN ('private', 'shared')),
  publication_review_required BOOLEAN NOT NULL,
  range_start INTEGER,
  range_end INTEGER,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (event_id, source_type, source_id),
  CHECK (scope_room IS NULL OR scope_room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$'),
  CHECK ((range_start IS NULL AND range_end IS NULL) OR (range_start >= 0 AND range_end >= range_start))
);

CREATE TABLE IF NOT EXISTS giga_candidates (
  candidate_schema_version INTEGER NOT NULL CHECK (candidate_schema_version = 1),
  candidate_id TEXT PRIMARY KEY,
  event_id TEXT NOT NULL REFERENCES giga_events(event_id) ON DELETE CASCADE,
  room TEXT,
  session_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('memory', 'coding_lesson', 'project_lesson', 'correction', 'supersession', 'entity_update', 'thread_update')),
  priority DOUBLE PRECISION NOT NULL CHECK (priority >= 0 AND priority <= 1 AND priority::TEXT NOT IN ('NaN', 'Infinity', '-Infinity')),
  novelty DOUBLE PRECISION NOT NULL CHECK (novelty >= 0 AND novelty <= 1 AND novelty::TEXT NOT IN ('NaN', 'Infinity', '-Infinity')),
  durability DOUBLE PRECISION NOT NULL CHECK (durability >= 0 AND durability <= 1 AND durability::TEXT NOT IN ('NaN', 'Infinity', '-Infinity')),
  confidence DOUBLE PRECISION NOT NULL CHECK (confidence >= 0 AND confidence <= 1 AND confidence::TEXT NOT IN ('NaN', 'Infinity', '-Infinity')),
  project_keys TEXT[] NOT NULL DEFAULT '{}',
  thread_keys TEXT[] NOT NULL DEFAULT '{}',
  entity_hints TEXT[] NOT NULL DEFAULT '{}',
  retrieval_terms TEXT[] NOT NULL DEFAULT '{}',
  proposed_title TEXT NOT NULL DEFAULT '',
  gist TEXT NOT NULL DEFAULT '',
  rationale TEXT NOT NULL DEFAULT '',
  proof_refs JSONB NOT NULL DEFAULT '[]',
  scope_room TEXT,
  scope_project TEXT,
  scope_visibility TEXT NOT NULL CHECK (scope_visibility IN ('private', 'shared')),
  publication_review_required BOOLEAN NOT NULL,
  authority TEXT NOT NULL DEFAULT 'pointer_only' CHECK (authority = 'pointer_only'),
  review_state TEXT NOT NULL DEFAULT 'unreviewed' CHECK (review_state IN (
    'unreviewed', 'in_review', 'promoted', 'merged', 'corrected',
    'dismissed', 'unresolved', 'curio', 'expired', 'superseded'
  )),
  classifier_model TEXT NOT NULL,
  classifier_provider_type TEXT NOT NULL,
  classifier_model_version TEXT NOT NULL,
  classifier_prompt_version TEXT NOT NULL,
  classifier_configuration_digest TEXT NOT NULL,
  classifier_run_id TEXT NOT NULL,
  classifier_completed_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  expires_at TIMESTAMPTZ,
  successor_candidate_id TEXT REFERENCES giga_candidates(candidate_id) ON DELETE SET NULL,
  promotion_refs JSONB NOT NULL DEFAULT '[]',
  CHECK (room IS NULL OR room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$'),
  CHECK (scope_room IS NULL OR scope_room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$'),
  CHECK (proof_refs IS NULL OR jsonb_typeof(proof_refs) = 'array'),
  CHECK (promotion_refs IS NULL OR jsonb_typeof(promotion_refs) = 'array')
);

CREATE TABLE IF NOT EXISTS giga_candidate_sources (
  candidate_id TEXT NOT NULL REFERENCES giga_candidates(candidate_id) ON DELETE CASCADE,
  event_id TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK (source_type IN ('turn', 'lifecycle_event', 'tool_result_summary', 'task_contract')),
  source_id TEXT NOT NULL,
  content_hash TEXT NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
  scope_room TEXT,
  scope_project TEXT,
  scope_visibility TEXT NOT NULL CHECK (scope_visibility IN ('private', 'shared')),
  publication_review_required BOOLEAN NOT NULL,
  range_start INTEGER,
  range_end INTEGER,
  is_proof BOOLEAN NOT NULL DEFAULT FALSE,
  PRIMARY KEY (candidate_id, source_type, source_id),
  FOREIGN KEY (event_id, source_type, source_id)
    REFERENCES giga_event_sources(event_id, source_type, source_id) ON DELETE CASCADE,
  CHECK (scope_room IS NULL OR scope_room ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$'),
  CHECK ((range_start IS NULL AND range_end IS NULL) OR (range_start >= 0 AND range_end >= range_start))
);

CREATE TABLE IF NOT EXISTS giga_reviews (
  id BIGSERIAL PRIMARY KEY,
  candidate_id TEXT NOT NULL REFERENCES giga_candidates(candidate_id) ON DELETE CASCADE,
  action TEXT NOT NULL CHECK (action IN ('start_review', 'promote', 'merge', 'correct', 'dismiss', 'resolve', 'curio', 'expire', 'supersede')),
  reviewer_principal TEXT NOT NULL,
  authorization_basis TEXT NOT NULL,
  previous_state TEXT NOT NULL CHECK (previous_state IN (
    'unreviewed', 'in_review', 'promoted', 'merged', 'corrected',
    'dismissed', 'unresolved', 'curio', 'expired', 'superseded'
  )),
  new_state TEXT NOT NULL CHECK (new_state IN (
    'unreviewed', 'in_review', 'promoted', 'merged', 'corrected',
    'dismissed', 'unresolved', 'curio', 'expired', 'superseded'
  )),
  reason TEXT NOT NULL,
  promotion_target JSONB,
  merge_targets JSONB,
  target_refs JSONB NOT NULL DEFAULT '[]',
  reviewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CHECK (promotion_target IS NULL OR jsonb_typeof(promotion_target) = 'object'),
  CHECK (merge_targets IS NULL OR jsonb_typeof(merge_targets) = 'array'),
  CHECK (jsonb_typeof(target_refs) = 'array')
);

CREATE INDEX IF NOT EXISTS giga_events_pending_work_idx
  ON giga_events (available_at, created_at)
  WHERE queue_state IN ('pending', 'failed');
CREATE INDEX IF NOT EXISTS giga_event_sources_lookup_idx
  ON giga_event_sources (source_type, source_id, content_hash);
CREATE INDEX IF NOT EXISTS giga_candidates_room_state_created_idx
  ON giga_candidates (room, review_state, created_at DESC);
CREATE INDEX IF NOT EXISTS giga_candidates_session_idx
  ON giga_candidates (session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS giga_candidates_event_idx
  ON giga_candidates (event_id, created_at DESC);
CREATE INDEX IF NOT EXISTS giga_candidate_sources_lookup_idx
  ON giga_candidate_sources (source_type, source_id, content_hash);
CREATE INDEX IF NOT EXISTS giga_reviews_history_idx
  ON giga_reviews (candidate_id, reviewed_at DESC, id DESC);

INSERT INTO schema_migrations (version)
VALUES (3)
ON CONFLICT (version) DO NOTHING;

COMMIT;
