-- 0032: Seven days of raw Insula observations; historical deletion proofs stay intact.
-- Expiry changes do not delete observations. The scheduled, coverage-guarded sweep
-- remains the only deletion path, and old 14-day receipts keep their original policy.

BEGIN;

-- Hours keep seven UTC days exact even when the connection crosses local DST.

ALTER TABLE insula.log DROP CONSTRAINT IF EXISTS insula_log_expiry_check;

UPDATE insula.log
   SET expires_at = observed_at + INTERVAL '168 hours'
 WHERE expires_at IS DISTINCT FROM observed_at + INTERVAL '168 hours';

ALTER TABLE insula.log
    ADD CONSTRAINT insula_log_expiry_check
    CHECK (expires_at = observed_at + INTERVAL '168 hours');

ALTER TABLE insula.retention_receipts
    DROP CONSTRAINT IF EXISTS insula_retention_receipts_retention_days_check;
ALTER TABLE insula.retention_receipts
    ADD CONSTRAINT insula_retention_receipts_retention_days_check
    CHECK (retention_days IN (7, 14));

DO $$
BEGIN
    IF to_regclass('room_settings') IS NOT NULL THEN
        UPDATE room_settings
           SET value = '7'::jsonb
         WHERE key = 'insula_retention_days'
           AND value = '14'::jsonb;
    END IF;
END $$;

COMMIT;
