//! The substrate door for the boat shape.

use crate::backup;
use crate::config::{AppError, Config};
use crate::remember::{prepare_memory_write, write_memory_tx};
use crate::settings::RoomSettings;
use chrono::Utc;
use hearth::RoomKey;
use summoning::{
    PaperBoatBackupStatus, PaperBoatSleepReceipt, PaperBoatSleepRequest, PaperBoatWakeReceipt,
    PaperBoatWakeRequest,
};
use origami::boats;
use origami::boats::error::BoatError;
use origami::boats::record::{MAX_WARNING_BYTES, bounded_utf8, positive_id};
use sqlx::PgPool;

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

    let backup_status = if !request.backup() {
        PaperBoatBackupStatus::NotRequested
    } else {
        match backup::run_post_write(pool, &cfg.database_url, settings.backup_keep_count).await {
            Ok(()) => PaperBoatBackupStatus::Completed,
            Err(error) => {
                let warning = format!(
                    "backup failed after PostgreSQL commit; paper boat remains durable: {error}"
                );
                warnings.push(bounded_utf8(&warning, MAX_WARNING_BYTES).0);
                PaperBoatBackupStatus::Failed
            }
        }
    };
    committed_sleep_receipt(
        memory_id,
        request.room().clone(),
        plan.source_path,
        outbox_event_id,
        inserted,
        backup_status,
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
    backup_status: PaperBoatBackupStatus,
    warnings: Vec<String>,
) -> Result<PaperBoatSleepReceipt, AppError> {
    PaperBoatSleepReceipt::committed(
        positive_id(memory_id)?,
        room,
        source_path,
        outbox_event_id,
        inserted,
        backup_status,
        warnings,
    )
    .map_err(|error| AppError::Invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use origami::boats::identity::source_identity;

    #[test]
    fn post_commit_backup_failure_receipt_does_not_deny_durability() {
        let receipt = committed_sleep_receipt(
            7,
            RoomKey::for_memory_write("kintsu").unwrap(),
            source_identity("kintsu", "body"),
            "event-7".into(),
            true,
            PaperBoatBackupStatus::Failed,
            vec!["backup failed after PostgreSQL commit".into()],
        )
        .unwrap();
        assert!(receipt.durable());
        assert_eq!(receipt.backup_status(), PaperBoatBackupStatus::Failed);
        assert_eq!(receipt.warnings().len(), 1);
    }
}
