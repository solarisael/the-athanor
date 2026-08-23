//! Cost and state (coding#195): every function here writes inside the
//! caller's transaction and commits nothing itself.

use super::errors::HallwayError;
use sqlx::{Postgres, Transaction};

/// Displaying or delivering Bells never clears them; only a covering read does.
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
