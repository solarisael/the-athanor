//! Bells: durable targeted attention rows. A Bell is not a display and
//! not a delivery — it is a row that stays lit until the room it names
//! actually reads the message that minted it.
//!
//! Two doors, both taking the caller's transaction so a Bell can never
//! commit apart from the message it belongs to:
//! [`mint`] lights one Bell per recipient room of a post, and
//! [`acknowledge`] quiets exactly the Bells a covering read returned.
//! [`bump_inbox_revisions`] is the third, smaller fact: a post changes
//! every allowed room's inbox view, whether or not it was targeted.
//!
//! Cost and state (coding#195): every function here writes inside the
//! caller's transaction and commits nothing itself.

use super::errors::HallwayError;
use sqlx::{Postgres, Transaction};

/// Mint targeted Bell rows for a posted message: one durable attention
/// event per recipient room. Displaying or delivering never clears
/// these; only a covering read does.
pub async fn mint(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    message_id: i64,
    recipient_rooms: &[String],
) -> Result<(), HallwayError> {
    for room in recipient_rooms {
        sqlx::query(
            "INSERT INTO hallway_notifications(hallway_id,message_id,recipient_room)
             VALUES($1,$2,$3) ON CONFLICT (message_id,recipient_room) DO NOTHING",
        )
        .bind(hallway_id)
        .bind(message_id)
        .bind(room)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Every allowed room's inbox view changed: bump revisions so projections
/// know to re-inject. Rows are created lazily for rooms that lack one.
pub async fn bump_inbox_revisions(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
) -> Result<(), HallwayError> {
    sqlx::query(
        "INSERT INTO hallway_room_state(hallway_id,room,notification_revision)
         SELECT $1, room, 1 FROM hallway_allowed_rooms WHERE hallway_id=$1
         ON CONFLICT (hallway_id,room) DO UPDATE
         SET notification_revision = hallway_room_state.notification_revision + 1,
             updated_at = NOW()",
    )
    .bind(hallway_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Acknowledge exactly the Bells a covering read returned, and report how
/// many actually quieted.
///
/// The Bell quiets only for what was actually returned: a filtered read
/// must not acknowledge hidden messages. An empty return acknowledges
/// nothing and touches no row.
pub async fn acknowledge(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    room: &str,
    message_ids: &[i64],
) -> Result<i64, HallwayError> {
    if message_ids.is_empty() {
        return Ok(0);
    }
    let acked = sqlx::query(
        "UPDATE hallway_notifications SET read_at=NOW()
                 WHERE hallway_id=$1 AND recipient_room=$2 AND read_at IS NULL
                   AND message_id = ANY($3)",
    )
    .bind(hallway_id)
    .bind(room)
    .bind(message_ids)
    .execute(&mut **tx)
    .await?
    .rows_affected() as i64;
    Ok(acked)
}
