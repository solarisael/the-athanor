//! Adapter: the substrate's hallway door onto [`origami::hallways`].
//!
//! The hallway logic lives in origami now — channels, messages, Bells,
//! and Knocks, one concern per file. This file exists only to speak the
//! substrate's two local dialects: it hands `Config` values in as plain
//! arguments and maps `HallwayError` back to [`AppError`]. Nothing here
//! decides anything; every rule, refusal, and persisted string is
//! origami's.
//!
//! The names below are the ones `main.rs` and `docket.rs` already
//! import, so the stdio routing and the quest clock did not move.
//!
//! Census warnings this adapter carries (2026-08-23, unfixed on
//! purpose): `health.rs` never checks a hallway table, so a green
//! health line says nothing about this family; and stdio mounts seven
//! of the nine functions below — `hallway_knock_claim` and
//! `hallway_knock_settle` are reachable only through house-host.

use crate::{AppError, Config};
use house_core::hallway::{
    HallwayCreateRequest, HallwayInboxReceipt, HallwayInboxRequest, HallwayJoinRequest,
    HallwayKnockClaimReceipt, HallwayKnockClaimRequest, HallwayKnockPolicyReceipt,
    HallwayKnockPolicyRequest, HallwayKnockReceipt, HallwayKnockRequest, HallwayKnockSettleReceipt,
    HallwayKnockSettleRequest, HallwayPostReceipt, HallwayPostRequest, HallwayPresenceReceipt,
    HallwayReadReceipt, HallwayReadRequest, HallwayReceipt,
};
use origami::hallways::{HallwayError, channels, knocks, messages};
use sqlx::PgPool;

/// One rename, no reading: a refusal stays a refusal with its exact code
/// and text, and a database failure stays a database failure.
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

/// The House-local timezone is substrate configuration; origami takes it
/// as the plain value it actually uses.
pub async fn hallway_post(
    pool: &PgPool,
    config: &Config,
    request: HallwayPostRequest,
) -> Result<HallwayPostReceipt, AppError> {
    messages::post(pool, &config.house_tz, request)
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
