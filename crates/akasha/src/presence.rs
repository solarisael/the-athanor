//! Presence sessions: one row per OMP session, keyed by the session id.
//!
//! The row is the authority for the frame and the ledger. The Host keeps a
//! cache in memory so a turn does not pay a read for state it already holds;
//! a Host that restarts reads the row back and continues. Contracts are not
//! stored: a contract expires with its turn by design.

use crate::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, types::Json};
use summoning::presence::{PresenceBinding, PresenceFrame, PresenceLedger};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresenceSessionRow {
    pub session_id: String,
    pub room: String,
    pub spirit: String,
    pub operator: String,
    pub frame: PresenceFrame,
    pub ledger: PresenceLedger,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub last_turn_at: Option<DateTime<Utc>>,
}

impl PresenceSessionRow {
    pub fn is_live(&self) -> bool {
        self.closed_at.is_none()
    }
}

fn session_key(session_id: &str) -> Result<&str, AppError> {
    let key = session_id.trim();
    if key.is_empty() {
        return Err(AppError::Invalid("presence session id is empty".into()));
    }
    Ok(key)
}

fn row(record: sqlx::postgres::PgRow) -> Result<PresenceSessionRow, AppError> {
    let frame: Json<PresenceFrame> = record.try_get("frame")?;
    let ledger: Json<PresenceLedger> = record.try_get("ledger")?;
    Ok(PresenceSessionRow {
        session_id: record.try_get("session_id")?,
        room: record.try_get("room")?,
        spirit: record.try_get("spirit")?,
        operator: record.try_get("operator")?,
        frame: frame.0,
        ledger: ledger.0,
        opened_at: record.try_get("opened_at")?,
        closed_at: record.try_get("closed_at")?,
        last_turn_at: record.try_get("last_turn_at")?,
    })
}

const COLUMNS: &str =
    "session_id, room, spirit, operator, frame, ledger, opened_at, closed_at, last_turn_at";

/// The row for a session, live or closed. `None` means the session never
/// opened a presence.
pub async fn presence_session_load(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<PresenceSessionRow>, AppError> {
    let key = session_key(session_id)?;
    let record = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM presence_sessions WHERE session_id = $1"
    ))
    .bind(key)
    .fetch_optional(pool)
    .await?;
    record.map(row).transpose()
}

/// Open or reopen the session's presence with this frame and ledger.
///
/// A new session inserts. A closed session reopens in place: `closed_at`
/// clears, `opened_at` moves to now, and the ledger the caller passes is
/// stored as given, so a reopen may carry the closed session's ledger
/// forward. A live session is rewritten with the given frame and ledger.
pub async fn presence_session_open(
    pool: &PgPool,
    binding: &PresenceBinding,
    frame: &PresenceFrame,
    ledger: &PresenceLedger,
) -> Result<PresenceSessionRow, AppError> {
    let key = session_key(&binding.session)?;
    let record = sqlx::query(&format!(
        "INSERT INTO presence_sessions \
             (session_id, room, spirit, operator, frame, ledger) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (session_id) DO UPDATE SET \
             room = EXCLUDED.room, \
             spirit = EXCLUDED.spirit, \
             operator = EXCLUDED.operator, \
             frame = EXCLUDED.frame, \
             ledger = EXCLUDED.ledger, \
             opened_at = CASE WHEN presence_sessions.closed_at IS NULL \
                              THEN presence_sessions.opened_at ELSE now() END, \
             closed_at = NULL, \
             updated_at = now() \
         RETURNING {COLUMNS}"
    ))
    .bind(key)
    .bind(&binding.room)
    .bind(&binding.spirit)
    .bind(&binding.operator)
    .bind(Json(frame))
    .bind(Json(ledger))
    .fetch_one(pool)
    .await?;
    row(record)
}

/// Write the ledger back after a compile or a settle and mark the turn.
/// Refuses a session that is not live: a closed presence learns nothing.
pub async fn presence_session_write_ledger(
    pool: &PgPool,
    session_id: &str,
    ledger: &PresenceLedger,
) -> Result<(), AppError> {
    let key = session_key(session_id)?;
    let updated = sqlx::query(
        "UPDATE presence_sessions \
            SET ledger = $2, last_turn_at = now(), updated_at = now() \
          WHERE session_id = $1 AND closed_at IS NULL",
    )
    .bind(key)
    .bind(Json(ledger))
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::Refusal {
            code: "presence_not_live",
            message: "presence session is not live",
        });
    }
    Ok(())
}

/// Close the session's presence. Closing an already closed or unknown
/// session is not an error: the row, if any, stays as it was.
pub async fn presence_session_close(
    pool: &PgPool,
    session_id: &str,
    ledger: &PresenceLedger,
) -> Result<(), AppError> {
    let key = session_key(session_id)?;
    sqlx::query(
        "UPDATE presence_sessions \
            SET ledger = $2, closed_at = now(), updated_at = now() \
          WHERE session_id = $1 AND closed_at IS NULL",
    )
    .bind(key)
    .bind(Json(ledger))
    .execute(pool)
    .await?;
    Ok(())
}
