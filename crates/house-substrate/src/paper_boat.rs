use crate::backup;
use crate::config::{AppError, Config};
use crate::remember::{prepare_memory_write, write_memory_tx};
use chrono::{DateTime, NaiveDate, Utc};
use house_core::{
    PAPER_BOAT_MAX_BODY_BYTES, PAPER_BOAT_MAX_UNBOATED, PaperBoatBackupStatus, PaperBoatRecord,
    PaperBoatSleepReceipt, PaperBoatSleepRequest, PaperBoatWakeReceipt, PaperBoatWakeRequest,
    RoomKey, UnboatedMemory,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

const PAPER_BOAT_THREAD: &str = "paper boat / sleep / for tomorrow";
const MAX_TITLE_BYTES: usize = 512;
const MAX_SOURCE_PATH_BYTES: usize = 2_048;
const MAX_KIND_BYTES: usize = 128;

pub async fn paper_boat_sleep(
    pool: &PgPool,
    cfg: &Config,
    request: PaperBoatSleepRequest,
) -> Result<PaperBoatSleepReceipt, AppError> {
    let room = request.room().as_str();
    let body = request.body();
    let source_path = paper_boat_source_path(room, body);
    let identity = source_path
        .strip_prefix("db-only/paper-boats/sha256-")
        .and_then(|value| value.strip_suffix(".md"))
        .unwrap_or_default();
    let now = Utc::now();
    let title = format!("paper boat — {}", now.date_naive());
    let threads = vec![PAPER_BOAT_THREAD.to_owned()];
    let mut prepared =
        prepare_memory_write(cfg, &source_path, body, &threads, now.date_naive()).await?;
    let mut warnings = std::mem::take(&mut prepared.warnings);
    let metadata = serde_json::json!({
        "origin": "paper-boat-sleep",
        "recorded_at": now.to_rfc3339(),
        "identity": format!("sha256:{identity}"),
    });

    let mut tx = pool.begin().await?;
    let (memory_id, inserted) = write_memory_tx(
        &mut tx,
        room,
        "paper-boat",
        &title,
        &source_path,
        body,
        &[],
        metadata,
        &prepared,
    )
    .await?;
    let outbox_event_id: String = sqlx::query_scalar(
        "SELECT event_id::text FROM crane_outbox
         WHERE aggregate_kind='memory' AND aggregate_id=$1 AND event_kind='boat.ready'",
    )
    .bind(memory_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    let backup_status = if !request.backup() {
        PaperBoatBackupStatus::NotRequested
    } else {
        match backup::run_post_write(pool, &cfg.database_url).await {
            Ok(()) => PaperBoatBackupStatus::Completed,
            Err(error) => {
                let warning = format!(
                    "backup failed after PostgreSQL commit; paper boat remains durable: {error}"
                );
                warnings.push(bounded_utf8(&warning, 4_096).0);
                PaperBoatBackupStatus::Failed
            }
        }
    };
    committed_sleep_receipt(
        memory_id,
        request.room().clone(),
        source_path,
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
    let room = request.room().as_str();
    let row = sqlx::query(
        "SELECT id,title,body,date,source_path,created_at
         FROM memories
         WHERE room=$1 AND type='paper-boat'
         ORDER BY created_at DESC,id DESC
         LIMIT 1",
    )
    .bind(room)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return PaperBoatWakeReceipt::new(request.room().clone(), None, Vec::new())
            .map_err(|error| AppError::Invalid(error.to_string()));
    };

    let id: i64 = row.try_get("id")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let mut warnings = Vec::new();
    let raw_body: String = row.try_get("body")?;
    let (body, body_clipped) = bounded_utf8(&raw_body, PAPER_BOAT_MAX_BODY_BYTES);
    if body_clipped {
        warnings.push(format!(
            "paper boat body clipped to {PAPER_BOAT_MAX_BODY_BYTES} UTF-8 bytes"
        ));
    }

    let mut later_rows = sqlx::query(
        "SELECT id,COALESCE(title,'untitled') AS title,type,source_path,created_at
         FROM memories
         WHERE room=$1 AND type<>'paper-boat'
           AND (created_at,id) > ($2,$3)
         ORDER BY created_at ASC,id ASC
         LIMIT $4",
    )
    .bind(room)
    .bind(created_at)
    .bind(id)
    .bind(i64::try_from(PAPER_BOAT_MAX_UNBOATED + 1).expect("bounded unboated limit fits i64"))
    .fetch_all(pool)
    .await?;
    let unboated_truncated = later_rows.len() > PAPER_BOAT_MAX_UNBOATED;
    later_rows.truncate(PAPER_BOAT_MAX_UNBOATED);
    if unboated_truncated {
        warnings.push(format!(
            "unboated memory list clipped to {PAPER_BOAT_MAX_UNBOATED} records"
        ));
    }
    let unboated = later_rows
        .into_iter()
        .map(|row| {
            let later_id: i64 = row.try_get("id")?;
            let title: String = row.try_get("title")?;
            let kind: String = row.try_get("type")?;
            let source_path: String = row.try_get("source_path")?;
            let created_at: DateTime<Utc> = row.try_get("created_at")?;
            Ok(UnboatedMemory {
                id: positive_id(later_id)?,
                title: bounded_utf8(&title, MAX_TITLE_BYTES).0,
                kind: bounded_utf8(&kind, MAX_KIND_BYTES).0,
                source_path: bounded_utf8(&source_path, MAX_SOURCE_PATH_BYTES).0,
                created_at: created_at.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let title: Option<String> = row.try_get("title")?;
    let date: Option<NaiveDate> = row.try_get("date")?;
    let source_path: String = row.try_get("source_path")?;
    let record = PaperBoatRecord {
        id: positive_id(id)?,
        title: bounded_utf8(title.as_deref().unwrap_or("untitled"), MAX_TITLE_BYTES).0,
        body,
        date: date.map(|value| value.to_string()),
        source_path: bounded_utf8(&source_path, MAX_SOURCE_PATH_BYTES).0,
        created_at: created_at.to_rfc3339(),
        unboated,
        unboated_truncated,
    };
    PaperBoatWakeReceipt::new(request.room().clone(), Some(record), warnings)
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

fn positive_id(id: i64) -> Result<u64, AppError> {
    u64::try_from(id)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::Invalid("paper boat database ID must be positive".into()))
}

pub(crate) fn paper_boat_source_path(room: &str, body: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"paper-boat\0");
    digest.update(room.as_bytes());
    digest.update(b"\0");
    digest.update(body.as_bytes());
    format!("db-only/paper-boats/sha256-{:x}.md", digest.finalize())
}

fn bounded_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_is_deterministic_and_room_scoped() {
        let first = paper_boat_source_path("kintsu", "same body");
        assert_eq!(first, paper_boat_source_path("kintsu", "same body"));
        assert_ne!(first, paper_boat_source_path("other-room", "same body"));
        assert_ne!(first, paper_boat_source_path("kintsu", "different body"));
    }

    #[test]
    fn post_commit_backup_failure_receipt_does_not_deny_durability() {
        let receipt = committed_sleep_receipt(
            7,
            RoomKey::for_memory_write("kintsu").unwrap(),
            paper_boat_source_path("kintsu", "body"),
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

    #[test]
    fn bounded_utf8_never_splits_a_character() {
        assert_eq!(bounded_utf8("ab💛cd", 5), ("ab".into(), true));
    }
}
