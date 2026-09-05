//! The substrate door for the boat shape.

use crate::backup;
use crate::config::{AppError, Config};
use crate::remember::{prepare_memory_write, write_memory_tx};
use crate::settings::RoomSettings;
use chrono::Utc;
use hearth::{BackupOutcome, RoomKey};
use origami::boats;
use origami::boats::error::BoatError;
use origami::boats::record::{MAX_WARNING_BYTES, bounded_utf8, positive_id};
use sqlx::PgPool;
use summoning::{
    PaperBoatSleepReceipt, PaperBoatSleepRequest, PaperBoatWakeReceipt, PaperBoatWakeRequest,
};

impl From<BoatError> for AppError {
    fn from(error: BoatError) -> Self {
        match error {
            BoatError::Invalid(message) => Self::Invalid(message),
            BoatError::Database(error) => Self::Database(error),
        }
    }
}

pub async fn paper_boat_sleep(
    pool: &PgPool,
    cfg: &Config,
    request: PaperBoatSleepRequest,
) -> Result<PaperBoatSleepReceipt, AppError> {
    let room = request.room().as_str();
    let settings = RoomSettings::load(pool, room).await?;
    let body = request.body();
    let plan = boats::sleep::plan(room, body, Utc::now());
    let mut prepared = prepare_memory_write(
        cfg,
        &settings,
        &plan.source_path,
        body,
        &plan.threads,
        plan.date,
    )
    .await?;
    let mut warnings = std::mem::take(&mut prepared.warnings);

    let mut tx = pool.begin().await?;
    let (memory_id, inserted) = write_memory_tx(
        &mut tx,
        room,
        boats::MEMORY_KIND,
        &plan.title,
        &plan.source_path,
        body,
        &[],
        plan.metadata,
        &prepared,
    )
    .await?;
    let outbox_event_id = boats::sleep::ready_pointer(&mut tx, memory_id).await?;
    tx.commit().await?;

    // The boat is durable once committed; a failed backup is a typed outcome
    // on the receipt and one bounded warning line, never a failed write.
    let backup = backup::post_write_outcome(
        pool,
        &cfg.database_url,
        settings.backup_keep_count,
        request.backup(),
    )
    .await;
    if let Some(warning) = backup.warning() {
        warnings.push(bounded_utf8(&warning, MAX_WARNING_BYTES).0);
    }
    committed_sleep_receipt(
        memory_id,
        request.room().clone(),
        plan.source_path,
        outbox_event_id,
        inserted,
        backup,
        warnings,
    )
}

pub async fn paper_boat_wake(
    pool: &PgPool,
    request: PaperBoatWakeRequest,
) -> Result<PaperBoatWakeReceipt, AppError> {
    let woken = boats::wake::wake(pool, request.room().as_str()).await?;
    PaperBoatWakeReceipt::new(request.room().clone(), woken.boat, woken.warnings)
        .map_err(|error| AppError::Invalid(error.to_string()))
}

fn committed_sleep_receipt(
    memory_id: i64,
    room: RoomKey,
    source_path: String,
    outbox_event_id: String,
    inserted: bool,
    backup: BackupOutcome,
    warnings: Vec<String>,
) -> Result<PaperBoatSleepReceipt, AppError> {
    PaperBoatSleepReceipt::committed(
        positive_id(memory_id)?,
        room,
        source_path,
        outbox_event_id,
        inserted,
        backup,
        warnings,
    )
    .map_err(|error| AppError::Invalid(error.to_string()))
}
