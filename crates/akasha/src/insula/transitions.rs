// The transition log. insula_writer.rs queues each move a Hearth machine makes and
// pushes it through here; the constellation reads the daily edge counts.

use super::{InsulaError, TrustedBinding, validate_trusted_binding};
use chrono::{DateTime, Duration, Utc};
use hearth::machines::Transition;
use sqlx::PgPool;
use uuid::Uuid;

// enough: one month of raw rows, sized for Hallway knocks; measure again before a busy
// machine (the GIGA queue) emits, or when insula.transitions passes a few hundred MB.
pub const TRANSITION_RETENTION_DAYS: i64 = 30;

pub struct TransitionRow {
    pub event_id: Uuid,
    pub writer_id: Uuid,
    pub writer_sequence: i64,
    pub observed_at: DateTime<Utc>,
    pub transition: Transition,
}

/// Inserts the rows and adds each new row to its daily edge count in one statement.
/// A replayed `event_id` inserts nothing and counts nothing while its raw row exists; the
/// writer never retries, so no replay arrives after retention.
pub async fn ingest_transitions(
    pool: &PgPool,
    b: &TrustedBinding,
    rows: &[TransitionRow],
) -> Result<(), InsulaError> {
    validate_trusted_binding(b)?;

    let rows: Vec<serde_json::Value> = rows.iter().map(|row| json_row(b, row)).collect();
    sqlx::query(
        "WITH new AS (
             INSERT INTO insula.transitions
             SELECT * FROM jsonb_populate_recordset(NULL::insula.transitions, $1)
             ON CONFLICT DO NOTHING
             RETURNING house_id,machine,from_state,event,to_state,observed_at
         )
         INSERT INTO insula.transitions_daily AS d
             (day,house_id,machine,from_state,event,to_state,transition_count)
         SELECT (observed_at AT TIME ZONE 'UTC')::date,house_id,machine,from_state,event,to_state,
                COUNT(*)
         FROM new
         GROUP BY 1,2,3,4,5,6
         ON CONFLICT (house_id,machine,day,from_state,event,to_state)
         DO UPDATE SET transition_count=d.transition_count+EXCLUDED.transition_count",
    )
    .bind(serde_json::Value::Array(rows))
    .execute(pool)
    .await?;
    Ok(())
}

/// Deletes one house's raw transitions older than `TRANSITION_RETENTION_DAYS`.
/// The daily counts stay.
pub async fn run_transition_retention(
    pool: &PgPool,
    house_id: &str,
    now: DateTime<Utc>,
) -> Result<(), InsulaError> {
    sqlx::query("DELETE FROM insula.transitions WHERE house_id=$1 AND observed_at<$2")
        .bind(house_id)
        .bind(now - Duration::days(TRANSITION_RETENTION_DAYS))
        .execute(pool)
        .await?;
    Ok(())
}

// Keys are insula.transitions column names; the table itself is the only schema.
fn json_row(b: &TrustedBinding, row: &TransitionRow) -> serde_json::Value {
    let t = &row.transition;
    serde_json::json!({
        "event_id": row.event_id.to_string(),
        "writer_id": row.writer_id.to_string(),
        "writer_sequence": row.writer_sequence,
        "house_id": b.house_id,
        "room": b.room,
        "spirit": b.spirit,
        "session_id": b.session_id,
        "machine": t.machine,
        "subject_id": t.subject_id,
        "from_state": t.from,
        "event": t.event,
        "to_state": t.to,
        "observed_at": row.observed_at,
    })
}
