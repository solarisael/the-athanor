-- 0033: Native OMP fires land in the ledger. OMP reports the rule that fired,
-- never which of its patterns matched, so the pattern column admits NULL when
-- a rule carries several patterns of the matched kind. Existing rows keep
-- their patterns; the substrate matcher still writes one every time.

BEGIN;

ALTER TABLE lesson_trigger_events ALTER COLUMN matched_pattern DROP NOT NULL;

COMMIT;
