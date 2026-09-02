use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};

use super::error::BoatResult;
use super::identity::{self, DIGEST_LABEL};
use super::{EVENT_KIND, SLEEP_ORIGIN, THREAD_KEY};

const TITLE_PREFIX: &str = "paper boat — ";

const OUTBOX_AGGREGATE_KIND: &str = "memory";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoatPlan {
    pub source_path: String,
    pub title: String,
    pub date: NaiveDate,
    pub threads: Vec<String>,
    pub metadata: Value,
}

pub fn plan(room: &str, body: &str, now: DateTime<Utc>) -> BoatPlan {
    let source_path = identity::source_identity(room, body);
    let digest = identity::digest_of(&source_path).unwrap_or_default();
    let date = now.date_naive();
    BoatPlan {
        title: format!("{TITLE_PREFIX}{date}"),
        date,
        threads: vec![THREAD_KEY.to_owned()],
        metadata: json!({
            "origin": SLEEP_ORIGIN,
            "recorded_at": now.to_rfc3339(),
            "identity": format!("{DIGEST_LABEL}:{digest}"),
        }),
        source_path,
    }
}

/// The one place boats touch cranes: the 0016 trigger enqueued this
/// pointer in the same transaction we're still inside.
pub async fn ready_pointer(
    tx: &mut Transaction<'_, Postgres>,
    memory_id: i64,
) -> BoatResult<String> {
    let event_id: String = sqlx::query_scalar(
        "SELECT event_id::text FROM crane_outbox
         WHERE aggregate_kind=$1 AND aggregate_id=$2 AND event_kind=$3",
    )
    .bind(OUTBOX_AGGREGATE_KIND)
    .bind(memory_id)
    .bind(EVENT_KIND)
    .fetch_one(&mut **tx)
    .await?;
    Ok(event_id)
}
