mod chunking;
mod dates;
mod embedding;
mod lessons;
mod memory_write;
mod normalize;
mod tokens;

pub(crate) use chunking::chunk_body;
pub(crate) use dates::derive_dates;
pub(crate) use embedding::embed;
pub(crate) use lessons::{write_coding_lesson_tx, write_project_lesson_tx};
// On the door because `prepare_memory_write` hands this type back: a caller outside
// this folder that names the returned value needs the path, though none does today.
#[allow(unused_imports)]
pub(crate) use memory_write::PreparedMemoryWrite;
pub(crate) use memory_write::{prepare_memory_write, write_memory_tx};
pub(crate) use normalize::normalize_strings;
pub(crate) use tokens::token_estimate;

use crate::backup;
use crate::config::{AppError, Config};
use crate::settings::RoomSettings;
use chrono::Local;
use hearth::{RememberReceipt, RememberRequest};
use lessons::remember_lesson;
use memory_write::write_continuations_tx;
use protocol::RememberResult;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// A memory that names no source path gets a stable one derived from the
/// fields that make it the same memory on retry.
fn derived_source_path(req: &RememberRequest) -> String {
    let identity = json!({
        "room": req.room().as_str(),
        "title": req.title(),
        "body": req.body(),
        "threads": req.threads(),
        "continues": req.continues().iter().map(|item| {
            (&item.thread, item.previous_memory_id)
        }).collect::<Vec<_>>(),
        "supersedes": req.supersedes(),
    });
    format!(
        "db-only/{}/{:x}",
        req.room(),
        Sha256::digest(identity.to_string())
    )
}

// The row is durable once committed; a failed backup is a warning, never a
// failed write.
pub(super) async fn backup_warning(
    pool: &PgPool,
    cfg: &Config,
    settings: &RoomSettings,
) -> Option<String> {
    backup::run_post_write(pool, &cfg.database_url, settings.backup_keep_count)
        .await
        .err()
        .map(|error| format!("backup failed: {error}"))
}

pub async fn remember(
    pool: &PgPool,
    cfg: &Config,
    req: RememberRequest,
) -> Result<RememberResult, AppError> {
    let settings = RoomSettings::load(pool, req.room().as_str()).await?;
    if req.kind().is_lesson() {
        return remember_lesson(pool, cfg, &settings, &req).await;
    }
    let source_path = req
        .source_path()
        .map_or_else(|| derived_source_path(&req), str::to_owned);
    let mut prepared = prepare_memory_write(
        cfg,
        &settings,
        &source_path,
        req.body(),
        req.threads(),
        Local::now().date_naive(),
    )
    .await?;
    let mut warnings = std::mem::take(&mut prepared.warnings);
    let meta = json!({
        "origin": "direct-db-write",
        "recorded_at": chrono::Utc::now().to_rfc3339(),
    });
    let mut tx = pool.begin().await?;
    let (memory_id, _) = write_memory_tx(
        &mut tx,
        req.room().as_str(),
        "memory",
        req.title(),
        &source_path,
        req.body(),
        req.supersedes(),
        meta,
        &prepared,
    )
    .await?;
    write_continuations_tx(&mut tx, req.room().as_str(), memory_id, req.continues()).await?;
    tx.commit().await?;
    if req.backup() {
        warnings.extend(backup_warning(pool, cfg, &settings).await);
    }
    let receipt = RememberReceipt::committed(
        u64::try_from(memory_id)
            .map_err(|_| AppError::Invalid("database returned an invalid memory ID".into()))?,
        req.room().clone(),
        source_path,
        warnings,
    )
    .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(receipt.into())
}
