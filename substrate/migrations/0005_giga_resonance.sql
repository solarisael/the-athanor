BEGIN;

UPDATE giga_event_attempts
SET error_class = 'GigaLegacyUnclassifiedFailure'
WHERE outcome IN ('retry', 'failed', 'lease_expired')
  AND error_class IS NULL;

UPDATE giga_events
SET last_error = 'GigaLegacyUnclassifiedFailure'
WHERE queue_state = 'failed'
  AND last_error IS NULL;

ALTER TABLE giga_event_attempts
  DROP CONSTRAINT IF EXISTS giga_event_attempts_outcome_error_class_check;
ALTER TABLE giga_event_attempts
  ADD CONSTRAINT giga_event_attempts_outcome_error_class_check CHECK (
    (outcome IS NULL AND error_class IS NULL)
    OR (outcome = 'succeeded' AND error_class IS NULL)
    OR (outcome IN ('retry', 'failed', 'lease_expired') AND error_class IS NOT NULL)
  );

ALTER TABLE giga_events
  DROP CONSTRAINT IF EXISTS giga_events_failed_error_class_check;
ALTER TABLE giga_events
  ADD CONSTRAINT giga_events_failed_error_class_check CHECK (
    queue_state <> 'failed' OR last_error IS NOT NULL
  );

CREATE TABLE IF NOT EXISTS giga_review_resonances (
  review_id BIGINT PRIMARY KEY REFERENCES giga_reviews(id) ON DELETE CASCADE,
  candidate_id TEXT NOT NULL REFERENCES giga_candidates(candidate_id) ON DELETE CASCADE,
  event_id TEXT NOT NULL REFERENCES giga_events(event_id) ON DELETE RESTRICT,
  score DOUBLE PRECISION NOT NULL CHECK (
    score >= 0 AND score <= 1 AND score::TEXT NOT IN ('NaN', 'Infinity', '-Infinity')
  ),
  classifier_model TEXT NOT NULL,
  classifier_provider_type TEXT NOT NULL,
  classifier_model_version TEXT NOT NULL,
  classifier_prompt_version TEXT NOT NULL,
  classifier_configuration_digest TEXT NOT NULL CHECK (
    classifier_configuration_digest ~ '^[0-9a-f]{64}$'
  ),
  classifier_run_id TEXT NOT NULL,
  classifier_completed_at TIMESTAMPTZ NOT NULL,
  source_refs JSONB NOT NULL CHECK (
    jsonb_typeof(source_refs) = 'array' AND jsonb_array_length(source_refs) > 0
  )
);

CREATE INDEX IF NOT EXISTS giga_review_resonances_candidate_idx
  ON giga_review_resonances (candidate_id, review_id DESC);
CREATE INDEX IF NOT EXISTS giga_review_resonances_event_idx
  ON giga_review_resonances (event_id, review_id DESC);

INSERT INTO schema_migrations (version)
VALUES (5)
ON CONFLICT (version) DO NOTHING;

COMMIT;
