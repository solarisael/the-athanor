use athanor_substrate::{
    Config, EmbeddingMode, giga_candidate_list, giga_candidate_store, giga_event_claim,
    giga_event_finish, giga_event_ingest, giga_event_replay, giga_health, giga_process,
    giga_promote, giga_queue_maintenance, giga_review, migrations::run_migrations,
};
use house_core::{
    GigaAuthority, GigaCandidate, GigaCandidateKind, GigaClassifierIdentity,
    GigaCodingLessonPromotionPayload, GigaEvent, GigaEventClaimReceipt, GigaEventClaimRequest,
    GigaEventFinishOutcome, GigaEventFinishRequest, GigaEventReplayRequest, GigaEventType,
    GigaLifecycle, GigaMemoryPromotionPayload, GigaProjectLessonPromotionPayload,
    GigaPromotionAuthority, GigaPromotionKind, GigaPromotionPayload, GigaPromotionRequest,
    GigaPublicationConsent, GigaQueueMaintenanceOperation, GigaQueueMaintenanceRequest,
    GigaQueueMaintenanceScope, GigaQueueState, GigaResonance, GigaReviewAction, GigaReviewState,
    GigaScope, GigaScores, GigaSourceRef, GigaSourceType, GigaVisibility, RoomKey,
};
use house_protocol::{
    GigaCandidateListParams, GigaCandidateStoreDisposition, GigaEventIngestDisposition,
    GigaHealthParams,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
    types::Json,
};
use std::{error::Error, str::FromStr};
use uuid::Uuid;

const ROOM: &str = "giga-runtime-test";
const OTHER_ROOM: &str = "giga-runtime-other";
const PROJECT: &str = "giga-runtime-project";
const SOURCE_AT: &str = "2030-01-01T00:00:00+00:00";
const EVENT_AT: &str = "2030-01-01T00:00:01Z";
const REVIEWED_AT: &str = "2030-01-01T01:00:00Z";

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

/// Canonical migrations live at `<athanor-root>/substrate/migrations`, outside
/// this crate. Resolve them from the crate manifest so the path survives the
/// test binary being built or run from anywhere.
macro_rules! migration {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../substrate/migrations/",
            $name
        ))
    };
}

fn failure(message: impl Into<String>) -> Box<dyn Error + Send + Sync> {
    std::io::Error::new(std::io::ErrorKind::Other, message.into()).into()
}

fn require(condition: bool, message: impl Into<String>) -> TestResult {
    if condition {
        Ok(())
    } else {
        Err(failure(message))
    }
}

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory"),
        "refusing the live/default database"
    );
    assert!(
        !lower.contains("solarisael-house"),
        "refusing a production-looking database"
    );
    url
}

fn private_source(source_id: &str, content_hash: String) -> TestResult<GigaSourceRef> {
    Ok(GigaSourceRef::new(
        GigaSourceType::Turn,
        source_id.into(),
        "user".into(),
        SOURCE_AT.into(),
        content_hash,
        GigaScope::new(Some(ROOM.into()), None, GigaVisibility::Private, false)?,
        None,
    )?)
}

fn queue_event(event_id: &str) -> TestResult<GigaEvent> {
    let source = private_source(&format!("{event_id}-turn"), "1".repeat(64))?;
    Ok(GigaEvent::new(
        event_id.into(),
        GigaEventType::ConversationWindow,
        RoomKey::new(ROOM)?,
        format!("{event_id}-session"),
        vec![],
        vec![source],
        GigaLifecycle::conversation_window(),
        EVENT_AT.into(),
    )?)
}

async fn ingest_queue_event(pool: &PgPool, event_id: &str) -> TestResult {
    let receipt = giga_event_ingest(pool, queue_event(event_id)?).await?;
    require(
        receipt.disposition == GigaEventIngestDisposition::Accepted,
        "queue fixture event must be ingested",
    )
}

fn claim_request(worker_id: &str, lease_seconds: u32) -> TestResult<GigaEventClaimRequest> {
    Ok(GigaEventClaimRequest::new(
        RoomKey::new(ROOM)?,
        worker_id.into(),
        lease_seconds,
    )?)
}

fn finish_request(
    event_id: &str,
    worker_id: &str,
    outcome: GigaEventFinishOutcome,
    error_class: Option<&str>,
    retry_after_seconds: Option<u32>,
) -> TestResult<GigaEventFinishRequest> {
    Ok(GigaEventFinishRequest::new(
        RoomKey::new(ROOM)?,
        event_id.into(),
        worker_id.into(),
        outcome,
        0,
        error_class.map(str::to_owned),
        retry_after_seconds,
    )?)
}

async fn queue_contracts(pool: &PgPool) -> TestResult {
    let concurrent_event = "queue-concurrent";
    ingest_queue_event(pool, concurrent_event).await?;
    let left_request = claim_request("claim-left", 60)?;
    let right_request = claim_request("claim-right", 60)?;
    let (left, right) = tokio::join!(
        giga_event_claim(pool, left_request),
        giga_event_claim(pool, right_request)
    );
    let left = left?;
    let right = right?;
    require(
        (left.event().is_some() as usize) + (right.event().is_some() as usize) == 1,
        "concurrent workers must not claim the same event",
    )?;
    let (owner, non_owner, claimed) = if left.event().is_some() {
        ("claim-left", "claim-right", &left)
    } else {
        ("claim-right", "claim-left", &right)
    };
    require(
        claimed.event().map(GigaEvent::event_id) == Some(concurrent_event),
        "the winning claim must contain the selected event",
    )?;
    require(
        claimed.attempt_count() == Some(1),
        "the first claim must be attempt one",
    )?;
    let stored_claimed_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT locked_at FROM giga_events WHERE event_id=$1")
            .bind(concurrent_event)
            .fetch_one(pool)
            .await?;
    require(
        claimed.claimed_at() == stored_claimed_at.to_rfc3339(),
        "claim receipts must expose the database clock stored with the lease",
    )?;

    let wrong_owner = giga_event_finish(
        pool,
        finish_request(
            concurrent_event,
            non_owner,
            GigaEventFinishOutcome::Succeeded,
            None,
            None,
        )?,
    )
    .await;
    require(
        wrong_owner.is_err(),
        "only the current lease owner may finish an event",
    )?;
    let unfinished: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM giga_event_attempts
         WHERE event_id=$1 AND finished_at IS NULL",
    )
    .bind(concurrent_event)
    .fetch_one(pool)
    .await?;
    require(
        unfinished == 1,
        "an ownership refusal must not finish the active attempt",
    )?;

    let success_request = finish_request(
        concurrent_event,
        owner,
        GigaEventFinishOutcome::Succeeded,
        None,
        None,
    )?;
    let success = giga_event_finish(pool, success_request.clone()).await?;
    require(
        success.queue_state() == GigaQueueState::Succeeded && success.candidate_count() == 0,
        "a zero-candidate classification must be a successful terminal outcome",
    )?;
    let stored_finished_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT finished_at FROM giga_event_attempts
         WHERE event_id=$1 AND replay_count=0 AND attempt_count=1",
    )
    .bind(concurrent_event)
    .fetch_one(pool)
    .await?;
    require(
        success.finished_at() == stored_finished_at.to_rfc3339(),
        "finish receipts must expose the database clock stored with the attempt",
    )?;
    let repeated_success = giga_event_finish(pool, success_request).await?;
    require(
        repeated_success == success,
        "an exact finish retry must return the original terminal receipt",
    )?;
    let attempt_rows: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM giga_event_attempts WHERE event_id=$1")
            .bind(concurrent_event)
            .fetch_one(pool)
            .await?;
    require(
        attempt_rows == 1,
        "an exact finish retry must not append attempt history",
    )?;

    let lease_event = "queue-expired-lease";
    ingest_queue_event(pool, lease_event).await?;
    let first = giga_event_claim(pool, claim_request("lease-original", 1)?).await?;
    require(
        first.attempt_count() == Some(1),
        "initial lease claim must be attempt one",
    )?;
    sqlx::query(
        "UPDATE giga_events
         SET lease_expires_at=clock_timestamp()-INTERVAL '1 second'
         WHERE event_id=$1 AND queue_state='running'",
    )
    .bind(lease_event)
    .execute(pool)
    .await?;
    let recovered = giga_event_claim(pool, claim_request("lease-recovery", 60)?).await?;
    require(
        recovered.event().map(GigaEvent::event_id) == Some(lease_event)
            && recovered.attempt_count() == Some(2),
        "an expired lease must be recoverable by another worker as the next attempt",
    )?;
    let expired_row = sqlx::query(
        "SELECT outcome,error_class,finished_at IS NOT NULL AS finished
         FROM giga_event_attempts
         WHERE event_id=$1 AND replay_count=0 AND attempt_count=1",
    )
    .bind(lease_event)
    .fetch_one(pool)
    .await?;
    require(
        expired_row
            .try_get::<Option<String>, _>("outcome")?
            .as_deref()
            == Some("lease_expired")
            && expired_row
                .try_get::<Option<String>, _>("error_class")?
                .as_deref()
                == Some("lease_expired")
            && expired_row.try_get::<bool, _>("finished")?,
        "expired lease recovery must retain terminal diagnostics for the abandoned attempt",
    )?;

    let retry_two = giga_event_finish(
        pool,
        finish_request(
            lease_event,
            "lease-recovery",
            GigaEventFinishOutcome::Retry,
            Some("transient"),
            Some(0),
        )?,
    )
    .await?;
    require(
        retry_two.queue_state() == GigaQueueState::Pending && retry_two.attempt_count() == 2,
        "a retry must return work to pending without resetting its bounded attempt count",
    )?;

    for attempt in [3_u32, 4_u32] {
        let claim = giga_event_claim(pool, claim_request("lease-retry", 60)?).await?;
        require(
            claim.attempt_count() == Some(attempt),
            format!("claim must advance to bounded attempt {attempt}"),
        )?;
        let retry = giga_event_finish(
            pool,
            finish_request(
                lease_event,
                "lease-retry",
                GigaEventFinishOutcome::Retry,
                Some("transient"),
                Some(0),
            )?,
        )
        .await?;
        require(
            retry.attempt_count() == attempt,
            "retry receipt must retain its attempt",
        )?;
    }

    let fifth = giga_event_claim(pool, claim_request("lease-final", 60)?).await?;
    require(
        fifth.attempt_count() == Some(5),
        "the fifth bounded attempt must be claimable",
    )?;
    let over_ceiling = giga_event_finish(
        pool,
        finish_request(
            lease_event,
            "lease-final",
            GigaEventFinishOutcome::Retry,
            Some("still_transient"),
            Some(0),
        )?,
    )
    .await;
    require(
        over_ceiling.is_err(),
        "the fifth attempt must not be returned for a sixth try",
    )?;
    let running_state: String =
        sqlx::query_scalar("SELECT queue_state FROM giga_events WHERE event_id=$1")
            .bind(lease_event)
            .fetch_one(pool)
            .await?;
    require(
        running_state == "running",
        "a retry-ceiling refusal must leave the fifth lease available for terminal finish",
    )?;
    let failed = giga_event_finish(
        pool,
        finish_request(
            lease_event,
            "lease-final",
            GigaEventFinishOutcome::Failed,
            Some("retry_exhausted"),
            None,
        )?,
    )
    .await?;
    require(
        failed.queue_state() == GigaQueueState::Failed && failed.attempt_count() == 5,
        "the final bounded attempt must remain inspectable as failed",
    )?;

    let replay_request = GigaEventReplayRequest::new(
        RoomKey::new(ROOM)?,
        lease_event.into(),
        "test-operator".into(),
        "deliberate integration replay".into(),
    )?;
    let replayed = giga_event_replay(pool, replay_request.clone()).await?;
    require(
        replayed.previous_state() == GigaQueueState::Failed
            && replayed.queue_state() == GigaQueueState::Pending
            && replayed.attempt_count() == 0,
        "deliberate replay must reset only queue counters and state",
    )?;
    let stored_replayed_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT replayed_at FROM giga_event_replays WHERE event_id=$1 AND replay_count=1",
    )
    .bind(lease_event)
    .fetch_one(pool)
    .await?;
    require(
        replayed.replayed_at() == stored_replayed_at.to_rfc3339(),
        "replay receipts must expose the database clock stored with the replay",
    )?;
    let replayed_again = giga_event_replay(pool, replay_request).await?;
    require(
        replayed_again == replayed,
        "an exact replay retry must be idempotent",
    )?;
    let replay_rows: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM giga_event_replays WHERE event_id=$1")
            .bind(lease_event)
            .fetch_one(pool)
            .await?;
    require(
        replay_rows == 1,
        "an exact replay retry must append one history row",
    )?;
    let first_generation_attempts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM giga_event_attempts
         WHERE event_id=$1 AND replay_count=0",
    )
    .bind(lease_event)
    .fetch_one(pool)
    .await?;
    require(
        first_generation_attempts == 5,
        "replay must preserve every attempt from the failed generation",
    )?;

    let after_replay = giga_event_claim(pool, claim_request("replay-worker", 60)?).await?;
    require(
        after_replay.attempt_count() == Some(1),
        "replayed work must begin a fresh bounded attempt generation",
    )?;
    giga_event_finish(
        pool,
        finish_request(
            lease_event,
            "replay-worker",
            GigaEventFinishOutcome::Failed,
            Some("replay_proof_complete"),
            None,
        )?,
    )
    .await?;
    let replay_generation_attempts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM giga_event_attempts
         WHERE event_id=$1 AND replay_count=1",
    )
    .bind(lease_event)
    .fetch_one(pool)
    .await?;
    require(
        replay_generation_attempts == 1,
        "new replay attempts must be separate from retained prior history",
    )?;
    sqlx::query("DELETE FROM giga_events WHERE event_id = ANY($1)")
        .bind(vec![concurrent_event, lease_event])
        .execute(pool)
        .await?;
    Ok(())
}
async fn queue_maintenance_contracts(pool: &PgPool) -> TestResult {
    ingest_queue_event(pool, "maintenance-failed").await?;
    let failed = giga_event_claim(pool, claim_request("maintenance-failed-worker", 60)?).await?;
    require(
        failed.event().map(GigaEvent::event_id) == Some("maintenance-failed"),
        "failed fixture must be claimed",
    )?;
    giga_event_finish(
        pool,
        finish_request(
            "maintenance-failed",
            "maintenance-failed-worker",
            GigaEventFinishOutcome::Failed,
            Some("fixture_failure"),
            None,
        )?,
    )
    .await?;

    ingest_queue_event(pool, "maintenance-expired").await?;
    let expired = giga_event_claim(pool, claim_request("maintenance-expired-worker", 60)?).await?;
    require(
        expired.event().map(GigaEvent::event_id) == Some("maintenance-expired"),
        "expired fixture must be claimed",
    )?;
    sqlx::query(
        "UPDATE giga_events SET lease_expires_at=NOW()-INTERVAL '1 second'
         WHERE event_id='maintenance-expired'",
    )
    .execute(pool)
    .await?;

    ingest_queue_event(pool, "maintenance-active").await?;
    let active = giga_event_claim(pool, claim_request("maintenance-active-worker", 60)?).await?;
    require(
        active.event().map(GigaEvent::event_id) == Some("maintenance-active"),
        "active fixture must be claimed",
    )?;

    ingest_queue_event(pool, "maintenance-succeeded").await?;
    let succeeded =
        giga_event_claim(pool, claim_request("maintenance-succeeded-worker", 60)?).await?;
    require(
        succeeded.event().map(GigaEvent::event_id) == Some("maintenance-succeeded"),
        "succeeded fixture must be claimed",
    )?;
    giga_event_finish(
        pool,
        finish_request(
            "maintenance-succeeded",
            "maintenance-succeeded-worker",
            GigaEventFinishOutcome::Succeeded,
            None,
            None,
        )?,
    )
    .await?;
    ingest_queue_event(pool, "maintenance-pending").await?;

    stage_candidate(
        pool,
        "maintenance-candidate",
        GigaCandidateKind::Memory,
        None,
        false,
        'b',
    )
    .await?;

    let check = giga_queue_maintenance(
        pool,
        GigaQueueMaintenanceRequest::new(
            RoomKey::new(ROOM)?,
            GigaQueueMaintenanceOperation::Check,
            GigaQueueMaintenanceScope::Room,
        ),
    )
    .await?;
    require(
        check.eligible_events == 3
            && check.blocked_events == 2
            && check.deleted_events == 0
            && check.deleted_attempts == 0
            && check.preserved_candidates == 1
            && check.before == check.after,
        "maintenance check must report the exact purge boundary without mutating it",
    )?;

    let purge = giga_queue_maintenance(
        pool,
        GigaQueueMaintenanceRequest::new(
            RoomKey::new(ROOM)?,
            GigaQueueMaintenanceOperation::PurgeStuck,
            GigaQueueMaintenanceScope::Room,
        ),
    )
    .await?;
    require(
        purge.eligible_events == 3
            && purge.blocked_events == 2
            && purge.deleted_events == 3
            && purge.deleted_attempts == 2
            && purge.preserved_candidates == 1,
        "purge must delete only discardable stuck events and their attempts",
    )?;

    for event_id in [
        "maintenance-pending",
        "maintenance-failed",
        "maintenance-expired",
    ] {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM giga_events WHERE event_id=$1)")
                .bind(event_id)
                .fetch_one(pool)
                .await?;
        require(!exists, format!("{event_id} must be deleted"))?;
    }
    for event_id in [
        "maintenance-active",
        "maintenance-succeeded",
        "maintenance-candidate-event",
    ] {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM giga_events WHERE event_id=$1)")
                .bind(event_id)
                .fetch_one(pool)
                .await?;
        require(exists, format!("{event_id} must be preserved"))?;
    }
    require(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)::bigint FROM giga_candidates WHERE candidate_id='maintenance-candidate'",
        )
        .fetch_one(pool)
        .await?
            == 1,
        "queue maintenance must preserve candidate state",
    )
}

async fn stage_candidate_in_room(
    pool: &PgPool,
    room: &str,
    candidate_id: &str,
    kind: GigaCandidateKind,
    project: Option<&str>,
    publication_review_required: bool,
    hash_digit: char,
) -> TestResult<GigaSourceRef> {
    let scope = if kind == GigaCandidateKind::ProjectLesson {
        GigaScope::new(
            Some(room.into()),
            project.map(str::to_owned),
            GigaVisibility::Private,
            publication_review_required,
        )?
    } else {
        GigaScope::new(Some(room.into()), None, GigaVisibility::Private, false)?
    };
    let source = GigaSourceRef::new(
        GigaSourceType::Turn,
        format!("{candidate_id}-turn"),
        "user".into(),
        SOURCE_AT.into(),
        hash_digit.to_string().repeat(64),
        scope.clone(),
        None,
    )?;
    let project_keys = project
        .map(|value| vec![value.to_owned()])
        .unwrap_or_default();
    let event_id = format!("{candidate_id}-event");
    let event = GigaEvent::new(
        event_id.clone(),
        GigaEventType::ConversationWindow,
        RoomKey::new(room)?,
        format!("{candidate_id}-session"),
        project_keys.clone(),
        vec![source.clone()],
        GigaLifecycle::conversation_window(),
        EVENT_AT.into(),
    )?;
    let ingested = giga_event_ingest(pool, event).await?;
    require(
        ingested.disposition == GigaEventIngestDisposition::Accepted,
        "candidate parent event must be ingested",
    )?;
    let proof_refs = if kind.requires_proof() {
        vec![source.source_id().to_owned()]
    } else {
        vec![]
    };
    let candidate = GigaCandidate::new(
        candidate_id.into(),
        event_id,
        RoomKey::new(room)?,
        format!("{candidate_id}-session"),
        kind,
        vec![source.clone()],
        proof_refs,
        GigaScores::new(0.8, 0.7, 0.9, 0.95)?,
        project_keys,
        vec![],
        vec![],
        vec!["integration".into(), "atomic".into()],
        format!("{candidate_id} proposed title"),
        "source-grounded gist".into(),
        "source-grounded rationale".into(),
        scope,
        GigaAuthority::PointerOnly,
        GigaReviewState::Unreviewed,
        GigaClassifierIdentity::new(
            "agents-a1".into(),
            "ollama".into(),
            "manifest-test".into(),
            "prompt-test".into(),
            "a".repeat(64),
            format!("{candidate_id}-run"),
            "2030-01-01T00:30:00Z".into(),
        )?,
        "2030-01-01T00:30:01Z".into(),
        None,
        vec![],
    )?;
    let stored = giga_candidate_store(pool, candidate).await?;
    require(
        stored.disposition == GigaCandidateStoreDisposition::Stored,
        "candidate fixture must be stored",
    )?;
    let review = GigaReviewAction::new(
        candidate_id.into(),
        "governing-spirit".into(),
        GigaReviewState::Unreviewed,
        GigaReviewState::InReview,
        "exact sources reviewed".into(),
        "operator-authorized review".into(),
        vec![source.clone()],
        None,
        None,
        vec![],
        None,
        "2030-01-01T00:40:00Z".into(),
    )?;
    let reviewed = giga_review(pool, review).await?;
    require(
        reviewed.new_state == "in_review",
        "promotion fixture must enter deliberate review",
    )?;
    Ok(source)
}

async fn persisted_process_contract(pool: &PgPool, cfg: &Config) -> TestResult {
    let event_id = "process-persisted-source";
    let session_id = "process-persisted-session";
    let source_id = "process-persisted-turn";
    let text = "exact persisted source text";
    let content_hash = format!("{:x}", Sha256::digest(text.as_bytes()));
    let scope = GigaScope::new(Some(ROOM.into()), None, GigaVisibility::Private, true)?;
    let source = GigaSourceRef::new(
        GigaSourceType::Turn,
        source_id.into(),
        "user".into(),
        SOURCE_AT.into(),
        content_hash,
        scope.clone(),
        None,
    )?;
    let event = GigaEvent::new(
        event_id.into(),
        GigaEventType::ConversationWindow,
        RoomKey::new(ROOM)?,
        session_id.into(),
        vec![],
        vec![source.clone()],
        GigaLifecycle::conversation_window(),
        EVENT_AT.into(),
    )?;
    require(
        giga_event_ingest(pool, event).await?.disposition == GigaEventIngestDisposition::Accepted,
        "persisted-source process event must be ingested",
    )?;
    let claim = giga_event_claim(pool, claim_request("process-owner", 60)?).await?;
    require(
        claim.event().map(GigaEvent::event_id) == Some(event_id),
        "processing must begin with the canonical room claim",
    )?;
    let candidate = GigaCandidate::new(
        "process-persisted-candidate".into(),
        event_id.into(),
        RoomKey::new(ROOM)?,
        session_id.into(),
        GigaCandidateKind::Memory,
        vec![source],
        vec![],
        GigaScores::new(0.8, 0.7, 0.9, 0.95)?,
        vec![],
        vec![],
        vec![],
        vec!["persisted".into(), "source".into()],
        "Persisted source candidate".into(),
        "Source-grounded gist".into(),
        "Source-grounded rationale".into(),
        scope,
        GigaAuthority::PointerOnly,
        GigaReviewState::Unreviewed,
        GigaClassifierIdentity::new(
            "agents-a1".into(),
            "ollama".into(),
            "manifest-test".into(),
            "prompt-test".into(),
            "a".repeat(64),
            "process-persisted-run".into(),
            "2030-01-01T00:30:00Z".into(),
        )?,
        "2030-01-01T00:30:01Z".into(),
        None,
        vec![],
    )?;
    require(
        giga_candidate_store(pool, candidate).await?.disposition
            == GigaCandidateStoreDisposition::Stored,
        "persisted-source candidate must be stored",
    )?;

    let directory = std::env::temp_dir().join(format!("giga-process-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&directory).await?;
    tokio::fs::write(
        directory.join("2030-01-01.jsonl"),
        format!(
            "{}\n",
            serde_json::json!({
                "sessionID": session_id,
                "messageID": source_id,
                "role": "user",
                "spirit": "Integration",
                "text": text
            })
        ),
    )
    .await?;
    let mut process_config = cfg.clone();
    process_config.giga_source_ledger_dir = Some(directory.clone());
    process_config.giga_source_room = Some(ROOM.into());
    let stolen_claim = GigaEventClaimReceipt::new(
        claim.room().clone(),
        "process-intruder".into(),
        claim.claimed_at().into(),
        claim.event().cloned(),
        claim.lease_expires_at().map(str::to_owned),
        claim.attempt_count(),
    )?;
    require(
        giga_process(pool, &process_config, &stolen_claim)
            .await
            .is_err(),
        "processing must not steal another worker's active claim",
    )?;
    let result = giga_process(pool, &process_config, &claim).await?;
    require(
        result.outcome == "succeeded" && result.candidate_count == 1 && result.attempt_count == 1,
        "claimed process must reload exact persisted text and finish",
    )?;
    let duplicate = giga_process(pool, &process_config, &claim).await;
    require(
        duplicate.is_err(),
        "a finished claim must not be accepted for duplicate processing",
    )?;
    let attempts: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM giga_event_attempts WHERE event_id=$1")
            .bind(event_id)
            .fetch_one(pool)
            .await?;
    require(
        attempts == 1,
        "a duplicate process attempt must not create a second claim",
    )?;

    let expired_event_id = "process-expired-claim";
    ingest_queue_event(pool, expired_event_id).await?;
    let expired_claim = giga_event_claim(pool, claim_request("process-expired-owner", 60)?).await?;
    sqlx::query(
        "UPDATE giga_events
         SET lease_expires_at=clock_timestamp()-INTERVAL '1 second'
         WHERE event_id=$1 AND queue_state='running'",
    )
    .bind(expired_event_id)
    .execute(pool)
    .await?;
    let expired_process = giga_process(pool, &process_config, &expired_claim).await;
    require(
        expired_process.is_err(),
        "processing must reject an expired claim before reading or classifying sources",
    )?;
    let recovered = giga_event_claim(pool, claim_request("process-recovery-owner", 60)?).await?;
    require(
        recovered.event().map(GigaEvent::event_id) == Some(expired_event_id)
            && recovered.attempt_count() == Some(2),
        "the canonical claim owner must recover a shutdown-abandoned lease",
    )?;
    giga_event_finish(
        pool,
        finish_request(
            expired_event_id,
            "process-recovery-owner",
            GigaEventFinishOutcome::Failed,
            Some("shutdown_recovery_proof"),
            None,
        )?,
    )
    .await?;
    sqlx::query("DELETE FROM giga_events WHERE event_id = ANY($1)")
        .bind(vec![event_id, expired_event_id])
        .execute(pool)
        .await?;
    tokio::fs::remove_dir_all(directory).await?;
    Ok(())
}

async fn stage_candidate(
    pool: &PgPool,
    candidate_id: &str,
    kind: GigaCandidateKind,
    project: Option<&str>,
    publication_review_required: bool,
    hash_digit: char,
) -> TestResult<GigaSourceRef> {
    stage_candidate_in_room(
        pool,
        ROOM,
        candidate_id,
        kind,
        project,
        publication_review_required,
        hash_digit,
    )
    .await
}

async fn review_resonance_and_room_scope_contracts(pool: &PgPool) -> TestResult {
    let candidate_id = "curio-resonance";
    let original_source = stage_candidate(
        pool,
        candidate_id,
        GigaCandidateKind::Memory,
        None,
        false,
        'c',
    )
    .await?;
    giga_review(
        pool,
        GigaReviewAction::new(
            candidate_id.into(),
            "governing-spirit".into(),
            GigaReviewState::InReview,
            GigaReviewState::Curio,
            "retain as curio pending new evidence".into(),
            "operator-authorized review".into(),
            vec![original_source.clone()],
            None,
            None,
            vec![],
            None,
            "2030-01-01T00:41:00Z".into(),
        )?,
    )
    .await?;

    let resonance_event_id = "curio-resonance-new-event";
    let resonance_source = GigaSourceRef::new(
        GigaSourceType::Turn,
        "curio-resonance-new-turn".into(),
        "assistant".into(),
        "2030-01-01T00:41:30Z".into(),
        "d".repeat(64),
        GigaScope::new(Some(ROOM.into()), None, GigaVisibility::Private, false)?,
        None,
    )?;
    giga_event_ingest(
        pool,
        GigaEvent::new(
            resonance_event_id.into(),
            GigaEventType::ConversationWindow,
            RoomKey::new(ROOM)?,
            "curio-resonance-new-session".into(),
            vec![],
            vec![resonance_source.clone()],
            GigaLifecycle::conversation_window(),
            "2030-01-01T00:41:31Z".into(),
        )?,
    )
    .await?;
    let resonance_classifier = GigaClassifierIdentity::new(
        "resonance-model".into(),
        "ollama".into(),
        "resonance-manifest".into(),
        "resonance-prompt".into(),
        "e".repeat(64),
        "resonance-run".into(),
        "2030-01-01T00:41:32Z".into(),
    )?;
    let mismatched_source = GigaSourceRef::new(
        GigaSourceType::Turn,
        resonance_source.source_id().into(),
        resonance_source.role().into(),
        resonance_source.timestamp().into(),
        "f".repeat(64),
        (*resonance_source.scope()).clone(),
        None,
    )?;
    let mismatched_review = GigaReviewAction::new(
        candidate_id.into(),
        "governing-spirit".into(),
        GigaReviewState::Curio,
        GigaReviewState::InReview,
        "new event appears to resonate".into(),
        "operator-authorized review".into(),
        vec![original_source.clone()],
        None,
        None,
        vec![],
        Some(GigaResonance::new(
            resonance_event_id.into(),
            0.91,
            resonance_classifier.clone(),
            vec![mismatched_source],
        )?),
        "2030-01-01T00:42:00Z".into(),
    )?;
    require(
        giga_review(pool, mismatched_review).await.is_err(),
        "resonance sources must match their separately named new event",
    )?;

    let reviewed = giga_review(
        pool,
        GigaReviewAction::new(
            candidate_id.into(),
            "governing-spirit".into(),
            GigaReviewState::Curio,
            GigaReviewState::InReview,
            "new event provides typed resonance evidence".into(),
            "operator-authorized review".into(),
            vec![original_source],
            None,
            None,
            vec![],
            Some(GigaResonance::new(
                resonance_event_id.into(),
                0.91,
                resonance_classifier,
                vec![resonance_source.clone()],
            )?),
            "2030-01-01T00:43:00Z".into(),
        )?,
    )
    .await?;
    let returned_resonance = reviewed
        .resonance
        .0
        .ok_or_else(|| failure("curio reactivation must return its typed resonance"))?;
    require(
        returned_resonance.event_id == resonance_event_id
            && returned_resonance.score == 0.91
            && returned_resonance.classifier.run_id == "resonance-run"
            && returned_resonance.source_refs.len() == 1
            && returned_resonance.source_refs[0].source_id == resonance_source.source_id(),
        "curio resonance must survive typed protocol readback",
    )?;
    let stored_resonance = sqlx::query(
        "SELECT event_id,score,classifier_model,classifier_provider_type,
                classifier_model_version,classifier_prompt_version,
                classifier_configuration_digest,classifier_run_id,
                classifier_completed_at IS NOT NULL AS completed,source_refs
         FROM giga_review_resonances WHERE candidate_id=$1",
    )
    .bind(candidate_id)
    .fetch_one(pool)
    .await?;
    let stored_sources: Json<Value> = stored_resonance.try_get("source_refs")?;
    require(
        stored_resonance.try_get::<String, _>("event_id")? == resonance_event_id
            && stored_resonance.try_get::<f64, _>("score")? == 0.91
            && stored_resonance.try_get::<String, _>("classifier_model")? == "resonance-model"
            && stored_resonance.try_get::<String, _>("classifier_provider_type")? == "ollama"
            && stored_resonance.try_get::<String, _>("classifier_model_version")?
                == "resonance-manifest"
            && stored_resonance.try_get::<String, _>("classifier_prompt_version")?
                == "resonance-prompt"
            && stored_resonance.try_get::<String, _>("classifier_configuration_digest")?
                == "e".repeat(64)
            && stored_resonance.try_get::<String, _>("classifier_run_id")? == "resonance-run"
            && stored_resonance.try_get::<bool, _>("completed")?
            && stored_sources.0[0]["source_id"] == resonance_source.source_id()
            && stored_sources.0[0]["content_hash"] == resonance_source.content_hash()
            && stored_sources.0[0]["timestamp"] == resonance_source.timestamp(),
        "the full typed resonance and new-event source association must be durable",
    )?;

    let health_before =
        giga_health(pool, GigaHealthParams { room: ROOM.into() }.try_into()?).await?;
    stage_candidate_in_room(
        pool,
        OTHER_ROOM,
        "other-room-private",
        GigaCandidateKind::Memory,
        None,
        false,
        '9',
    )
    .await?;
    let health_after =
        giga_health(pool, GigaHealthParams { room: ROOM.into() }.try_into()?).await?;
    require(
        health_before.queue_depth == health_after.queue_depth
            && health_before.processed_count == health_after.processed_count
            && health_before.failed_count == health_after.failed_count
            && health_before.candidates_by_kind_state == health_after.candidates_by_kind_state,
        "room health must not aggregate events or candidates from another room",
    )?;
    let listed = giga_candidate_list(
        pool,
        GigaCandidateListParams {
            room: ROOM.into(),
            review_state: None,
            limit: 200,
        }
        .try_into()?,
    )
    .await?;
    require(
        listed.candidates.iter().all(|candidate| {
            candidate.room == ROOM && candidate.candidate_id != "other-room-private"
        }),
        "ordinary candidate listing must remain private to its requested room",
    )
}

fn promotion_request(
    candidate_id: &str,
    room: &str,
    source_refs: Vec<GigaSourceRef>,
    payload: GigaPromotionPayload,
    reviewed_at: &str,
) -> TestResult<GigaPromotionRequest> {
    Ok(GigaPromotionRequest::new(
        candidate_id.into(),
        RoomKey::new(room)?,
        "governing-spirit".into(),
        "test-operator".into(),
        "explicit deliberate publication approval".into(),
        source_refs,
        payload,
        reviewed_at.into(),
    )?)
}

#[derive(Debug, Eq, PartialEq)]
struct CandidateEffects {
    memories: i64,
    coding_lessons: i64,
    project_lessons: i64,
    promotion_reviews: i64,
    review_state: String,
    promotion_refs: Vec<String>,
}

async fn candidate_effects(pool: &PgPool, candidate_id: &str) -> TestResult<CandidateEffects> {
    let row: (i64, i64, i64, i64, String, Json<Vec<String>>) = sqlx::query_as(
        "SELECT
           (SELECT count(*)::bigint FROM memories WHERE meta->>'candidate_id'=$1),
           (SELECT count(*)::bigint FROM lessons WHERE lesson_key='coding' AND meta->>'candidate_id'=$1),
           (SELECT count(*)::bigint FROM lessons WHERE lesson_key='project' AND meta->>'candidate_id'=$1),
           (SELECT count(*)::bigint FROM giga_reviews WHERE candidate_id=$1 AND action='promote'),
           review_state,promotion_refs
         FROM giga_candidates WHERE candidate_id=$1",
    )
    .bind(candidate_id)
    .fetch_one(pool)
    .await?;
    Ok(CandidateEffects {
        memories: row.0,
        coding_lessons: row.1,
        project_lessons: row.2,
        promotion_reviews: row.3,
        review_state: row.4,
        promotion_refs: row.5.0,
    })
}

async fn promotion_contracts(pool: &PgPool, cfg: &Config) -> TestResult {
    let memory_source = stage_candidate(
        pool,
        "promote-memory",
        GigaCandidateKind::Memory,
        None,
        false,
        '2',
    )
    .await?;
    let memory_payload = GigaPromotionPayload::Memory(GigaMemoryPromotionPayload::new(
        "Edited durable memory".into(),
        "The reviewed exact source supports this durable memory.".into(),
        vec!["atomicity".into(), "integration".into()],
    )?);
    let memory_request = promotion_request(
        "promote-memory",
        ROOM,
        vec![memory_source.clone()],
        memory_payload,
        REVIEWED_AT,
    )?;
    let memory_receipt = giga_promote(pool, cfg, memory_request.clone()).await?;
    require(
        memory_receipt.durable_kind() == GigaPromotionKind::Memory
            && memory_receipt.review_state() == GigaReviewState::Promoted
            && memory_receipt.authority() == GigaPromotionAuthority::Full,
        "memory promotion must return a full-authority durable receipt",
    )?;
    let memory_retry = giga_promote(pool, cfg, memory_request).await?;
    require(
        memory_retry.durable_id() == memory_receipt.durable_id()
            && memory_retry.durable_kind() == memory_receipt.durable_kind(),
        "the exact promotion retry must return the original durable identity",
    )?;
    let memory_effects = candidate_effects(pool, "promote-memory").await?;
    require(
        memory_effects.memories == 1
            && memory_effects.coding_lessons == 0
            && memory_effects.project_lessons == 0
            && memory_effects.promotion_reviews == 1
            && memory_effects.review_state == "promoted"
            && memory_effects.promotion_refs
                == vec![format!("memory:{}", memory_receipt.durable_id())],
        "an exact promotion retry must not duplicate any durable or review write",
    )?;
    let memory_row = sqlx::query("SELECT room,title,body,threads,meta FROM memories WHERE id=$1")
        .bind(i64::try_from(memory_receipt.durable_id())?)
        .fetch_one(pool)
        .await?;
    let memory_meta: Json<Value> = memory_row.try_get("meta")?;
    let candidate_created_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT created_at FROM giga_candidates WHERE candidate_id=$1")
            .bind("promote-memory")
            .fetch_one(pool)
            .await?;
    require(
        memory_row.try_get::<String, _>("room")? == ROOM
            && memory_row.try_get::<Option<String>, _>("title")?.as_deref()
                == Some("Edited durable memory")
            && memory_row.try_get::<String, _>("body")?
                == "The reviewed exact source supports this durable memory."
            && memory_row.try_get::<Vec<String>, _>("threads")?
                == vec!["atomicity".to_owned(), "integration".to_owned()]
            && memory_meta.0["candidate_id"] == "promote-memory"
            && memory_meta.0["origin"] == "giga-promotion"
            && memory_meta.0["giga"]["durability"].as_f64() == Some(0.9)
            && memory_meta.0["giga"]["decay_anchor"] == "candidate_created_at"
            && candidate_created_at.to_rfc3339() == "2030-01-01T00:30:01+00:00"
            && memory_meta.0["giga"]["decay_anchor_at"] == "2030-01-01T00:30:01+00:00",
        "memory promotion must persist the edited payload and GIGA provenance",
    )?;
    let memory_chunks: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM memory_chunks WHERE memory_id=$1")
            .bind(i64::try_from(memory_receipt.durable_id())?)
            .fetch_one(pool)
            .await?;
    let memory_threads: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
         FROM thread_events e
         JOIN threads t ON t.id=e.thread_id
         WHERE e.memory_id=$1 AND t.room=$2",
    )
    .bind(i64::try_from(memory_receipt.durable_id())?)
    .bind(ROOM)
    .fetch_one(pool)
    .await?;
    require(
        memory_chunks > 0 && memory_threads == 2,
        format!(
            "memory promotion must atomically persist recall chunks and thread links; chunks={memory_chunks}, threads={memory_threads}"
        ),
    )?;

    let altered_retry = promotion_request(
        "promote-memory",
        ROOM,
        vec![memory_source],
        GigaPromotionPayload::Memory(GigaMemoryPromotionPayload::new(
            "Different edited memory".into(),
            "The reviewed exact source supports this durable memory.".into(),
            vec!["atomicity".into(), "integration".into()],
        )?),
        REVIEWED_AT,
    )?;
    let altered_result = giga_promote(pool, cfg, altered_retry).await;
    require(
        altered_result.is_err(),
        "idempotency must accept only an exact retry of the authorized request",
    )?;
    require(
        candidate_effects(pool, "promote-memory").await? == memory_effects,
        "a changed retry must not alter the committed durable promotion",
    )?;

    let coding_source = stage_candidate(
        pool,
        "promote-coding",
        GigaCandidateKind::CodingLesson,
        None,
        false,
        '3',
    )
    .await?;
    let existing_coding_id: i64 = sqlx::query_scalar(
        "INSERT INTO lessons (lesson_key,scope,title,lesson,tags,meta)
         VALUES ('coding',$1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(ROOM)
    .bind("Verify atomic writes")
    .bind("older canonical lesson body")
    .bind(Vec::<String>::new())
    .bind(Json(
        serde_json::json!({"origin":"direct-remember-fixture"}),
    ))
    .fetch_one(pool)
    .await?;
    let coding_receipt = giga_promote(
        pool,
        cfg,
        promotion_request(
            "promote-coding",
            ROOM,
            vec![coding_source],
            GigaPromotionPayload::CodingLesson(GigaCodingLessonPromotionPayload::new(
                "Verify atomic writes".into(),
                "Exercise the real transaction and prove every coupled write.".into(),
                Some("database transaction".into()),
                "a late failure leaves zero durable rows".into(),
                "when promoting reviewed classifier output".into(),
                vec!["rust".into()],
                vec!["postgresql".into()],
                vec!["subagent-dispatch".into()],
                vec!["atomic".into(), "postgres".into()],
            )?),
            "2030-01-01T01:01:00Z",
        )?,
    )
    .await?;
    require(
        coding_receipt.durable_id() == u64::try_from(existing_coding_id)?,
        "coding promotion must reuse the canonical direct-remember upsert identity",
    )?;
    require(
        coding_receipt.durable_kind() == GigaPromotionKind::CodingLesson,
        "coding lesson promotion must return its typed durable identity",
    )?;
    let coding_row = sqlx::query(
        "SELECT scope,title,lesson,shape,proof_pattern,trigger_context,thread_keys,tags,meta
         FROM lessons WHERE lesson_key='coding' AND id=$1",
    )
    .bind(i64::try_from(coding_receipt.durable_id())?)
    .fetch_one(pool)
    .await?;
    let coding_meta: Json<Value> = coding_row.try_get("meta")?;
    require(
        coding_row.try_get::<String, _>("scope")? == ROOM
            && coding_row.try_get::<String, _>("title")? == "Verify atomic writes"
            && coding_row.try_get::<String, _>("lesson")?
                == "Exercise the real transaction and prove every coupled write."
            && coding_row.try_get::<Option<String>, _>("shape")?.as_deref()
                == Some("database transaction")
            && coding_row
                .try_get::<Option<String>, _>("proof_pattern")?
                .as_deref()
                == Some("a late failure leaves zero durable rows")
            && coding_row
                .try_get::<Option<String>, _>("trigger_context")?
                .as_deref()
                == Some("when promoting reviewed classifier output")
            && coding_row.try_get::<Vec<String>, _>("thread_keys")?
                == vec!["subagent-dispatch".to_owned()]
            && coding_row.try_get::<Vec<String>, _>("tags")?
                == vec!["atomic".to_owned(), "postgres".to_owned()]
            && coding_meta.0["candidate_id"] == "promote-coding"
            && coding_meta.0["origin_room"] == ROOM,
        "coding lesson promotion must persist private room scope, edited fields, and provenance",
    )?;

    let project_source = stage_candidate(
        pool,
        "promote-project",
        GigaCandidateKind::ProjectLesson,
        Some(PROJECT),
        true,
        '4',
    )
    .await?;
    let existing_project_id: i64 = sqlx::query_scalar(
        "INSERT INTO lessons (lesson_key,project,title,lesson,tags,meta)
         VALUES ('project',$1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(PROJECT)
    .bind("Use the isolated database guard")
    .bind("older canonical project lesson body")
    .bind(Vec::<String>::new())
    .bind(Json(
        serde_json::json!({"origin":"direct-remember-fixture"}),
    ))
    .fetch_one(pool)
    .await?;
    let project_receipt = giga_promote(
        pool,
        cfg,
        promotion_request(
            "promote-project",
            ROOM,
            vec![project_source],
            GigaPromotionPayload::ProjectLesson {
                payload: GigaProjectLessonPromotionPayload::new(
                    "Use the isolated database guard".into(),
                    "All live database proofs must run under a generated test schema.".into(),
                    PROJECT.into(),
                    "the schema is dropped after success or failure".into(),
                    "when a proof needs real PostgreSQL concurrency".into(),
                    vec![],
                    vec!["postgresql".into()],
                    vec!["subagent-dispatch".into()],
                    vec!["integration".into(), "isolation".into()],
                )?,
                publication_consent: GigaPublicationConsent::new(true)?,
            },
            "2030-01-01T01:02:00Z",
        )?,
    )
    .await?;
    require(
        project_receipt.durable_id() == u64::try_from(existing_project_id)?,
        "project promotion must reuse the canonical direct-remember upsert identity",
    )?;
    require(
        project_receipt.durable_kind() == GigaPromotionKind::ProjectLesson,
        "project lesson promotion must return its typed durable identity",
    )?;
    let project_row = sqlx::query(
        "SELECT project,title,lesson,proof_pattern,trigger_context,thread_keys,tags,meta
         FROM lessons WHERE lesson_key='project' AND id=$1",
    )
    .bind(i64::try_from(project_receipt.durable_id())?)
    .fetch_one(pool)
    .await?;
    let project_meta: Json<Value> = project_row.try_get("meta")?;
    require(
        project_row.try_get::<String, _>("project")? == PROJECT
            && project_row.try_get::<String, _>("title")? == "Use the isolated database guard"
            && project_row.try_get::<String, _>("lesson")?
                == "All live database proofs must run under a generated test schema."
            && project_row
                .try_get::<Option<String>, _>("proof_pattern")?
                .as_deref()
                == Some("the schema is dropped after success or failure")
            && project_row
                .try_get::<Option<String>, _>("trigger_context")?
                .as_deref()
                == Some("when a proof needs real PostgreSQL concurrency")
            && project_row.try_get::<Vec<String>, _>("thread_keys")?
                == vec!["subagent-dispatch".to_owned()]
            && project_row.try_get::<Vec<String>, _>("tags")?
                == vec!["integration".to_owned(), "isolation".to_owned()]
            && project_meta.0["candidate_id"] == "promote-project"
            && project_meta.0["publication_consent"]["operator_approved"] == true
            && project_meta.0["publication_consent"]["reviewer_approved"] == true,
        "project lesson promotion must persist the exact project and dual publication approval",
    )?;

    let refusal_source = stage_candidate(
        pool,
        "refuse-memory",
        GigaCandidateKind::Memory,
        None,
        false,
        '5',
    )
    .await?;
    let refusal_before = candidate_effects(pool, "refuse-memory").await?;
    let stale_source = GigaSourceRef::new(
        refusal_source.source_type(),
        refusal_source.source_id().into(),
        refusal_source.role().into(),
        refusal_source.timestamp().into(),
        "6".repeat(64),
        refusal_source.scope().clone(),
        None,
    )?;
    let hash_result = giga_promote(
        pool,
        cfg,
        promotion_request(
            "refuse-memory",
            ROOM,
            vec![stale_source],
            GigaPromotionPayload::Memory(GigaMemoryPromotionPayload::new(
                "Must not persist".into(),
                "stale hash".into(),
                vec![],
            )?),
            "2030-01-01T01:03:00Z",
        )?,
    )
    .await;
    require(
        hash_result.is_err(),
        "promotion must refuse a stale source hash",
    )?;
    require(
        candidate_effects(pool, "refuse-memory").await? == refusal_before,
        "hash refusal must leave no durable, review, or candidate-state partial write",
    )?;

    let cross_room_source = GigaSourceRef::new(
        refusal_source.source_type(),
        refusal_source.source_id().into(),
        refusal_source.role().into(),
        refusal_source.timestamp().into(),
        refusal_source.content_hash().into(),
        GigaScope::new(
            Some(OTHER_ROOM.into()),
            None,
            GigaVisibility::Private,
            false,
        )?,
        None,
    )?;
    let room_result = giga_promote(
        pool,
        cfg,
        promotion_request(
            "refuse-memory",
            OTHER_ROOM,
            vec![cross_room_source],
            GigaPromotionPayload::Memory(GigaMemoryPromotionPayload::new(
                "Must not persist".into(),
                "cross-room request".into(),
                vec![],
            )?),
            "2030-01-01T01:03:01Z",
        )?,
    )
    .await;
    require(
        room_result.is_err(),
        "promotion must refuse a cross-room request",
    )?;
    require(
        candidate_effects(pool, "refuse-memory").await? == refusal_before,
        "room refusal must leave no durable, review, or candidate-state partial write",
    )?;

    require(
        GigaPublicationConsent::new(false).is_err(),
        "project authorization must require operator approval",
    )?;
    let auth_source = stage_candidate(
        pool,
        "refuse-auth",
        GigaCandidateKind::ProjectLesson,
        Some(PROJECT),
        false,
        '7',
    )
    .await?;
    let auth_before = candidate_effects(pool, "refuse-auth").await?;
    let auth_result = giga_promote(
        pool,
        cfg,
        promotion_request(
            "refuse-auth",
            ROOM,
            vec![auth_source],
            GigaPromotionPayload::ProjectLesson {
                payload: GigaProjectLessonPromotionPayload::new(
                    "Must not persist".into(),
                    "publication review was not required by the source".into(),
                    PROJECT.into(),
                    "source scope omits publication review authority".into(),
                    "a private candidate requests project publication".into(),
                    vec![],
                    vec![],
                    vec![],
                    vec![],
                )?,
                publication_consent: GigaPublicationConsent::new(true)?,
            },
            "2030-01-01T01:04:00Z",
        )?,
    )
    .await;
    require(
        auth_result.is_err(),
        "dual approval must not manufacture publication authority absent from source scope",
    )?;
    require(
        candidate_effects(pool, "refuse-auth").await? == auth_before,
        "authorization refusal must leave no durable, review, or candidate-state partial write",
    )?;

    let project_refusal_source = stage_candidate(
        pool,
        "refuse-project",
        GigaCandidateKind::ProjectLesson,
        Some(PROJECT),
        true,
        '8',
    )
    .await?;
    let project_before = candidate_effects(pool, "refuse-project").await?;
    let project_result = giga_promote(
        pool,
        cfg,
        promotion_request(
            "refuse-project",
            ROOM,
            vec![project_refusal_source],
            GigaPromotionPayload::ProjectLesson {
                payload: GigaProjectLessonPromotionPayload::new(
                    "Must not persist".into(),
                    "project mismatch".into(),
                    "different-project".into(),
                    "candidate and payload projects differ".into(),
                    "a reviewed project lesson is promoted".into(),
                    vec![],
                    vec![],
                    vec![],
                    vec![],
                )?,
                publication_consent: GigaPublicationConsent::new(true)?,
            },
            "2030-01-01T01:05:00Z",
        )?,
    )
    .await;
    require(
        project_result.is_err(),
        "promotion must refuse a payload project that differs from candidate scope",
    )?;
    require(
        candidate_effects(pool, "refuse-project").await? == project_before,
        "project refusal must leave no durable, review, or candidate-state partial write",
    )?;

    let rollback_source = stage_candidate(
        pool,
        "rollback-late",
        GigaCandidateKind::Memory,
        None,
        false,
        '9',
    )
    .await?;
    sqlx::query(
        "INSERT INTO giga_reviews
         (candidate_id,action,reviewer_principal,operator_identity,authorization_basis,
          previous_state,new_state,reason,target_refs,reviewed_at,promotion_request_digest)
         VALUES ($1,'promote','conflict-reviewer','conflict-operator','conflict-authority',
                 'in_review','promoted','deliberate late-transaction conflict','[]'::jsonb,
                 '2030-01-01T00:59:00Z'::timestamptz,$2)",
    )
    .bind("rollback-late")
    .bind("f".repeat(64))
    .execute(pool)
    .await?;
    let rollback_before = candidate_effects(pool, "rollback-late").await?;
    let rollback_result = giga_promote(
        pool,
        cfg,
        promotion_request(
            "rollback-late",
            ROOM,
            vec![rollback_source],
            GigaPromotionPayload::Memory(GigaMemoryPromotionPayload::new(
                "Rolled back memory".into(),
                "This row is inserted before the forced review conflict.".into(),
                vec!["rollback".into()],
            )?),
            "2030-01-01T01:06:00Z",
        )?,
    )
    .await;
    require(
        rollback_result.is_err(),
        "a late review uniqueness failure must abort promotion",
    )?;
    require(
        candidate_effects(pool, "rollback-late").await? == rollback_before,
        "a failure after the durable insert must roll back the durable row and candidate mutation",
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL database"]
async fn queue_and_atomic_promotion_contracts() {
    let url = isolated_database_url();
    let options = PgConnectOptions::from_str(&url).expect("dedicated test URL must be valid");
    let cfg = Config {
        database_url: url.clone(),
        embed_url: None,
        embed_model: "test-disabled".into(),
        embed_dimension: 2_048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "America/Sao_Paulo".into(),
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .expect("isolated database must be reachable");
    let schema = format!("solarisael_giga_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .expect("isolated GIGA schema must create");

    let connection_schema = schema.clone();
    let pool_result = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |connection, _meta| {
            let schema = connection_schema.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO {schema}, public"))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await;

    let result: TestResult = match pool_result {
        Ok(pool) => {
            let result: TestResult = async {
                run_migrations(&pool).await?;
                sqlx::raw_sql(migration!("0005_giga_resonance.sql"))
                    .execute(&pool)
                    .await?;
                persisted_process_contract(&pool, &cfg).await?;
                queue_contracts(&pool).await?;
                queue_maintenance_contracts(&pool).await?;
                review_resonance_and_room_scope_contracts(&pool).await?;
                promotion_contracts(&pool, &cfg).await?;
                Ok(())
            }
            .await;
            pool.close().await;
            result
        }
        Err(error) => Err(error.into()),
    };

    let cleanup = sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await;
    admin.close().await;
    cleanup.expect("isolated GIGA schema cleanup must succeed");
    result.expect("queue and atomic promotion integration contract");
}
