use super::error::domain_error;
use super::lifecycle::lifecycle_from_json;
use super::sources::source_from_row;
use crate::AppError;
use chrono::{DateTime, Utc};
use hearth::{GigaEvent, GigaEventType, RoomKey};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction, types::Json};

pub(crate) async fn event_from_store(
    tx: &mut Transaction<'_, Postgres>,
    event_id: &str,
) -> Result<GigaEvent, AppError> {
    let event = sqlx::query(
        "SELECT event_schema_version,event_id,event_type,room,session_id,project_keys,lifecycle,created_at
         FROM giga_events WHERE event_id=$1",
    )
    .bind(event_id)
    .fetch_one(&mut **tx)
    .await?;
    if event.try_get::<i32, _>("event_schema_version")? != 1 {
        return Err(AppError::Invalid(
            "stored GIGA event schema version is unsupported".into(),
        ));
    }
    let event_type =
        GigaEventType::parse(&event.try_get::<String, _>("event_type")?).map_err(domain_error)?;
    let source_rows = sqlx::query(
        "SELECT source_type,source_id,source_role,content_hash,scope_room,scope_project,
                scope_visibility,publication_review_required,range_start,range_end,
                created_at AS source_created_at
         FROM giga_event_sources WHERE event_id=$1 ORDER BY source_ordinal",
    )
    .bind(event_id)
    .fetch_all(&mut **tx)
    .await?;
    let sources = source_rows
        .iter()
        .map(source_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let lifecycle: Json<Value> = event.try_get("lifecycle")?;
    GigaEvent::new(
        event.try_get("event_id")?,
        event_type,
        RoomKey::new(
            event
                .try_get::<Option<String>, _>("room")?
                .ok_or_else(|| AppError::Invalid("stored GIGA event has no room".into()))?,
        )
        .map_err(domain_error)?,
        event.try_get("session_id")?,
        event.try_get("project_keys")?,
        sources,
        lifecycle_from_json(event_type, &lifecycle.0)?,
        event
            .try_get::<DateTime<Utc>, _>("created_at")?
            .to_rfc3339(),
    )
    .map_err(domain_error)
}
