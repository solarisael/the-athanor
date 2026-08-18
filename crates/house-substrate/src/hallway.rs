use crate::{AppError, Config};
use chrono::{DateTime, Duration, Utc};
use house_core::hallway::{
    HallwayCreateDisposition, HallwayCreateRequest, HallwayInboxEntry, HallwayInboxNotification,
    HallwayInboxReceipt, HallwayInboxRequest, HallwayJoinDisposition, HallwayJoinRequest,
    HallwayKnockClaimReceipt, HallwayKnockClaimRequest, HallwayKnockOutcome, HallwayKnockPointer,
    HallwayKnockPolicyMode, HallwayKnockPolicyReceipt, HallwayKnockPolicyRequest,
    HallwayKnockReceipt, HallwayKnockRequest, HallwayKnockSettleReceipt, HallwayKnockSettleRequest,
    HallwayMessage, HallwayPostDisposition, HallwayPostReceipt, HallwayPostRequest,
    HallwayPresenceReceipt, HallwayReadReceipt, HallwayReadRequest, HallwayReceipt,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

fn refusal(code: &'static str, message: &'static str) -> AppError {
    AppError::Refusal { code, message }
}

fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

async fn hallway_id(tx: &mut Transaction<'_, Postgres>, hallway: &str) -> Result<i64, AppError> {
    sqlx::query_scalar("SELECT id FROM hallway_channels WHERE hallway_key=$1")
        .bind(hallway)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| refusal("hallway_not_found", "hallway does not exist"))
}

/// Presence gate with lazy join: an authenticated session whose room is
/// already allowed in the hallway gets its presence row created on first
/// use, so a fresh OMP session never has to remember to join. Refusals are
/// truthful: only a genuinely disallowed room or a spirit conflict turns
/// the caller away.
async fn ensure_presence(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    hallway_key: &str,
    room: &str,
    spirit: &str,
    session: &str,
) -> Result<i64, AppError> {
    // Two passes: select-or-lazily-insert, then the select must hit. The
    // ON CONFLICT DO NOTHING absorbs a concurrent lazy join racing us.
    for _ in 0..2 {
        if let Some(row) = sqlx::query(
            "SELECT read_cursor,spirit FROM hallway_presences
             WHERE hallway_id=$1 AND room=$2 AND session_id=$3
             FOR UPDATE",
        )
        .bind(hallway_id)
        .bind(room)
        .bind(session)
        .fetch_optional(&mut **tx)
        .await?
        {
            let stored_spirit: String = row.try_get("spirit")?;
            if stored_spirit != spirit {
                return Err(refusal(
                    "spirit_mismatch",
                    "session is bound to a different spirit in this hallway",
                ));
            }
            return Ok(row.try_get("read_cursor")?);
        }
        let allowed = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM hallway_allowed_rooms WHERE hallway_id=$1 AND room=$2
             )",
        )
        .bind(hallway_id)
        .bind(room)
        .fetch_one(&mut **tx)
        .await?;
        if !allowed {
            return Err(refusal(
                "room_not_allowed",
                "room is not allowed in this hallway",
            ));
        }
        let join_digest = digest(&[hallway_key, room, spirit, session]);
        sqlx::query(
            "INSERT INTO hallway_presences(
                hallway_id,room,spirit,session_id,join_idempotency_key,join_digest
             ) VALUES($1,$2,$3,$4,$5,$6)
             ON CONFLICT DO NOTHING",
        )
        .bind(hallway_id)
        .bind(room)
        .bind(spirit)
        .bind(session)
        .bind(format!("lazy:{session}"))
        .bind(&join_digest)
        .execute(&mut **tx)
        .await?;
    }
    Err(invalid("hallway presence could not be established"))
}

pub async fn hallway_create(
    pool: &PgPool,
    request: HallwayCreateRequest,
) -> Result<HallwayReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let mut allowed_rooms = request.allowed_rooms.clone();
    allowed_rooms.sort();
    let rooms_digest = allowed_rooms.join("\n");
    let create_digest = digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
        &rooms_digest,
    ]);
    let join_digest = digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    ]);

    let mut tx = pool.begin().await?;
    let inserted = sqlx::query_scalar::<_, i64>(
        "INSERT INTO hallway_channels(
            hallway_key,created_by_room,created_by_spirit,created_by_session,
            create_idempotency_key,create_digest
         ) VALUES($1,$2,$3,$4,$5,$6)
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .bind(&request.hallway)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .bind(&create_digest)
    .fetch_optional(&mut *tx)
    .await?;

    let (id, created) = match inserted {
        Some(id) => (id, true),
        None => {
            let row = sqlx::query(
                "SELECT id,created_by_room,created_by_spirit,created_by_session,
                        create_idempotency_key,create_digest
                 FROM hallway_channels WHERE hallway_key=$1",
            )
            .bind(&request.hallway)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| invalid("idempotency key is already used by another hallway command"))?;
            let matches = row.try_get::<String, _>("created_by_room")? == request.room
                && row.try_get::<String, _>("created_by_spirit")? == request.spirit
                && row.try_get::<String, _>("created_by_session")? == request.session
                && row.try_get::<String, _>("create_idempotency_key")? == request.idempotency_key
                && row.try_get::<String, _>("create_digest")? == create_digest;
            if !matches {
                return Err(refusal(
                    "idempotency_reuse",
                    "hallway already exists with a different create command",
                ));
            }
            (row.try_get("id")?, false)
        }
    };

    if created {
        for room in &allowed_rooms {
            sqlx::query("INSERT INTO hallway_allowed_rooms(hallway_id,room) VALUES($1,$2)")
                .bind(id)
                .bind(room)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO hallway_presences(
                hallway_id,room,spirit,session_id,join_idempotency_key,join_digest
             ) VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(id)
        .bind(&request.room)
        .bind(&request.spirit)
        .bind(&request.session)
        .bind(&request.idempotency_key)
        .bind(&join_digest)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(HallwayReceipt {
        ok: true,
        hallway: request.hallway,
        disposition: if created {
            HallwayCreateDisposition::Created
        } else {
            HallwayCreateDisposition::Duplicate
        },
        operator_visible: true,
        wake_policy: "manual".into(),
    })
}

pub async fn hallway_join(
    pool: &PgPool,
    request: HallwayJoinRequest,
) -> Result<HallwayPresenceReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let join_digest = digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    ]);
    let mut tx = pool.begin().await?;
    let id = hallway_id(&mut tx, &request.hallway).await?;
    sqlx::query("SELECT id FROM hallway_channels WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM hallway_allowed_rooms WHERE hallway_id=$1 AND room=$2
         )",
    )
    .bind(id)
    .bind(&request.room)
    .fetch_one(&mut *tx)
    .await?;
    if !allowed {
        return Err(refusal(
            "room_not_allowed",
            "room is not allowed in this hallway",
        ));
    }

    let existing = sqlx::query(
        "SELECT spirit,join_idempotency_key,join_digest,read_cursor
         FROM hallway_presences WHERE hallway_id=$1 AND room=$2 AND session_id=$3",
    )
    .bind(id)
    .bind(&request.room)
    .bind(&request.session)
    .fetch_optional(&mut *tx)
    .await?;

    let (joined, cursor) = if let Some(row) = existing {
        // A presence row is identity, not a ledger entry: the same session
        // re-joining as the same spirit is a duplicate success no matter
        // which idempotency key (explicit or lazy) created the row.
        let stored_spirit: String = row.try_get("spirit")?;
        if stored_spirit != request.spirit {
            return Err(refusal(
                "spirit_mismatch",
                "session is bound to a different spirit in this hallway",
            ));
        }
        (false, row.try_get("read_cursor")?)
    } else {
        sqlx::query(
            "INSERT INTO hallway_presences(
                hallway_id,room,spirit,session_id,join_idempotency_key,join_digest
             ) VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(id)
        .bind(&request.room)
        .bind(&request.spirit)
        .bind(&request.session)
        .bind(&request.idempotency_key)
        .bind(&join_digest)
        .execute(&mut *tx)
        .await?;
        (true, 0)
    };
    tx.commit().await?;

    Ok(HallwayPresenceReceipt {
        ok: true,
        hallway: request.hallway,
        room: request.room,
        spirit: request.spirit,
        session: request.session,
        disposition: if joined {
            HallwayJoinDisposition::Joined
        } else {
            HallwayJoinDisposition::Duplicate
        },
        read_cursor: cursor,
    })
}

fn message_from_row(
    row: &sqlx::postgres::PgRow,
    hallway: &str,
) -> Result<HallwayMessage, AppError> {
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

pub async fn hallway_post(
    pool: &PgPool,
    config: &Config,
    mut request: HallwayPostRequest,
) -> Result<HallwayPostReceipt, AppError> {
    request.validate().map_err(invalid)?;
    request.to_rooms.sort_unstable();
    let recipient_digest = request.to_rooms.join("\0");
    let body_digest = digest(&[
        &request.body,
        &request
            .reply_to
            .map(|id| id.to_string())
            .unwrap_or_default(),
        &request.spirit,
        &recipient_digest,
    ]);
    let mut tx = pool.begin().await?;
    let id = hallway_id(&mut tx, &request.hallway).await?;
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

    // Structured recipients are stable room keys and must be members here.
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

    // Replies inherit the parent's thread even across midnight, so a living
    // conversation is never chopped by the date boundary.
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
            // First new top-level message of the House-local day lazily and
            // idempotently creates that day's thread.
            let key: String =
                sqlx::query_scalar("SELECT to_char(NOW() AT TIME ZONE $1, 'YYYY-MM-DD')")
                    .bind(&config.house_tz)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|_| {
                        AppError::Config(
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

    // Durable Bell rows: one targeted attention event per recipient room.
    // Displaying or delivering never clears these; only a covering read does.
    for room in &request.to_rooms {
        sqlx::query(
            "INSERT INTO hallway_notifications(hallway_id,message_id,recipient_room)
             VALUES($1,$2,$3) ON CONFLICT (message_id,recipient_room) DO NOTHING",
        )
        .bind(id)
        .bind(message_id)
        .bind(room)
        .execute(&mut *tx)
        .await?;
    }
    // Every allowed room's inbox view changed: bump revisions so projections
    // know to re-inject. Rows are created lazily for rooms that lack one.
    sqlx::query(
        "INSERT INTO hallway_room_state(hallway_id,room,notification_revision)
         SELECT $1, room, 1 FROM hallway_allowed_rooms WHERE hallway_id=$1
         ON CONFLICT (hallway_id,room) DO UPDATE
         SET notification_revision = hallway_room_state.notification_revision + 1,
             updated_at = NOW()",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

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

pub async fn hallway_read(
    pool: &PgPool,
    request: HallwayReadRequest,
) -> Result<HallwayReadReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let mut tx = pool.begin().await?;
    let id = hallway_id(&mut tx, &request.hallway).await?;
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
        // A filtered thread must never advance the session-global delivery
        // cursor past messages from other threads that were not returned.
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
        // The Bell quiets only for what was actually returned: a filtered
        // read must not acknowledge hidden messages.
        let message_ids: Vec<i64> = messages.iter().map(|message| message.id).collect();
        if !message_ids.is_empty() {
            acked_mentions = sqlx::query(
                "UPDATE hallway_notifications SET read_at=NOW()
                 WHERE hallway_id=$1 AND recipient_room=$2 AND read_at IS NULL
                   AND message_id = ANY($3)",
            )
            .bind(id)
            .bind(&request.room)
            .bind(&message_ids)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;
        }

        // Advance only through the exact contiguous global sequence returned.
        // Filtered thread rows may contain gaps occupied by other threads.
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

/// Room-level inbox: which persistent Hallways can this room open, how much
/// ordinary unread waits in each (derived, never stored), and how many
/// targeted mentions are pending. Reading the inbox clears nothing.
pub async fn hallway_inbox(
    pool: &PgPool,
    request: HallwayInboxRequest,
) -> Result<HallwayInboxReceipt, AppError> {
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
        .map(|row| -> Result<HallwayInboxEntry, AppError> {
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

fn policy_mode_from_db(value: &str) -> Result<HallwayKnockPolicyMode, AppError> {
    match value {
        "manual" => Ok(HallwayKnockPolicyMode::Manual),
        "allow_list" => Ok(HallwayKnockPolicyMode::AllowList),
        _ => Err(refusal(
            "knock_state_conflict",
            "stored Hallway Knock policy has an invalid mode",
        )),
    }
}

fn policy_receipt_from_row(
    row: &sqlx::postgres::PgRow,
    hallway: String,
    room: String,
    duplicate: bool,
) -> Result<HallwayKnockPolicyReceipt, AppError> {
    let mode: String = row.try_get("mode")?;
    let max_turns: i16 = row.try_get("max_turns")?;
    Ok(HallwayKnockPolicyReceipt {
        ok: true,
        duplicate,
        hallway,
        room,
        mode: policy_mode_from_db(&mode)?,
        allowed_rooms: row.try_get("allowed_rooms")?,
        max_turns: max_turns as u8,
        revision: row.try_get("revision")?,
    })
}

pub async fn hallway_knock_policy(
    pool: &PgPool,
    mut request: HallwayKnockPolicyRequest,
) -> Result<HallwayKnockPolicyReceipt, AppError> {
    request.validate().map_err(invalid)?;
    request.allowed_rooms.sort_unstable();
    let allowed_digest = request.allowed_rooms.join("\0");
    let request_digest = digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
        request.mode.as_str(),
        &allowed_digest,
        &request.max_turns.to_string(),
    ]);
    let mut tx = pool.begin().await?;
    let id = hallway_id(&mut tx, &request.hallway).await?;
    let sibling_binding: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM hallway_presences
            WHERE hallway_id=$1 AND session_id=$2 AND (room<>$3 OR spirit<>$4)
         )",
    )
    .bind(id)
    .bind(&request.session)
    .bind(&request.room)
    .bind(&request.spirit)
    .fetch_one(&mut *tx)
    .await?;
    if sibling_binding {
        return Err(refusal(
            "room_not_allowed",
            "session cannot set a sibling room's Hallway Knock policy",
        ));
    }
    ensure_presence(
        &mut tx,
        id,
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;

    if let Some(row) = sqlx::query(
        "SELECT request_digest,mode,allowed_rooms,max_turns,revision
         FROM hallway_knock_policies
         WHERE hallway_id=$1 AND room=$2 AND session_id=$3 AND idempotency_key=$4",
    )
    .bind(id)
    .bind(&request.room)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.try_get::<String, _>("request_digest")? != request_digest {
            return Err(refusal(
                "idempotency_reuse",
                "idempotency key was reused with a different Hallway Knock policy request",
            ));
        }
        let receipt = policy_receipt_from_row(&row, request.hallway, request.room, true)?;
        tx.commit().await?;
        return Ok(receipt);
    }

    if !request.allowed_rooms.is_empty() {
        let allowed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM hallway_allowed_rooms
             WHERE hallway_id=$1 AND room=ANY($2)",
        )
        .bind(id)
        .bind(&request.allowed_rooms)
        .fetch_one(&mut *tx)
        .await?;
        if allowed as usize != request.allowed_rooms.len() {
            return Err(refusal(
                "room_not_allowed",
                "Knock policy may only name rooms allowed in this hallway",
            ));
        }
    }

    sqlx::query(
        "INSERT INTO hallway_room_state(hallway_id,room)
         VALUES($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(&request.room)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "SELECT notification_revision FROM hallway_room_state
         WHERE hallway_id=$1 AND room=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(&request.room)
    .fetch_one(&mut *tx)
    .await?;
    let revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM hallway_knock_policies WHERE hallway_id=$1 AND room=$2",
    )
    .bind(id)
    .bind(&request.room)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE hallway_knock_policies SET superseded_at=NOW()
         WHERE hallway_id=$1 AND room=$2 AND superseded_at IS NULL",
    )
    .bind(id)
    .bind(&request.room)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query(
        "INSERT INTO hallway_knock_policies(
            hallway_id,room,spirit,session_id,idempotency_key,request_digest,
            mode,allowed_rooms,max_turns,revision
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING mode,allowed_rooms,max_turns,revision",
    )
    .bind(id)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .bind(&request_digest)
    .bind(request.mode.as_str())
    .bind(&request.allowed_rooms)
    .bind(i16::from(request.max_turns))
    .bind(revision)
    .fetch_one(&mut *tx)
    .await?;
    let receipt = policy_receipt_from_row(&row, request.hallway, request.room, false)?;
    tx.commit().await?;
    Ok(receipt)
}

fn knock_pointer_from_row(row: &sqlx::postgres::PgRow) -> Result<HallwayKnockPointer, AppError> {
    let turn_index: i16 = row.try_get("turn_index")?;
    let max_turns: i16 = row.try_get("max_turns")?;
    Ok(HallwayKnockPointer {
        knock_id: row.try_get("knock_id")?,
        hallway: row.try_get("hallway_key")?,
        message_id: row.try_get("message_id")?,
        sequence: row.try_get("sequence")?,
        thread: row.try_get("thread_key")?,
        from_room: row.try_get("from_room")?,
        from_spirit: row.try_get("from_spirit")?,
        recipient_room: row.try_get("recipient_room")?,
        parent_knock_id: row.try_get("parent_knock_id")?,
        root_knock_id: row.try_get("root_knock_id")?,
        turn_index: turn_index as u8,
        max_turns: max_turns as u8,
        status: row.try_get("status")?,
        expires_at: row.try_get("expires_at_text")?,
    })
}

async fn knock_row_by_id(
    tx: &mut Transaction<'_, Postgres>,
    knock_id: &str,
) -> Result<Option<sqlx::postgres::PgRow>, AppError> {
    Ok(sqlx::query(
        "SELECT k.knock_id::text AS knock_id,c.hallway_key,k.message_id,m.sequence,
                COALESCE(t.thread_key,'') AS thread_key,k.from_room,k.from_spirit,
                k.recipient_room,k.parent_knock_id::text AS parent_knock_id,
                k.root_knock_id::text AS root_knock_id,k.turn_index,k.max_turns,k.status,
                k.expires_at::text AS expires_at_text
         FROM hallway_knocks k
         JOIN hallway_channels c ON c.id=k.hallway_id
         JOIN hallway_messages m ON m.id=k.message_id
         LEFT JOIN hallway_threads t ON t.id=m.thread_id
         WHERE k.knock_id=$1::uuid",
    )
    .bind(knock_id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn existing_knock_row(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    room: &str,
    session: &str,
    idempotency_key: &str,
) -> Result<Option<sqlx::postgres::PgRow>, AppError> {
    Ok(sqlx::query(
        "SELECT k.request_digest,k.knock_id::text AS knock_id,c.hallway_key,
                k.message_id,m.sequence,COALESCE(t.thread_key,'') AS thread_key,
                k.from_room,k.from_spirit,k.recipient_room,
                k.parent_knock_id::text AS parent_knock_id,
                k.root_knock_id::text AS root_knock_id,k.turn_index,k.max_turns,k.status,
                k.expires_at::text AS expires_at_text
         FROM hallway_knocks k
         JOIN hallway_channels c ON c.id=k.hallway_id
         JOIN hallway_messages m ON m.id=k.message_id
         LEFT JOIN hallway_threads t ON t.id=m.thread_id
         WHERE k.hallway_id=$1 AND k.from_room=$2
           AND k.request_session=$3 AND k.idempotency_key=$4",
    )
    .bind(hallway_id)
    .bind(room)
    .bind(session)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn hallway_knock(
    pool: &PgPool,
    request: HallwayKnockRequest,
) -> Result<HallwayKnockReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let parent_knock_id = request
        .parent_knock_id
        .as_deref()
        .map(|value| {
            Uuid::parse_str(value)
                .map(|uuid| uuid.to_string())
                .map_err(|_| refusal("malformed_uuid", "parentKnockId must be a UUID"))
        })
        .transpose()?;
    let request_digest = digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
        &request.message_id.to_string(),
        &request.recipient_room,
        parent_knock_id.as_deref().unwrap_or(""),
        &request.max_turns.to_string(),
    ]);
    let mut tx = pool.begin().await?;
    let id = hallway_id(&mut tx, &request.hallway).await?;
    ensure_presence(
        &mut tx,
        id,
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;

    if let Some(row) = existing_knock_row(
        &mut tx,
        id,
        &request.room,
        &request.session,
        &request.idempotency_key,
    )
    .await?
    {
        if row.try_get::<String, _>("request_digest")? != request_digest {
            return Err(refusal(
                "idempotency_reuse",
                "idempotency key was reused with a different Hallway Knock request",
            ));
        }
        let knock = knock_pointer_from_row(&row)?;
        tx.commit().await?;
        return Ok(HallwayKnockReceipt {
            ok: true,
            duplicate: true,
            knock,
        });
    }

    let message = sqlx::query(
        "SELECT m.room,m.spirit,m.session_id,m.reply_to,m.to_rooms,m.thread_id,
                COALESCE(t.thread_key,'') AS thread_key
         FROM hallway_messages m
         LEFT JOIN hallway_threads t ON t.id=m.thread_id
         WHERE m.hallway_id=$1 AND m.id=$2
         FOR UPDATE OF m",
    )
    .bind(id)
    .bind(request.message_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| refusal("message_not_found", "Hallway message does not exist"))?;
    if message.try_get::<String, _>("room")? != request.room
        || message.try_get::<String, _>("spirit")? != request.spirit
        || message.try_get::<String, _>("session_id")? != request.session
    {
        return Err(refusal(
            "knock_message_mismatch",
            "Knock message was not authored by this room, spirit, and session",
        ));
    }
    let to_rooms: Vec<String> = message.try_get("to_rooms")?;
    if !to_rooms.iter().any(|room| room == &request.recipient_room) {
        return Err(refusal(
            "knock_message_mismatch",
            "Knock recipient is not a target of the referenced message",
        ));
    }
    let already_requested: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM hallway_knocks WHERE message_id=$1 AND recipient_room=$2
         )",
    )
    .bind(request.message_id)
    .bind(&request.recipient_room)
    .fetch_one(&mut *tx)
    .await?;
    if already_requested {
        return Err(refusal(
            "knock_already_requested",
            "referenced Hallway message already requested a Knock for this recipient",
        ));
    }
    let recipient_allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM hallway_allowed_rooms WHERE hallway_id=$1 AND room=$2
         )",
    )
    .bind(id)
    .bind(&request.recipient_room)
    .fetch_one(&mut *tx)
    .await?;
    if !recipient_allowed {
        return Err(refusal(
            "room_not_allowed",
            "Knock recipient is not allowed in this hallway",
        ));
    }
    let policy = sqlx::query(
        "SELECT mode,allowed_rooms,max_turns
         FROM hallway_knock_policies
         WHERE hallway_id=$1 AND room=$2 AND superseded_at IS NULL
         FOR SHARE",
    )
    .bind(id)
    .bind(&request.recipient_room)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(policy) = policy else {
        return Err(refusal(
            "knock_policy_denied",
            "recipient room has not enabled Hallway Knocks from this room",
        ));
    };
    let allowed_rooms: Vec<String> = policy.try_get("allowed_rooms")?;
    let policy_max: i16 = policy.try_get("max_turns")?;
    if policy.try_get::<String, _>("mode")? != "allow_list"
        || !allowed_rooms.iter().any(|room| room == &request.room)
        || i16::from(request.max_turns) > policy_max
    {
        return Err(refusal(
            "knock_policy_denied",
            "recipient room policy does not allow this Hallway Knock",
        ));
    }

    let knock_id = Uuid::new_v4().to_string();
    let (root_knock_id, turn_index, max_turns, expires_at) = if let Some(parent_knock_id) =
        parent_knock_id.as_deref()
    {
        let parent = sqlx::query(
            "SELECT k.from_room,k.recipient_room,k.message_id,
                        k.root_knock_id::text AS root_knock_id,k.turn_index,k.max_turns,
                        k.status,k.expires_at,pm.thread_id
                 FROM hallway_knocks k
                 JOIN hallway_messages pm ON pm.id=k.message_id
                 WHERE k.knock_id=$1::uuid AND k.hallway_id=$2
                 FOR SHARE OF k",
        )
        .bind(parent_knock_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            refusal(
                "knock_parent_mismatch",
                "parent Knock does not belong to this hallway",
            )
        })?;
        let parent_status: String = parent.try_get("status")?;
        if parent_status != "started" && parent_status != "completed" {
            return Err(refusal(
                "knock_state_conflict",
                "child Knock requires a parent turn that started",
            ));
        }
        let parent_max: i16 = parent.try_get("max_turns")?;
        let parent_turn: i16 = parent.try_get("turn_index")?;
        let next_turn = parent_turn + 1;
        if next_turn > parent_max {
            return Err(refusal(
                "knock_exchange_exhausted",
                "Hallway Knock exchange has reached its maximum turns",
            ));
        }
        if request.max_turns != parent_max as u8 {
            return Err(refusal(
                "knock_parent_mismatch",
                "child Knock must inherit maxTurns from its parent",
            ));
        }
        if parent.try_get::<String, _>("recipient_room")? != request.room
            || parent.try_get::<String, _>("from_room")? != request.recipient_room
            || message.try_get::<Option<i64>, _>("reply_to")? != Some(parent.try_get("message_id")?)
            || message.try_get::<Option<i64>, _>("thread_id")?
                != parent.try_get::<Option<i64>, _>("thread_id")?
        {
            return Err(refusal(
                "knock_parent_mismatch",
                "child Knock must reverse rooms and directly reply in the parent thread",
            ));
        }
        let expires_at: DateTime<Utc> = parent.try_get("expires_at")?;
        if expires_at <= Utc::now() {
            return Err(refusal(
                "knock_state_conflict",
                "parent Knock exchange has expired",
            ));
        }
        (
            parent.try_get("root_knock_id")?,
            next_turn,
            parent_max,
            expires_at,
        )
    } else {
        (
            knock_id.clone(),
            1_i16,
            i16::from(request.max_turns),
            Utc::now() + Duration::minutes(15),
        )
    };

    sqlx::query(
        "INSERT INTO hallway_knocks(
            knock_id,hallway_id,message_id,from_room,from_spirit,request_session,
            idempotency_key,request_digest,recipient_room,parent_knock_id,
            root_knock_id,turn_index,max_turns,expires_at
         ) VALUES(
            $1::uuid,$2,$3,$4,$5,$6,$7,$8,$9,$10::uuid,$11::uuid,$12,$13,$14
         )",
    )
    .bind(&knock_id)
    .bind(id)
    .bind(request.message_id)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .bind(&request_digest)
    .bind(&request.recipient_room)
    .bind(parent_knock_id.as_deref())
    .bind(&root_knock_id)
    .bind(turn_index)
    .bind(max_turns)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    let bumped = sqlx::query(
        "UPDATE hallway_room_state
         SET notification_revision=notification_revision+1,updated_at=NOW()
         WHERE hallway_id=$1 AND room=$2",
    )
    .bind(id)
    .bind(&request.recipient_room)
    .execute(&mut *tx)
    .await?;
    if bumped.rows_affected() != 1 {
        return Err(refusal(
            "knock_state_conflict",
            "recipient Hallway notification state is missing",
        ));
    }
    let row = knock_row_by_id(&mut tx, &knock_id).await?.ok_or_else(|| {
        refusal(
            "knock_state_conflict",
            "created Knock could not be read back",
        )
    })?;
    let knock = knock_pointer_from_row(&row)?;
    tx.commit().await?;
    Ok(HallwayKnockReceipt {
        ok: true,
        duplicate: false,
        knock,
    })
}

pub async fn hallway_knock_claim(
    pool: &PgPool,
    request: HallwayKnockClaimRequest,
) -> Result<HallwayKnockClaimReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE hallway_knocks
         SET status='failed',
             settled_reason='recipient turn expired before completion',
             settled_at=NOW()
         WHERE recipient_room=$1
           AND status IN ('claimed','started')
           AND expires_at<=NOW()",
    )
    .bind(&request.room)
    .execute(&mut *tx)
    .await?;
    let candidate = sqlx::query(
        "SELECT k.knock_id::text,k.hallway_id,h.hallway_key AS hallway
         FROM hallway_knocks k
         JOIN hallway_channels h ON h.id=k.hallway_id
         JOIN hallway_knock_policies p
           ON p.hallway_id=k.hallway_id
          AND p.room=k.recipient_room
          AND p.superseded_at IS NULL
         WHERE k.recipient_room=$1
           AND p.mode='allow_list'
           AND k.from_room=ANY(p.allowed_rooms)
           AND k.expires_at>NOW()
           AND (
                k.status='pending'
                OR (k.status='claimed' AND k.lease_expires_at<=NOW())
           )
         ORDER BY k.created_at,k.knock_id
         FOR UPDATE OF k SKIP LOCKED
         LIMIT 1",
    )
    .bind(&request.room)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(candidate) = candidate else {
        tx.commit().await?;
        return Ok(HallwayKnockClaimReceipt {
            ok: true,
            knock: None,
        });
    };
    let knock_id: String = candidate.try_get("knock_id")?;
    let hallway: String = candidate.try_get("hallway")?;
    let hallway_id: i64 = candidate.try_get("hallway_id")?;
    ensure_presence(
        &mut tx,
        hallway_id,
        &hallway,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;
    sqlx::query(
        "UPDATE hallway_knocks
         SET status='claimed',claimed_by_room=$2,claimed_by_spirit=$3,
             claimed_by_session=$4,lease_expires_at=NOW()+INTERVAL '30 seconds'
         WHERE knock_id=$1::uuid",
    )
    .bind(&knock_id)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .execute(&mut *tx)
    .await?;
    let row = knock_row_by_id(&mut tx, &knock_id).await?.ok_or_else(|| {
        refusal(
            "knock_state_conflict",
            "claimed Knock could not be read back",
        )
    })?;
    let knock = knock_pointer_from_row(&row)?;
    tx.commit().await?;
    Ok(HallwayKnockClaimReceipt {
        ok: true,
        knock: Some(knock),
    })
}

pub async fn hallway_knock_settle(
    pool: &PgPool,
    request: HallwayKnockSettleRequest,
) -> Result<HallwayKnockSettleReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let knock_id = Uuid::parse_str(&request.knock_id)
        .map(|uuid| uuid.to_string())
        .map_err(|_| refusal("malformed_uuid", "knockId must be a UUID"))?;
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT status,claimed_by_room,claimed_by_spirit,claimed_by_session,
                lease_expires_at,started_reason,settled_reason
         FROM hallway_knocks WHERE knock_id=$1::uuid FOR UPDATE",
    )
    .bind(&knock_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| refusal("knock_not_found", "Hallway Knock does not exist"))?;
    if row
        .try_get::<Option<String>, _>("claimed_by_room")?
        .as_deref()
        != Some(request.room.as_str())
        || row
            .try_get::<Option<String>, _>("claimed_by_spirit")?
            .as_deref()
            != Some(request.spirit.as_str())
        || row
            .try_get::<Option<String>, _>("claimed_by_session")?
            .as_deref()
            != Some(request.session.as_str())
    {
        return Err(refusal(
            "knock_state_conflict",
            "Knock is not leased to this room, spirit, and session",
        ));
    }
    let status: String = row.try_get("status")?;
    let requested_status = request.outcome.as_str();
    let stored_reason = match request.outcome {
        HallwayKnockOutcome::Started => row.try_get::<Option<String>, _>("started_reason")?,
        HallwayKnockOutcome::Completed | HallwayKnockOutcome::Failed => {
            row.try_get::<Option<String>, _>("settled_reason")?
        }
    };
    if status == requested_status && stored_reason == request.reason {
        tx.commit().await?;
        return Ok(HallwayKnockSettleReceipt {
            ok: true,
            duplicate: true,
            knock_id,
            status,
        });
    }

    match request.outcome {
        HallwayKnockOutcome::Started => {
            let lease_expires_at: DateTime<Utc> = row.try_get("lease_expires_at")?;
            if status != "claimed" || lease_expires_at <= Utc::now() {
                return Err(refusal(
                    "knock_state_conflict",
                    "Knock cannot start outside its active claim lease",
                ));
            }
            sqlx::query(
                "UPDATE hallway_knocks
                 SET status='started',started_at=NOW(),started_reason=$2
                 WHERE knock_id=$1::uuid",
            )
            .bind(&knock_id)
            .bind(&request.reason)
            .execute(&mut *tx)
            .await?;
        }
        HallwayKnockOutcome::Completed => {
            if status != "started" {
                return Err(refusal(
                    "knock_state_conflict",
                    "Knock can complete only after its turn has started",
                ));
            }
            sqlx::query(
                "UPDATE hallway_knocks
                 SET status='completed',settled_at=NOW(),settled_reason=$2
                 WHERE knock_id=$1::uuid",
            )
            .bind(&knock_id)
            .bind(&request.reason)
            .execute(&mut *tx)
            .await?;
        }
        HallwayKnockOutcome::Failed => {
            let lease_expires_at: DateTime<Utc> = row.try_get("lease_expires_at")?;
            if status != "started" && !(status == "claimed" && lease_expires_at > Utc::now()) {
                return Err(refusal(
                    "knock_state_conflict",
                    "Knock cannot fail outside its active claim or started turn",
                ));
            }
            sqlx::query(
                "UPDATE hallway_knocks
                 SET status='failed',settled_at=NOW(),settled_reason=$2
                 WHERE knock_id=$1::uuid",
            )
            .bind(&knock_id)
            .bind(&request.reason)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(HallwayKnockSettleReceipt {
        ok: true,
        duplicate: false,
        knock_id,
        status: requested_status.into(),
    })
}
