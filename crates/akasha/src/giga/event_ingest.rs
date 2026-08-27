use crate::AppError;
use hearth::GigaEvent;
use protocol::{GigaEventIngestDisposition, GigaEventIngestResult};
use sqlx::{PgPool, types::Json};
use super::clock::timestamp;
use super::lifecycle::lifecycle_json;
use super::sources::insert_event_source;

pub async fn giga_event_ingest(
    pool: &PgPool,
    event: GigaEvent,
) -> Result<GigaEventIngestResult, AppError> {
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO giga_events
         (event_schema_version,event_id,event_type,room,session_id,project_keys,lifecycle,created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(i32::from(event.event_schema_version()))
    .bind(event.event_id())
    .bind(event.event_type().as_str())
    .bind(event.room().to_string())
    .bind(event.session_id())
    .bind(event.project_keys().to_vec())
    .bind(Json(lifecycle_json(&event)))
    .bind(timestamp(event.created_at())?)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(GigaEventIngestResult {
            event_id: event.event_id().into(),
            disposition: GigaEventIngestDisposition::Duplicate,
        });
    }
    for (source_ordinal, source) in event.source_refs().iter().enumerate() {
        insert_event_source(&mut tx, event.event_id(), source_ordinal, source).await?;
    }
    tx.commit().await?;
    Ok(GigaEventIngestResult {
        event_id: event.event_id().into(),
        disposition: GigaEventIngestDisposition::Accepted,
    })
}
