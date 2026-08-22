use athanor_substrate::{AppError, QuestReportAction, QuestReportParams, quest_report};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const DOCKET_MIGRATION: &str = include_str!("../../../substrate/migrations/0023_docket.sql");
const CAPABILITY_MIGRATION: &str =
    include_str!("../../../substrate/migrations/0024_docket_capability.sql");
const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("Docket proof requires a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

async fn fresh_docket() -> TestResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&isolated_database_url())
        .await?;
    // This proof owns only the dedicated test database and deliberately starts
    // each migration assertion from the same pre-Docket state.
    sqlx::query("DROP SCHEMA IF EXISTS docket CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(DOCKET_MIGRATION).execute(&pool).await?;
    Ok(pool)
}

async fn insert_frozen_quest(pool: &PgPool) -> TestResult<String> {
    Ok(sqlx::query_scalar(
        "INSERT INTO docket.quests (
            house_id, kind, title, body, authority_ceiling,
            posted_by_room, posted_by_spirit,
            intent_authority_principal, acceptance_policy,
            acceptance_policy_digest, review_class, state, activated_at
         )
         VALUES (
            'test-house', 'maintenance', 'contract quest', 'bounded proof', 'operator',
            'tuner', 'Tuner',
            'test-authority', $1::jsonb, $2, 'R1', 'offered', NOW()
         )
         RETURNING quest_id::text",
    )
    .bind(r#"{"mode":"contract"}"#)
    .bind(SHA256)
    .fetch_one(pool)
    .await?)
}

async fn insert_attempt(
    pool: &PgPool,
    quest_id: &str,
    claim_epoch: i64,
    idempotency_key: &str,
    lease_token_hash: &str,
) -> TestResult {
    sqlx::query(
        "INSERT INTO docket.quest_attempts (
            quest_id, claim_epoch, quest_revision, claimant_room, claimant_spirit,
            session_id, lease_token_hash, lease_expires_at, idempotency_key
         )
         VALUES ($1::uuid, $2, 0, 'test-room', 'test-spirit',
                 'test-session', $3, NOW() + INTERVAL '1 hour', $4)",
    )
    .bind(quest_id)
    .bind(claim_epoch)
    .bind(lease_token_hash)
    .bind(idempotency_key)
    .execute(pool)
    .await?;
    Ok(())
}

// Kills: a re-applicable migration that leaves an unexpected table behind, or
// fails when it is applied over its own completed Docket v1 schema.
// red-proof: append `CREATE TABLE docket.leftover (id integer)` to 0023, or
// replace a `CREATE ... IF NOT EXISTS` relation declaration with `CREATE`.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_migration_is_idempotent_and_residual_free() -> TestResult {
    let pool = fresh_docket().await?;
    sqlx::raw_sql(DOCKET_MIGRATION).execute(&pool).await?;

    let residuals: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'docket' AND c.relkind = 'r'
           AND c.relname NOT IN (
               'goals', 'quests', 'quest_dependencies', 'quest_attempts',
               'quest_acceptance_items', 'quest_events', 'quest_receipts'
           )",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        residuals.is_empty(),
        "migration left residual relations: {residuals:?}"
    );
    Ok(())
}

// Kills: the event ledger admitting UPDATE or DELETE after an event has been
// recorded, rather than enforcing the docket_quest_events_append_only trigger.
// red-proof: drop docket_quest_events_append_only or remove UPDATE from it.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_events_are_append_only() -> TestResult {
    let pool = fresh_docket().await?;
    let quest_id = insert_frozen_quest(&pool).await?;
    let event_id: String = sqlx::query_scalar(
        "INSERT INTO docket.quest_events (quest_id, event_kind, principal)
         VALUES ($1::uuid, 'posted', 'test-principal')
         RETURNING event_id::text",
    )
    .bind(&quest_id)
    .fetch_one(&pool)
    .await?;

    let update = sqlx::query(
        "UPDATE docket.quest_events SET event_kind = 'edited' WHERE event_id = $1::uuid",
    )
    .bind(&event_id)
    .execute(&pool)
    .await;
    assert!(update.is_err(), "append-only ledger accepted UPDATE");
    assert!(
        update
            .expect_err("checked above")
            .to_string()
            .contains("append-only"),
        "UPDATE refusal must come from docket_quest_events_append_only"
    );

    let delete = sqlx::query("DELETE FROM docket.quest_events WHERE event_id = $1::uuid")
        .bind(&event_id)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "append-only ledger accepted DELETE");
    assert!(
        delete
            .expect_err("checked above")
            .to_string()
            .contains("append-only"),
        "DELETE refusal must come from docket_quest_events_append_only"
    );
    Ok(())
}

// Kills: an executor role being accepted as a settlement principal, or a
// non-pending verdict being accepted without its accountable settlement tuple.
// red-proof: add 'executor' to docket_quest_acceptance_items_role_check, or
// remove the principal-and-time conjuncts from settlement_check.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_executor_never_self_settles_acceptance() -> TestResult {
    let pool = fresh_docket().await?;
    let quest_id = insert_frozen_quest(&pool).await?;

    let executor = sqlx::query(
        "INSERT INTO docket.quest_acceptance_items
         (quest_id, position, criterion, verdict, settled_by_role, settled_by_room, settled_by_spirit, settled_at)
         VALUES ($1::uuid, 1, 'reviewed', 'met', 'executor', 'test-room', 'test-spirit', NOW())",
    )
    .bind(&quest_id)
    .execute(&pool)
    .await;
    assert!(
        executor.is_err(),
        "executor settled its own acceptance item"
    );
    assert!(
        executor
            .expect_err("checked above")
            .to_string()
            .contains("docket_quest_acceptance_items_role_check"),
        "executor refusal must name role_check"
    );

    let missing_principals = sqlx::query(
        "INSERT INTO docket.quest_acceptance_items (quest_id, position, criterion, verdict)
         VALUES ($1::uuid, 2, 'reviewed', 'met')",
    )
    .bind(&quest_id)
    .execute(&pool)
    .await;
    assert!(
        missing_principals.is_err(),
        "met verdict omitted settlement principals"
    );
    assert!(
        missing_principals
            .expect_err("checked above")
            .to_string()
            .contains("docket_quest_acceptance_items_settlement_check"),
        "missing-principal refusal must name settlement_check"
    );

    sqlx::query(
        "INSERT INTO docket.quest_acceptance_items
         (quest_id, position, criterion, verdict, settled_by_role, settled_by_room, settled_by_spirit, settled_at)
         VALUES ($1::uuid, 3, 'reviewed', 'met', 'reviewer', 'review-room', 'reviewer', NOW())",
    )
    .bind(&quest_id)
    .execute(&pool)
    .await?;
    Ok(())
}

// Kills: activation letting an offered quest omit any frozen authority,
// acceptance digest, review class, or activation timestamp while draft stays editable.
// red-proof: replace the activation_freeze_check body with `CHECK (TRUE)`.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_activation_freezes_the_full_authority_tuple() -> TestResult {
    let pool = fresh_docket().await?;
    sqlx::query(
        "INSERT INTO docket.quests (house_id, kind, title, body, authority_ceiling,
                                    posted_by_room, posted_by_spirit, state)
         VALUES ('test-house', 'maintenance', 'draft remains editable', 'body', 'operator',
                 'tuner', 'Tuner', 'draft')",
    )
    .execute(&pool)
    .await?;

    for (title, digest, review_class) in [
        ("missing digest", None, Some("R1")),
        ("missing review class", Some(SHA256), None),
    ] {
        let refused = sqlx::query(
            "INSERT INTO docket.quests (
                house_id, kind, title, body, authority_ceiling,
                posted_by_room, posted_by_spirit,
                intent_authority_principal, acceptance_policy,
                acceptance_policy_digest, review_class, state, activated_at
             ) VALUES (
                'test-house', 'maintenance', $1, 'body', 'operator',
                'tuner', 'Tuner',
                'test-authority', $2::jsonb, $3, $4, 'offered', NOW()
             )",
        )
        .bind(title)
        .bind(r#"{"mode":"contract"}"#)
        .bind(digest)
        .bind(review_class)
        .execute(&pool)
        .await;
        assert!(refused.is_err(), "offered quest {title} was accepted");
        assert!(
            refused
                .expect_err("checked above")
                .to_string()
                .contains("docket_quests_activation_freeze_check"),
            "offered quest {title} must fail activation_freeze_check"
        );
    }

    insert_frozen_quest(&pool).await?;
    Ok(())
}

// Kills: a reclaim fence allowing a second attempt in the same claim epoch or
// allowing a claim request to replay its quest-scoped idempotency key.
// red-proof: drop docket_quest_attempts_epoch_key or idempotency_key.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_attempts_fence_claim_epoch_and_idempotency() -> TestResult {
    let pool = fresh_docket().await?;
    let quest_id = insert_frozen_quest(&pool).await?;
    insert_attempt(&pool, &quest_id, 1, "claim-one", SHA256).await?;

    let same_epoch = insert_attempt(&pool, &quest_id, 1, "claim-two", SHA256).await;
    assert!(same_epoch.is_err(), "second attempt reused claim epoch");
    assert!(
        same_epoch
            .expect_err("checked above")
            .to_string()
            .contains("docket_quest_attempts_epoch_key"),
        "same epoch refusal must name epoch_key"
    );

    let replay = insert_attempt(&pool, &quest_id, 2, "claim-one", SHA256).await;
    assert!(replay.is_err(), "second attempt replayed idempotency key");
    assert!(
        replay
            .expect_err("checked above")
            .to_string()
            .contains("docket_quest_attempts_idempotency_key"),
        "replay refusal must name idempotency_key"
    );
    Ok(())
}

// Kills: lease tokens stored in plaintext or in a hash shape weaker than a
// lowercase 64-hex digest.
// red-proof: delete docket_quest_attempts_lease_hash_check.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_attempt_lease_token_must_be_a_sha256_shape() -> TestResult {
    let pool = fresh_docket().await?;
    let quest_id = insert_frozen_quest(&pool).await?;
    let refused = insert_attempt(&pool, &quest_id, 1, "bad-lease", "plaintext-token").await;
    assert!(refused.is_err(), "non-hash lease token was accepted");
    assert!(
        refused
            .expect_err("checked above")
            .to_string()
            .contains("docket_quest_attempts_lease_hash_check"),
        "lease refusal must name lease_hash_check"
    );
    Ok(())
}

// Kills: a dependency edge pointing a quest at itself, allowing an impossible
// prerequisite cycle at the first edge.
// red-proof: delete docket_quest_dependencies_self_check.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_dependency_refuses_self_loop() -> TestResult {
    let pool = fresh_docket().await?;
    let quest_id = insert_frozen_quest(&pool).await?;
    let refused = sqlx::query(
        "INSERT INTO docket.quest_dependencies (quest_id, depends_on_quest_id)
         VALUES ($1::uuid, $1::uuid)",
    )
    .bind(&quest_id)
    .execute(&pool)
    .await;
    assert!(refused.is_err(), "quest dependency accepted a self-loop");
    assert!(
        refused
            .expect_err("checked above")
            .to_string()
            .contains("docket_quest_dependencies_self_check"),
        "self-loop refusal must name self_check"
    );
    Ok(())
}

fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn settle_request(
    room: &str,
    capability: &str,
    quest_id: &str,
    attempt_id: &str,
    lease_token: &str,
    idempotency_key: &str,
) -> QuestReportParams {
    QuestReportParams {
        room: room.into(),
        spirit: "Prover".into(),
        session: "test-session".into(),
        capability: capability.into(),
        idempotency_key: idempotency_key.into(),
        quest_id: quest_id.into(),
        attempt_id: attempt_id.into(),
        lease_token: lease_token.into(),
        action: QuestReportAction::SettleItem,
        body: "reviewed against the criterion".into(),
        kind: None,
        performed_by: None,
        authored_role: Some("reviewer".into()),
        item_position: Some(1),
        verdict: Some("met".into()),
    }
}

// Kills: the claimant room settling its own acceptance items by declaring a
// reviewer role. Review independence (guild-hall #144) is enforced at the
// authenticated room level, never by the caller's role text alone.
// red-proof: remove the claimant_room comparison from quest_report's
// SettleItem arm.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_review_independence_refuses_claimant_room() -> TestResult {
    let pool = fresh_docket().await?;
    sqlx::raw_sql(CAPABILITY_MIGRATION).execute(&pool).await?;

    let quest_id = insert_frozen_quest(&pool).await?;
    let lease_token = "review-independence-lease";
    insert_attempt(
        &pool,
        &quest_id,
        1,
        "claim-review",
        &sha256_hex(lease_token),
    )
    .await?;
    sqlx::query(
        "UPDATE docket.quests SET state='submitted', claim_epoch=1 WHERE quest_id=$1::uuid",
    )
    .bind(&quest_id)
    .execute(&pool)
    .await?;
    sqlx::query("UPDATE docket.quest_attempts SET state='yielded' WHERE quest_id=$1::uuid")
        .bind(&quest_id)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO docket.quest_acceptance_items (quest_id, position, criterion)
         VALUES ($1::uuid, 1, 'reviewed')",
    )
    .bind(&quest_id)
    .execute(&pool)
    .await?;
    // The helper claims as test-room; review-room is the independent peer.
    for (room, secret) in [
        ("test-room", "claimant-secret"),
        ("review-room", "reviewer-secret"),
    ] {
        sqlx::query(
            "INSERT INTO docket.room_capabilities (room, operation_class, capability_hash)
             VALUES ($1, 'docket_write', $2)",
        )
        .bind(room)
        .bind(sha256_hex(secret))
        .execute(&pool)
        .await?;
    }
    let attempt_id: String = sqlx::query_scalar(
        "SELECT attempt_id::text FROM docket.quest_attempts WHERE quest_id=$1::uuid",
    )
    .bind(&quest_id)
    .fetch_one(&pool)
    .await?;

    let self_settle = quest_report(
        &pool,
        settle_request(
            "test-room",
            "claimant-secret",
            &quest_id,
            &attempt_id,
            lease_token,
            "self-settle-1",
        ),
    )
    .await;
    match self_settle {
        Err(AppError::Refusal { code, .. }) => assert_eq!(
            code, "review_independence",
            "self-settle refusal must name review_independence"
        ),
        other => panic!("claimant room settled its own item: {other:?}"),
    }

    let peer = quest_report(
        &pool,
        settle_request(
            "review-room",
            "reviewer-secret",
            &quest_id,
            &attempt_id,
            lease_token,
            "peer-settle-1",
        ),
    )
    .await?;
    assert!(
        peer.settled,
        "an independent peer settlement must settle the single-item quest"
    );
    Ok(())
}

fn work_request(
    room: &str,
    capability: &str,
    quest_id: &str,
    attempt_id: &str,
    lease_token: &str,
    idempotency_key: &str,
    action: QuestReportAction,
) -> QuestReportParams {
    QuestReportParams {
        room: room.into(),
        spirit: "Prover".into(),
        session: "test-session".into(),
        capability: capability.into(),
        idempotency_key: idempotency_key.into(),
        quest_id: quest_id.into(),
        attempt_id: attempt_id.into(),
        lease_token: lease_token.into(),
        action,
        body: "work against the quest".into(),
        kind: None,
        performed_by: None,
        authored_role: None,
        item_position: None,
        verdict: None,
    }
}

// Kills: a foreign room driving another room's attempt with a leaked valid
// lease. The claimant binding (guild-hall #159 ruling 2) is the symmetric
// twin of review independence: Progress and Submit bind to the claimant room
// at the authenticated room level, never by the lease token alone.
// red-proof: remove the claimant_room comparison ahead of quest_report's
// action match.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn docket_claimant_binding_refuses_foreign_room() -> TestResult {
    let pool = fresh_docket().await?;
    sqlx::raw_sql(CAPABILITY_MIGRATION).execute(&pool).await?;

    let quest_id = insert_frozen_quest(&pool).await?;
    let lease_token = "claimant-binding-lease";
    insert_attempt(
        &pool,
        &quest_id,
        1,
        "claim-binding",
        &sha256_hex(lease_token),
    )
    .await?;
    sqlx::query("UPDATE docket.quests SET state='claimed', claim_epoch=1 WHERE quest_id=$1::uuid")
        .bind(&quest_id)
        .execute(&pool)
        .await?;
    // The helper claims as test-room; thief-room holds a real capability and
    // the leaked lease token, and must still be refused.
    for (room, secret) in [
        ("test-room", "claimant-secret"),
        ("thief-room", "thief-secret"),
    ] {
        sqlx::query(
            "INSERT INTO docket.room_capabilities (room, operation_class, capability_hash)
             VALUES ($1, 'docket_write', $2)",
        )
        .bind(room)
        .bind(sha256_hex(secret))
        .execute(&pool)
        .await?;
    }
    let attempt_id: String = sqlx::query_scalar(
        "SELECT attempt_id::text FROM docket.quest_attempts WHERE quest_id=$1::uuid",
    )
    .bind(&quest_id)
    .fetch_one(&pool)
    .await?;

    for (action, key) in [
        (QuestReportAction::Progress, "thief-progress-1"),
        (QuestReportAction::Submit, "thief-submit-1"),
    ] {
        let leaked = quest_report(
            &pool,
            work_request(
                "thief-room",
                "thief-secret",
                &quest_id,
                &attempt_id,
                lease_token,
                key,
                action,
            ),
        )
        .await;
        match leaked {
            Err(AppError::Refusal { code, .. }) => assert_eq!(
                code, "claimant_binding",
                "foreign-room refusal must name claimant_binding"
            ),
            other => panic!("foreign room drove the attempt: {other:?}"),
        }
    }

    let own = quest_report(
        &pool,
        work_request(
            "test-room",
            "claimant-secret",
            &quest_id,
            &attempt_id,
            lease_token,
            "claimant-progress-1",
            QuestReportAction::Progress,
        ),
    )
    .await?;
    assert!(own.ok, "the claimant room's own progress must succeed");
    Ok(())
}
