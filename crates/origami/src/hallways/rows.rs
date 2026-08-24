//! One concern: mapping a validated hearth Hallway request onto the substrate
//! table row it becomes. Every key below is a column name, so the table's DDL
//! in substrate/migrations is the only schema this module has — a renamed or
//! misspelled key arrives as NULL and dies as that column's NOT NULL refusal,
//! never as a silent positional swap between two TEXT columns.
//!
//! Columns that carry a DDL default are absent on purpose. The writers pair
//! each row with an explicit INSERT column list and a matching SELECT list over
//! `jsonb_populate_record`, because `SELECT *` would write NULL over every
//! default — over BIGSERIAL ids and over `created_at DEFAULT NOW()`, moving the
//! Hallway's clock into the driver. Adding a column here means adding it to
//! that writer's two lists; the first insert says so out loud.

use chrono::{DateTime, Utc};
use hearth::hallway::{
    HallwayCreateRequest, HallwayKnockPolicyRequest, HallwayKnockRequest, HallwayPostRequest,
};
use serde_json::{Value, json};

/// `hallway_channels` (0018_hallway_chatrooms.sql:3-16).
///
/// operator_visible, wake_policy, next_sequence, created_at and id stay with
/// their defaults; `create::HallwayReceipt` reports the two a caller can see.
pub(super) fn channel(request: &HallwayCreateRequest, create_digest: &str) -> Value {
    json!({
        "hallway_key": request.hallway,
        "created_by_room": request.room,
        "created_by_spirit": request.spirit,
        "created_by_session": request.session,
        "create_idempotency_key": request.idempotency_key,
        "create_digest": create_digest,
    })
}

/// `hallway_presences` (0018_hallway_chatrooms.sql:25-40).
///
/// Three writers share this row and differ only in conflict policy, so the
/// shape lives here once and the policy stays in each statement. read_cursor,
/// joined_at and last_seen_at keep their defaults; both timestamps must remain
/// the server's clock.
pub(super) fn presence(
    hallway_id: i64,
    room: &str,
    spirit: &str,
    session: &str,
    join_idempotency_key: &str,
    join_digest: &str,
) -> Value {
    json!({
        "hallway_id": hallway_id,
        "room": room,
        "spirit": spirit,
        "session_id": session,
        "join_idempotency_key": join_idempotency_key,
        "join_digest": join_digest,
    })
}

/// `hallway_messages` (0018_hallway_chatrooms.sql:42-59, thread_id and to_rooms
/// added by 0020_hallway_bell.sql:23-26).
///
/// id and created_at stay with their defaults; the writer reads created_at back
/// so the receipt carries the server's timestamp rather than the driver's.
pub(super) fn message(
    hallway_id: i64,
    sequence: i64,
    request: &HallwayPostRequest,
    body_digest: &str,
    thread_id: i64,
) -> Value {
    json!({
        "hallway_id": hallway_id,
        "sequence": sequence,
        "room": request.room,
        "spirit": request.spirit,
        "session_id": request.session,
        "idempotency_key": request.idempotency_key,
        "body": request.body,
        "body_digest": body_digest,
        "reply_to": request.reply_to,
        "thread_id": thread_id,
        "to_rooms": request.to_rooms,
    })
}

/// `hallway_knock_policies` (0021_hallway_knock.sql:9-28).
///
/// Policy rows are append-only command history: id, created_at and
/// superseded_at stay with the DDL, and the caller supersedes the incumbent in
/// its own statement before this row lands.
pub(super) fn knock_policy(
    hallway_id: i64,
    request: &HallwayKnockPolicyRequest,
    request_digest: &str,
    revision: i64,
) -> Value {
    json!({
        "hallway_id": hallway_id,
        "room": request.room,
        "spirit": request.spirit,
        "session_id": request.session,
        "idempotency_key": request.idempotency_key,
        "request_digest": request_digest,
        "mode": request.mode.as_str(),
        "allowed_rooms": request.allowed_rooms,
        "max_turns": i16::from(request.max_turns),
        "revision": revision,
    })
}

/// `hallway_knocks` (0021_hallway_knock.sql:37-87).
///
/// The three UUID columns arrive as JSON strings and are coerced by the record
/// type, which is why no `::uuid` cast survives at the writer. status,
/// created_at and every claim, start and settle column stay NULL or default
/// here: a fresh Knock is pending, and `claim`/`settle` own those columns.
#[allow(clippy::too_many_arguments)]
pub(super) fn knock(
    hallway_id: i64,
    knock_id: &str,
    request: &HallwayKnockRequest,
    request_digest: &str,
    parent_knock_id: Option<&str>,
    root_knock_id: &str,
    turn_index: i16,
    max_turns: i16,
    expires_at: DateTime<Utc>,
) -> Value {
    json!({
        "knock_id": knock_id,
        "hallway_id": hallway_id,
        "message_id": request.message_id,
        "from_room": request.room,
        "from_spirit": request.spirit,
        "request_session": request.session,
        "idempotency_key": request.idempotency_key,
        "request_digest": request_digest,
        "recipient_room": request.recipient_room,
        "parent_knock_id": parent_knock_id,
        "root_knock_id": root_knock_id,
        "turn_index": turn_index,
        "max_turns": max_turns,
        "expires_at": expires_at,
    })
}
