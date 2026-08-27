use chrono::{DateTime, Utc};
use protocol::restart::RestartState;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use super::binding::is_room;
use super::error::{InsulaError, bad};

// enough: a restart storm is bounded to three exits per workspace per hour, so
// a hundred newest-first rows cover days of unverified exits; upgrade path is a
// keyset cursor on (exiting_at, intent_id), not a bigger cap.
pub const INSULA_MAX_UNVERIFIED_EXIT_ROWS: u32 = 100;
const UNVERIFIED_EXIT: &str = "insula.session.unverified_exit";

/// One session that armed a restart exit and never came back verified. The
/// restart plane owns these columns (`restart.intents`); this family only
/// observes them, which is why the row carries no writer or span identity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnverifiedExitRow {
    pub intent_id: String,
    pub workspace: String,
    pub session_id: Option<String>,
    pub mode: String,
    pub state: String,
    pub failed_stage: Option<String>,
    pub requester_room: String,
    pub requester_spirit: String,
    pub requester_session: String,
    pub exiting_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnverifiedExitResult {
    pub query_name: String,
    pub query_version: i16,
    /// The room this read was scoped to. The rows carry a workspace path and a
    /// requester session, so the answer names whose divergence it reports.
    pub room: String,
    pub window_secs: i64,
    pub rows: Vec<UnverifiedExitRow>,
}

/// Sessions whose restart intent reached `exiting` and never reached
/// `verified` inside the stage window, for one room only. The rows carry a
/// workspace path and a requester identity, so the room comes from the Host's
/// own binding and never from the caller (Kintsu's Insula verdict, 2026-08-25).
///
/// The window comes from the restart module's const block — one authority for
/// the deadline, so a policy change here can never drift from the plane that
/// enforces it.
pub async fn query_unverified_exit(
    pool: &PgPool,
    room: &str,
    limit: u32,
) -> Result<UnverifiedExitResult, InsulaError> {
    if !is_room(room) {
        return Err(bad("room", "invalid_room_key"));
    }
    if limit == 0 || limit > INSULA_MAX_UNVERIFIED_EXIT_ROWS {
        return Err(bad("limit", "out_of_range"));
    }
    let window_secs =
        crate::restart::EXITING_DEADLINE_SECS + crate::restart::RELAUNCHING_DEADLINE_SECS;
    // The first exiting event is the one that starts the clock: a retry never
    // buys a session more silence.
    let rs = sqlx::query(
        "SELECT intent.intent_id::text intent_id,intent.workspace,intent.session_id,intent.mode,intent.state,intent.failed_stage,intent.requester_room,intent.requester_spirit,intent.requester_session,exit_event.created_at exiting_at,exit_event.created_at+($1*INTERVAL '1 second')deadline_at FROM restart.intents intent JOIN LATERAL(SELECT created_at FROM restart.intent_events WHERE intent_id=intent.intent_id AND event_kind=$2 ORDER BY created_at LIMIT 1)exit_event ON TRUE WHERE intent.requester_room=$3 AND intent.verified_at IS NULL AND exit_event.created_at+($1*INTERVAL '1 second')<=NOW() ORDER BY exit_event.created_at DESC LIMIT $4",
    )
    .bind(window_secs)
    .bind(RestartState::Exiting.as_str())
    .bind(room)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    let rows = rs
        .into_iter()
        .map(|r| {
            Ok(UnverifiedExitRow {
                intent_id: r.try_get("intent_id")?,
                workspace: r.try_get("workspace")?,
                session_id: r.try_get("session_id")?,
                mode: r.try_get("mode")?,
                state: r.try_get("state")?,
                failed_stage: r.try_get("failed_stage")?,
                requester_room: r.try_get("requester_room")?,
                requester_spirit: r.try_get("requester_spirit")?,
                requester_session: r.try_get("requester_session")?,
                exiting_at: r.try_get("exiting_at")?,
                deadline_at: r.try_get("deadline_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(UnverifiedExitResult {
        query_name: UNVERIFIED_EXIT.into(),
        query_version: 1,
        room: room.to_owned(),
        window_secs,
        rows,
    })
}
