use crate::{AppError, Config};
use house_core::hallway::{
    HallwayCreateDisposition, HallwayCreateRequest, HallwayInboxEntry, HallwayInboxReceipt,
    HallwayInboxRequest, HallwayJoinDisposition, HallwayJoinRequest, HallwayMessage,
    HallwayPostDisposition, HallwayPostReceipt, HallwayPostRequest, HallwayPresenceReceipt,
    HallwayReadReceipt, HallwayReadRequest, HallwayReceipt,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};

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
    request: HallwayPostRequest,
) -> Result<HallwayPostReceipt, AppError> {
    request.validate().map_err(invalid)?;
    let body_digest = digest(&[
        &request.body,
        &request
            .reply_to
            .map(|id| id.to_string())
            .unwrap_or_default(),
        &request.spirit,
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
                m.created_at::text AS created_at_text,m.body_digest,m.to_rooms,
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
        if row.try_get::<String, _>("body_digest")? != body_digest {
            return Err(refusal(
                "idempotency_reuse",
                "idempotency key was reused with different message content",
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
        let row = sqlx::query(
            "SELECT thread_id FROM hallway_messages WHERE hallway_id=$1 AND id=$2",
        )
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
    let after = request.after.unwrap_or(previous_cursor);
    let rows = sqlx::query(
        "SELECT m.id,m.sequence,m.room,m.spirit,m.session_id,m.body,m.reply_to,
                m.created_at::text AS created_at_text,m.to_rooms,
                COALESCE(t.thread_key,'') AS thread_key
         FROM hallway_messages m
         LEFT JOIN hallway_threads t ON t.id=m.thread_id
         WHERE m.hallway_id=$1 AND m.id>$2
         ORDER BY m.id ASC LIMIT $3",
    )
    .bind(id)
    .bind(after)
    .bind(i64::from(request.limit) + 1)
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

        // Room-stable read state advances only on contiguous coverage: a
        // jump-ahead read leaves unseen history unread.
        let mut state_changed = acked_mentions > 0;
        if let (Some(first), Some(last)) = (messages.first(), messages.last())
            && first.sequence <= room_read_sequence + 1
            && last.sequence > room_read_sequence
        {
            room_read_sequence = last.sequence;
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
                COALESCE(s.read_sequence,0) AS read_sequence,
                COALESCE(s.notification_revision,0) AS notification_revision,
                (SELECT COUNT(*) FROM hallway_notifications n
                  WHERE n.hallway_id=c.id AND n.recipient_room=$1 AND n.read_at IS NULL
                ) AS mentions,
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
            let read_sequence: i64 = row.try_get("read_sequence")?;
            Ok(HallwayInboxEntry {
                hallway: row.try_get("hallway_key")?,
                unread: (latest_sequence - read_sequence).max(0),
                mentions: row.try_get("mentions")?,
                notification_revision: row.try_get("notification_revision")?,
                latest_sequence,
                latest_room: row.try_get("latest_room")?,
                latest_spirit: row.try_get("latest_spirit")?,
                latest_excerpt: row.try_get("latest_excerpt")?,
                latest_created_at: row.try_get("latest_created_at")?,
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
