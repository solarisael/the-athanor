//! Adapter: the substrate's hallway door onto [`origami::hallways`].

use crate::{AppError, Config};
use hearth::hallway::{
    HallwayCreateRequest, HallwayInboxReceipt, HallwayInboxRequest, HallwayJoinRequest,
    HallwayKnockClaimReceipt, HallwayKnockClaimRequest, HallwayKnockPolicyReceipt,
    HallwayKnockPolicyRequest, HallwayKnockReceipt, HallwayKnockRequest, HallwayKnockSettleReceipt,
    HallwayKnockSettleRequest, HallwayMessagesPage, HallwayMessagesRequest, HallwayPostReceipt,
    HallwayPostRequest, HallwayPresenceReceipt, HallwayReadReceipt, HallwayReadRequest,
    HallwayReceipt,
};
use origami::hallways::{HallwayError, channels, knocks, messages};
use sqlx::PgPool;

fn app_error(error: HallwayError) -> AppError {
    match error {
        HallwayError::Invalid(message) => AppError::Invalid(message),
        HallwayError::Refusal { code, message } => AppError::Refusal { code, message },
        HallwayError::Config(message) => AppError::Config(message),
        HallwayError::Database(error) => AppError::Database(error),
    }
}

pub async fn hallway_create(
    pool: &PgPool,
    request: HallwayCreateRequest,
) -> Result<HallwayReceipt, AppError> {
    channels::create(pool, request).await.map_err(app_error)
}

pub async fn hallway_join(
    pool: &PgPool,
    request: HallwayJoinRequest,
) -> Result<HallwayPresenceReceipt, AppError> {
    channels::join(pool, request).await.map_err(app_error)
}

pub async fn hallway_post(
    pool: &PgPool,
    config: &Config,
    request: HallwayPostRequest,
) -> Result<HallwayPostReceipt, AppError> {
    let house_tz = config.house_timezone(pool, &request.room).await?;
    let idempotency_key = request.idempotency_key.clone();
    let receipt = messages::post(pool, &house_tz, request).await.map_err(app_error)?;
    // The write stands when the sea is down; turn-boundary inbox reads reconcile it.
    let published = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        publish_hallway_post(pool, config.nats_url.as_deref(), &receipt, &idempotency_key),
    ).await;
    let reason = match published {
        Ok(Ok(())) => None,
        Ok(Err(reason)) => Some(reason),
        Err(_) => Some("publish_timeout".into()),
    };
    if let Some(reason) = reason {
        let mut binding = crate::insula_writer::system_binding();
        binding.room = receipt.message.room.clone();
        binding.spirit = receipt.message.spirit.clone();
        crate::insula_writer::record_point(
            &binding, "akasha", "origami", "hallway.publish",
            crate::OutcomeClass::Error, Some(&reason), None,
        );
    }
    Ok(receipt)
}

async fn publish_hallway_post(
    pool: &PgPool,
    nats_url: Option<&str>,
    receipt: &HallwayPostReceipt,
    idempotency_key: &str,
) -> Result<(), String> {
    let url = nats_url.ok_or_else(|| "nats_not_configured".to_string())?;
    let broker = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        origami::cranes::broker::Broker::connect(url),
    ).await.map_err(|_| "connect_timeout".to_string())?
        .map_err(|_| "connect_failed".to_string())?;
    let projection = origami::hallways::sea::HallwayPostProjection::from_receipt(receipt);
    let allowed_rooms: Vec<String> = if projection.to_rooms.is_empty() {
        sqlx::query_scalar(
            "SELECT a.room FROM hallway_allowed_rooms a JOIN hallway_channels c ON c.id=a.hallway_id WHERE c.hallway_key=$1 ORDER BY a.room",
        ).bind(&projection.hallway).fetch_all(pool).await
            .map_err(|_| "recipient_lookup_failed".to_string())?
    } else {
        Vec::new()
    };
    let published = broker.publish_hallway(&projection, &allowed_rooms, idempotency_key).await;
    let drained = broker.drain().await;
    published.map_err(|_| "publish_failed".to_string())?;
    drained.map_err(|_| "drain_failed".to_string())
}

pub async fn hallway_read(
    pool: &PgPool,
    request: HallwayReadRequest,
) -> Result<HallwayReadReceipt, AppError> {
    messages::read(pool, request).await.map_err(app_error)
}

pub async fn hallway_inbox(
    pool: &PgPool,
    request: HallwayInboxRequest,
) -> Result<HallwayInboxReceipt, AppError> {
    messages::inbox(pool, request).await.map_err(app_error)
}

pub async fn hallway_messages(
    pool: &PgPool,
    request: HallwayMessagesRequest,
) -> Result<HallwayMessagesPage, AppError> {
    messages::page(pool, request).await.map_err(app_error)
}

pub async fn hallway_knock_policy(
    pool: &PgPool,
    request: HallwayKnockPolicyRequest,
) -> Result<HallwayKnockPolicyReceipt, AppError> {
    knocks::policy(pool, request).await.map_err(app_error)
}

pub async fn hallway_knock(
    pool: &PgPool,
    request: HallwayKnockRequest,
) -> Result<HallwayKnockReceipt, AppError> {
    knocks::knock(pool, request).await.map_err(app_error)
}

pub async fn hallway_knock_claim(
    pool: &PgPool,
    request: HallwayKnockClaimRequest,
) -> Result<HallwayKnockClaimReceipt, AppError> {
    knocks::claim(pool, request).await.map_err(app_error)
}

pub async fn hallway_knock_settle(
    pool: &PgPool,
    request: HallwayKnockSettleRequest,
) -> Result<HallwayKnockSettleReceipt, AppError> {
    knocks::settle(pool, request).await.map_err(app_error)
}
