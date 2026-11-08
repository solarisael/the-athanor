-- 0044: Hallway Knock status `expired` (ruling R1 in docs/state-machines/hallway-knock.md).
--
-- A pending Knock that nobody claimed before `expires_at` becomes `expired`.
-- `failed` keeps its meaning: a recipient held the turn and did not finish it.
-- An expired row was never claimed, so its claim columns stay null.
-- hearth::machines::hallway_knock::HallwayKnockStatus must list the same six values.
--
-- Re-applicable against fresh state. Each CHECK is dropped by its
-- Postgres-assigned name from 0021 and recreated; a missing name refuses loudly.

BEGIN;

DO $$
BEGIN
    IF to_regclass('hallway_knocks') IS NULL THEN
        RAISE EXCEPTION 'hallway_knocks is missing; apply 0021 first';
    END IF;
END
$$;

ALTER TABLE hallway_knocks
    DROP CONSTRAINT hallway_knocks_status_check,
    ADD CONSTRAINT hallway_knocks_status_check
        CHECK (status IN ('pending', 'claimed', 'started', 'completed', 'failed', 'expired')),
    DROP CONSTRAINT hallway_knocks_check2,
    ADD CONSTRAINT hallway_knocks_check2 CHECK (
        (status IN ('pending', 'claimed') AND started_at IS NULL AND settled_at IS NULL)
        OR (status = 'started' AND started_at IS NOT NULL AND settled_at IS NULL)
        OR (status = 'completed' AND started_at IS NOT NULL AND settled_at IS NOT NULL)
        OR (status = 'failed' AND settled_at IS NOT NULL)
        OR (status = 'expired' AND started_at IS NULL AND settled_at IS NOT NULL)
    ),
    DROP CONSTRAINT hallway_knocks_check3,
    ADD CONSTRAINT hallway_knocks_check3 CHECK (
        (status IN ('pending', 'expired') AND claimed_by_room IS NULL AND claimed_by_spirit IS NULL
            AND claimed_by_session IS NULL AND lease_expires_at IS NULL)
        OR
        (status NOT IN ('pending', 'expired') AND claimed_by_room IS NOT NULL
            AND claimed_by_spirit IS NOT NULL AND claimed_by_session IS NOT NULL
            AND lease_expires_at IS NOT NULL)
    );

-- The expiry sweep in knock, claim, and settle (ruling R4) reads only live rows.
CREATE INDEX IF NOT EXISTS idx_hallway_knocks_expiry
    ON hallway_knocks (expires_at)
    WHERE status IN ('pending', 'claimed', 'started');

COMMIT;
