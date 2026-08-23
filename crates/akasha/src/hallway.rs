//! Adapter: the substrate's hallway door onto [`origami::hallways`].

use crate::{AppError, Config};
use hearth::hallway::{
    HallwayCreateRequest, HallwayInboxReceipt, HallwayInboxRequest, HallwayJoinRequest,
    HallwayKnockClaimReceipt, HallwayKnockClaimRequest, HallwayKnockPolicyReceipt,
    HallwayKnockPolicyRequest, HallwayKnockReceipt, HallwayKnockRequest, HallwayKnockSettleReceipt,
    HallwayKnockSettleRequest, HallwayPostReceipt, HallwayPostRequest, HallwayPresenceReceipt,
    HallwayReadReceipt, HallwayReadRequest, HallwayReceipt,
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
    messages::post(pool, &house_tz, request)
        .await
        .map_err(app_error)
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
