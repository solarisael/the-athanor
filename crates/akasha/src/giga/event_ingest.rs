use super::clock::timestamp;
use super::lifecycle::lifecycle_json;
use super::sources::insert_event_source;
use crate::AppError;
use hearth::GigaEvent;
use protocol::{GigaEventIngestDisposition, GigaEventIngestResult};
use sqlx::{PgPool, Postgres, Transaction, types::Json};

/// Writes one event and its sources. A waiting event is a captured turn that
/// stays unclaimable until its conversation batch is released; any other event
/// is claimable at once. Returns false, writing nothing, when the id is stored.
pub(super) async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &GigaEvent,
    waiting: bool,
) -> Result<bool, AppError> {
    let inserted = sqlx::query(
        "INSERT INTO giga_events
         (event_schema_version,event_id,event_type,room,session_id,project_keys,lifecycle,created_at,
          waiting_since)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,CASE WHEN $9 THEN NOW() END)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(i32::from(event.event_schema_version()))
    .bind(event.event_id())
    .bind(event.event_type().as_str())
    .bind(event.room().to_string())
    .bind(event.session_id())
    .bind(event.project_keys().to_vec())
    .bind(Json(lifecycle_json(event)))
    .bind(timestamp(event.created_at())?)
    .bind(waiting)
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(false);
    }
    for (source_ordinal, source) in event.source_refs().iter().enumerate() {
        insert_event_source(tx, event.event_id(), source_ordinal, source).await?;
    }
    Ok(true)
}

pub async fn giga_event_ingest(
    pool: &PgPool,
    event: GigaEvent,
) -> Result<GigaEventIngestResult, AppError> {
    let mut tx = pool.begin().await?;
    let inserted = insert_event(&mut tx, &event, false).await?;
    tx.commit().await?;
    Ok(GigaEventIngestResult {
        event_id: event.event_id().into(),
        disposition: if inserted {
            GigaEventIngestDisposition::Accepted
        } else {
            GigaEventIngestDisposition::Duplicate
        },
    })
}
