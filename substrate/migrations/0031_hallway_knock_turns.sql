-- 0031: Hallway Knock turns — ceiling 20, default 10.
--
-- A four-turn exchange ended the first live Kodo–Tuner conversation exactly
-- when it got good. The knocker still picks the budget for one exchange and
-- the recipient's policy still caps it; only the ceiling and default move.
-- hearth::hallway::HALLWAY_MAX_KNOCK_TURNS and HALLWAY_DEFAULT_KNOCK_TURNS
-- must agree with the numbers here.
--
-- Re-applicable against fresh state. Each CHECK is dropped by its
-- Postgres-assigned name and recreated; a missing name refuses loudly.

BEGIN;

DO $$
BEGIN
    IF to_regclass('hallway_knock_policies') IS NULL THEN
        RAISE EXCEPTION 'hallway_knock_policies is missing; apply 0021 first';
    END IF;
    IF to_regclass('hallway_knocks') IS NULL THEN
        RAISE EXCEPTION 'hallway_knocks is missing; apply 0021 first';
    END IF;
END
$$;

ALTER TABLE hallway_knock_policies
    DROP CONSTRAINT hallway_knock_policies_max_turns_check,
    ADD CONSTRAINT hallway_knock_policies_max_turns_check
        CHECK (max_turns BETWEEN 1 AND 20),
    ALTER COLUMN max_turns SET DEFAULT 10;

ALTER TABLE hallway_knocks
    DROP CONSTRAINT hallway_knocks_turn_index_check,
    ADD CONSTRAINT hallway_knocks_turn_index_check
        CHECK (turn_index BETWEEN 1 AND 20),
    DROP CONSTRAINT hallway_knocks_max_turns_check,
    ADD CONSTRAINT hallway_knocks_max_turns_check
        CHECK (max_turns BETWEEN 1 AND 20);

COMMIT;
