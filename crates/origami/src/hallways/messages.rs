
use super::bells;
use super::channels::{ensure_presence, lookup_id};
use super::errors::{HallwayError, invalid, refusal};
use crate::sea::idempotency_digest;
use hearth::hallway::{
    HallwayInboxEntry, HallwayInboxNotification, HallwayInboxReceipt, HallwayInboxRequest,
    HallwayMessage, HallwayPostDisposition, HallwayPostReceipt, HallwayPostRequest,
    HallwayReadReceipt, HallwayReadRequest,
};
use sqlx::{PgPool, Row};

fn message_from_row(
    row: &sqlx::postgres::PgRow,
    hallway: &str,
) -> Result<HallwayMessage, HallwayError> {
    Ok(HallwayMessage {
        id: row.try_get("id")?,
        sequence: row.try_get("sequence")?,
        hallway: hallway.into(),
        room: row.try_get("room")?,
        spirit: row.try_get("spirit")?,
        session: row.try_get("session_id")?,
        body: row.try_get("body")?,
        reply_to: row.try_get("reply_to")?,
        created_at: row.try_get("created_at_text")?,
        thread: row.try_get("thread_key")?,
        to_rooms: row.try_get("to_rooms")?,
    })
}

pub async fn post(
    pool: &PgPool,
    house_tz: &str,
    mut request: HallwayPostRequest,
) -> Result<HallwayPostReceipt, HallwayError> {
    request.validate().map_err(invalid)?;
    request.to_rooms.sort_unstable();
    let recipient_digest = request.to_rooms.join("\0");
    let body_digest = idempotency_digest(&[
        &request.body,
        &request
            .reply_to
            .map(|id| id.to_string())
            .unwrap_or_default(),
        &request.spirit,
        &recipient_digest,
    ]);
    let mut tx = pool.begin().await?;
    let id = lookup_id(&mut tx, &request.hallway).await?;
    ensure_presence(
        &mut tx,
        id,
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;

    let existing = sqlx::query(
        "SELECT m.id,m.sequence,m.room,m.spirit,m.session_id,m.body,m.reply_to,
                m.created_at::text AS created_at_text,m.to_rooms,
                COALESCE(t.thread_key,'') AS thread_key
         FROM hallway_messages m
         LEFT JOIN hallway_threads t ON t.id=m.thread_id
         WHERE m.hallway_id=$1 AND m.room=$2 AND m.session_id=$3 AND m.idempotency_key=$4",
    )
    .bind(id)
    .bind(&request.room)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = existing {
        let mut stored_to_rooms: Vec<String> = row.try_get("to_rooms")?;
        stored_to_rooms.sort_unstable();
        let same_request = row.try_get::<String, _>("body")? == request.body
            && row.try_get::<Option<i64>, _>("reply_to")? == request.reply_to
            && row.try_get::<String, _>("spirit")? == request.spirit
            && stored_to_rooms == request.to_rooms;
        if !same_request {
            return Err(refusal(
                "idempotency_reuse",
                "idempotency key was reused with different message content or recipients",
            ));
        }
        let message = message_from_row(&row, &request.hallway)?;
        tx.commit().await?;
        return Ok(HallwayPostReceipt {
            ok: true,
            disposition: HallwayPostDisposition::Duplicate,
            message,
        });
    }

    if !request.to_rooms.is_empty() {
        let allowed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM hallway_allowed_rooms WHERE hallway_id=$1 AND room=ANY($2)",
        )
        .bind(id)
        .bind(&request.to_rooms)
        .fetch_one(&mut *tx)
        .await?;
        if allowed as usize != request.to_rooms.len() {
            return Err(refusal(
                "room_not_allowed",
                "toRooms may only name rooms allowed in this hallway",
            ));
        }
    }

    let inherited_thread: Option<i64> = if let Some(reply_to) = request.reply_to {
        let row =
            sqlx::query("SELECT thread_id FROM hallway_messages WHERE hallway_id=$1 AND id=$2")
                .bind(id)
                .bind(reply_to)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(row) = row else {
            return Err(refusal(
                "message_not_found",
                "replyTo does not identify a message in this hallway",
            ));
        };
        row.try_get("thread_id")?
    } else {
        None
    };

    let (thread_id, thread_key) = match inherited_thread {
        Some(thread_id) => {
            let key: String =
                sqlx::query_scalar("SELECT thread_key FROM hallway_threads WHERE id=$1")
                    .bind(thread_id)
                    .fetch_one(&mut *tx)
                    .await?;
            (thread_id, key)
        }
        None => {
            let key: String =
                sqlx::query_scalar("SELECT to_char(NOW() AT TIME ZONE $1, 'YYYY-MM-DD')")
                    .bind(house_tz)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|_| {
                        HallwayError::Config(
                            "SOLARISAEL_HOUSE_TZ is not a timezone PostgreSQL recognizes".into(),
                        )
                    })?;
            sqlx::query(
                "INSERT INTO hallway_threads(hallway_id,thread_key) VALUES($1,$2)
                 ON CONFLICT (hallway_id,thread_key) DO NOTHING",
            )
            .bind(id)
            .bind(&key)
            .execute(&mut *tx)
            .await?;
            let thread_id: i64 = sqlx::query_scalar(
                "SELECT id FROM hallway_threads WHERE hallway_id=$1 AND thread_key=$2",
            )
            .bind(id)
            .bind(&key)
            .fetch_one(&mut *tx)
            .await?;
            (thread_id, key)
        }
    };

    // one UPDATE..RETURNING so a torn write can't skip or share a number
    let sequence: i64 = sqlx::query_scalar(
        "UPDATE hallway_channels SET next_sequence=next_sequence+1
         WHERE id=$1 RETURNING next_sequence-1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let row = sqlx::query(
        "INSERT INTO hallway_messages(
            hallway_id,sequence,room,spirit,session_id,idempotency_key,body,body_digest,
            reply_to,thread_id,to_rooms
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         RETURNING id,created_at::text AS created_at_text",
    )
    .bind(id)
    .bind(sequence)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .bind(&request.body)
    .bind(&body_digest)
    .bind(request.reply_to)
    .bind(thread_id)
    .bind(&request.to_rooms)
    .fetch_one(&mut *tx)
    .await?;
    let message_id: i64 = row.try_get("id")?;

    bells::mint(&mut tx, id, message_id, &request.to_rooms).await?;
    bells::bump_inbox_revisions(&mut tx, id).await?;

    let message = HallwayMessage {
        id: message_id,
        sequence,
        hallway: request.hallway,
        room: request.room,
        spirit: request.spirit,
        session: request.session,
        body: request.body,
        reply_to: request.reply_to,
        created_at: row.try_get("created_at_text")?,
        thread: thread_key,
        to_rooms: request.to_rooms,
    };
    tx.commit().await?;
    Ok(HallwayPostReceipt {
        ok: true,
        disposition: HallwayPostDisposition::Posted,
        message,
    })
}

pub async fn read(
    pool: &PgPool,
    request: HallwayReadRequest,
) -> Result<HallwayReadReceipt, HallwayError> {
    request.validate().map_err(invalid)?;
    let mut tx = pool.begin().await?;
    let id = lookup_id(&mut tx, &request.hallway).await?;
    let previous_cursor = ensure_presence(
        &mut tx,
        id,
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;
    let thread_id = if let Some(thread) = request.thread.as_deref() {
        Some(
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM hallway_threads WHERE hallway_id=$1 AND thread_key=$2",
            )
            .bind(id)
            .bind(thread)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| refusal("thread_not_found", "thread does not exist in this hallway"))?,
        )
    } else {
        None
    };
    let after = request.after.unwrap_or(if thread_id.is_some() {
        0
    } else {
        previous_cursor
    });
    let rows = sqlx::query(
        "SELECT m.id,m.sequence,m.room,m.spirit,m.session_id,m.body,m.reply_to,
                m.created_at::text AS created_at_text,m.to_rooms,
                COALESCE(t.thread_key,'') AS thread_key
         FROM hallway_messages m
         LEFT JOIN hallway_threads t ON t.id=m.thread_id
         WHERE m.hallway_id=$1 AND m.id>$2
           AND ($4::bigint IS NULL OR m.thread_id=$4)
         ORDER BY m.id ASC LIMIT $3",
    )
    .bind(id)
    .bind(after)
    .bind(i64::from(request.limit) + 1)
    .bind(thread_id)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > request.limit as usize;
    let messages = rows
        .iter()
        .take(request.limit as usize)
        .map(|row| message_from_row(row, &request.hallway))
        .collect::<Result<Vec<_>, _>>()?;
    let visible_cursor = messages.last().map_or(after, |message| message.id);

    let mut acked_mentions: i64 = 0;
    let mut room_read_sequence: i64 = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT read_sequence FROM hallway_room_state WHERE hallway_id=$1 AND room=$2",
    )
    .bind(id)
    .bind(&request.room)
    .fetch_optional(&mut *tx)
    .await?
    .flatten()
    .unwrap_or(0);

    let read_cursor = if request.advance_cursor {
        let cursor = if thread_id.is_none() {
            let cursor = previous_cursor.max(visible_cursor);
            sqlx::query(
                "UPDATE hallway_presences SET read_cursor=$4,last_seen_at=NOW()
                 WHERE hallway_id=$1 AND room=$2 AND session_id=$3",
            )
            .bind(id)
            .bind(&request.room)
            .bind(&request.session)
            .bind(cursor)
            .execute(&mut *tx)
            .await?;
            cursor
        } else {
            previous_cursor
        };
        let message_ids: Vec<i64> = messages.iter().map(|message| message.id).collect();
        acked_mentions = bells::acknowledge(&mut tx, id, &request.room, &message_ids).await?;

        let mut state_changed = acked_mentions > 0;
        let mut covered_sequence = room_read_sequence;
        for message in &messages {
            if message.sequence <= covered_sequence {
                continue;
            }
            if message.sequence != covered_sequence + 1 {
                break;
            }
            covered_sequence = message.sequence;
        }
        if covered_sequence > room_read_sequence {
            room_read_sequence = covered_sequence;
            state_changed = true;
        }
        if state_changed {
            sqlx::query(
                "INSERT INTO hallway_room_state(hallway_id,room,read_sequence,notification_revision)
                 VALUES($1,$2,$3,1)
                 ON CONFLICT (hallway_id,room) DO UPDATE
                 SET read_sequence = GREATEST(hallway_room_state.read_sequence,$3),
                     notification_revision = hallway_room_state.notification_revision + 1,
                     updated_at = NOW()",
            )
            .bind(id)
            .bind(&request.room)
            .bind(room_read_sequence)
            .execute(&mut *tx)
            .await?;
        }
        cursor
    } else {
        previous_cursor
    };
    tx.commit().await?;

    Ok(HallwayReadReceipt {
        ok: true,
        hallway: request.hallway,
        room: request.room,
        spirit: request.spirit,
        session: request.session,
        previous_cursor,
        read_cursor,
        messages,
        has_more,
        room_read_sequence,
        acked_mentions,
        thread: request.thread,
        wake_policy: "manual".into(),
    })
}

pub async fn inbox(
    pool: &PgPool,
    request: HallwayInboxRequest,
) -> Result<HallwayInboxReceipt, HallwayError> {
    request.validate().map_err(invalid)?;
    let rows = sqlx::query(
        "SELECT c.hallway_key,
                c.next_sequence - 1 AS latest_sequence,
                (SELECT COUNT(*) FROM hallway_messages unread_message
                  WHERE unread_message.hallway_id=c.id
                    AND unread_message.sequence > COALESCE(s.read_sequence,0)
                    AND unread_message.room <> $1
                ) AS unread,
                COALESCE(s.notification_revision,0) AS notification_revision,
                (SELECT COUNT(*) FROM hallway_notifications n
                  WHERE n.hallway_id=c.id AND n.recipient_room=$1 AND n.read_at IS NULL
                ) AS mentions,
                COALESCE((
                    SELECT jsonb_agg(
                        jsonb_build_object(
                            'messageId',m.id,
                            'sequence',m.sequence,
                            'thread',COALESCE(t.thread_key,'')
                        )
                        ORDER BY m.sequence
                    )
                    FROM hallway_notifications n
                    JOIN hallway_messages m ON m.id=n.message_id
                    LEFT JOIN hallway_threads t ON t.id=m.thread_id
                    WHERE n.hallway_id=c.id
                      AND n.recipient_room=$1
                      AND n.read_at IS NULL
                ), '[]'::jsonb) AS notifications,
                lm.room AS latest_room,
                lm.spirit AS latest_spirit,
                LEFT(lm.body,160) AS latest_excerpt,
                lm.created_at::text AS latest_created_at
         FROM hallway_channels c
         JOIN hallway_allowed_rooms ar ON ar.hallway_id=c.id AND ar.room=$1
         LEFT JOIN hallway_room_state s ON s.hallway_id=c.id AND s.room=$1
         LEFT JOIN LATERAL (
             SELECT room,spirit,body,created_at FROM hallway_messages m
             WHERE m.hallway_id=c.id ORDER BY m.sequence DESC LIMIT 1
         ) lm ON TRUE
         ORDER BY c.hallway_key",
    )
    .bind(&request.room)
    .fetch_all(pool)
    .await?;
    let hallways = rows
        .iter()
        .map(|row| -> Result<HallwayInboxEntry, HallwayError> {
            let latest_sequence: i64 = row.try_get("latest_sequence")?;
            let notifications = serde_json::from_value::<Vec<HallwayInboxNotification>>(
                row.try_get("notifications")?,
            )
            .map_err(|error| invalid(format!("hallway notification row is malformed: {error}")))?;
            Ok(HallwayInboxEntry {
                hallway: row.try_get("hallway_key")?,
                unread: row.try_get("unread")?,
                mentions: row.try_get("mentions")?,
                notification_revision: row.try_get("notification_revision")?,
                latest_sequence,
                latest_room: row.try_get("latest_room")?,
                latest_spirit: row.try_get("latest_spirit")?,
                latest_excerpt: row.try_get("latest_excerpt")?,
                latest_created_at: row.try_get("latest_created_at")?,
                notifications,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HallwayInboxReceipt {
        ok: true,
        room: request.room,
        spirit: request.spirit,
        hallways,
    })
}
