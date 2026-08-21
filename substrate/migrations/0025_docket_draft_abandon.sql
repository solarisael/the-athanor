-- 0025: Docket draft abandon — a poster may cancel its own draft.
--
-- The activation-freeze check refused draft -> cancelled: a draft carries no
-- frozen tuple, so the steward could not clean up a mistyped or polluting
-- draft without hand-editing the ledger's own law. Cancelled keeps that
-- freedom; every other state still demands the frozen tuple.
--
-- Re-applicable against fresh state.

BEGIN;

ALTER TABLE docket.quests DROP CONSTRAINT IF EXISTS docket_quests_activation_freeze_check;

ALTER TABLE docket.quests ADD CONSTRAINT docket_quests_activation_freeze_check
    CHECK (
        state IN ('draft', 'cancelled')
        OR (
            intent_authority_principal IS NOT NULL
            AND acceptance_policy IS NOT NULL
            AND acceptance_policy_digest IS NOT NULL
            AND review_class IS NOT NULL
            AND activated_at IS NOT NULL
        )
    );

COMMIT;
