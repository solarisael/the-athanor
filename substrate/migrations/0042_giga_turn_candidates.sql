BEGIN;

ALTER TABLE giga_events DROP CONSTRAINT IF EXISTS giga_events_candidate_count_check;
ALTER TABLE giga_events ADD CONSTRAINT giga_events_candidate_count_check
    CHECK (candidate_count BETWEEN 0 AND 8);

ALTER TABLE giga_event_attempts DROP CONSTRAINT IF EXISTS giga_event_attempts_candidate_count_check;
ALTER TABLE giga_event_attempts ADD CONSTRAINT giga_event_attempts_candidate_count_check
    CHECK (candidate_count BETWEEN 0 AND 8);

COMMIT;
