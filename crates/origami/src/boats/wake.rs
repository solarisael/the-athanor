
use chrono::{DateTime, NaiveDate, Utc};
use house_core::{
    PAPER_BOAT_MAX_BODY_BYTES, PAPER_BOAT_MAX_UNBOATED, PaperBoatRecord, UnboatedMemory,
};
use sqlx::{PgPool, Row};

use super::MEMORY_KIND;
use super::error::{BoatError, BoatResult};
use super::record::{
    MAX_KIND_BYTES, MAX_SOURCE_PATH_BYTES, MAX_TITLE_BYTES, bounded_utf8, positive_id,
};

const UNTITLED: &str = "untitled";

/// The newest boat for a room, with the warnings the read collected.
/// `boat` is `None` when the room has never cast one — an explicit
/// absence, never an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WokenBoat {
    pub boat: Option<PaperBoatRecord>,
    pub warnings: Vec<String>,
}

pub async fn wake(pool: &PgPool, room: &str) -> BoatResult<WokenBoat> {
    let row = sqlx::query(
        "SELECT id,title,body,date,source_path,created_at
         FROM memories
         WHERE room=$1 AND type=$2
         ORDER BY created_at DESC,id DESC
         LIMIT 1",
    )
    .bind(room)
    .bind(MEMORY_KIND)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(WokenBoat {
            boat: None,
            warnings: Vec::new(),
        });
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

    let (unboated, unboated_truncated) = unboated_tail(pool, room, created_at, id).await?;
    if unboated_truncated {
        warnings.push(format!(
            "unboated memory list clipped to {PAPER_BOAT_MAX_UNBOATED} records"
        ));
    }

    let title: Option<String> = row.try_get("title")?;
    let date: Option<NaiveDate> = row.try_get("date")?;
    let source_path: String = row.try_get("source_path")?;
    Ok(WokenBoat {
        boat: Some(PaperBoatRecord {
            id: positive_id(id)?,
            title: bounded_utf8(title.as_deref().unwrap_or(UNTITLED), MAX_TITLE_BYTES).0,
            body,
            date: date.map(|value| value.to_string()),
            source_path: bounded_utf8(&source_path, MAX_SOURCE_PATH_BYTES).0,
            created_at: created_at.to_rfc3339(),
            unboated,
            unboated_truncated,
        }),
        warnings,
    })
}

pub async fn unboated_tail(
    pool: &PgPool,
    room: &str,
    after_created_at: DateTime<Utc>,
    after_id: i64,
) -> BoatResult<(Vec<UnboatedMemory>, bool)> {
    let mut rows = sqlx::query(
        "SELECT id,COALESCE(title,$1) AS title,type,source_path,created_at
         FROM memories
         WHERE room=$2 AND type<>$3
           AND (created_at,id) > ($4,$5)
         ORDER BY created_at ASC,id ASC
         LIMIT $6",
    )
    .bind(UNTITLED)
    .bind(room)
    .bind(MEMORY_KIND)
    .bind(after_created_at)
    .bind(after_id)
    .bind(i64::try_from(PAPER_BOAT_MAX_UNBOATED + 1).expect("bounded unboated limit fits i64"))
    .fetch_all(pool)
    .await?;
    let truncated = rows.len() > PAPER_BOAT_MAX_UNBOATED;
    rows.truncate(PAPER_BOAT_MAX_UNBOATED);
    let unboated = rows
        .into_iter()
        .map(|row| {
            let id: i64 = row.try_get("id")?;
            let title: String = row.try_get("title")?;
            let kind: String = row.try_get("type")?;
            let source_path: String = row.try_get("source_path")?;
            let created_at: DateTime<Utc> = row.try_get("created_at")?;
            Ok(UnboatedMemory {
                id: positive_id(id)?,
                title: bounded_utf8(&title, MAX_TITLE_BYTES).0,
                kind: bounded_utf8(&kind, MAX_KIND_BYTES).0,
                source_path: bounded_utf8(&source_path, MAX_SOURCE_PATH_BYTES).0,
                created_at: created_at.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>, BoatError>>()?;
    Ok((unboated, truncated))
}
