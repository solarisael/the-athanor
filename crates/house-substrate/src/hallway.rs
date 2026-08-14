use crate::AppError;
use house_core::hallway::{
    HallwayCreateRequest, HallwayJoinRequest, HallwayMessage, HallwayPostReceipt,
    HallwayPostRequest, HallwayPresenceReceipt, HallwayReadReceipt, HallwayReadRequest,
    HallwayReceipt,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
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
        .ok_or_else(|| invalid(format!("hallway does not exist: {hallway}")))
}

async fn require_presence(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    room: &str,
    spirit: &str,
    session: &str,
) -> Result<i64, AppError> {
    let row = sqlx::query(
        "SELECT read_cursor,spirit FROM hallway_presences
         WHERE hallway_id=$1 AND room=$2 AND session_id=$3
         FOR UPDATE",
    )
    .bind(hallway_id)
    .bind(room)
    .bind(session)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("session is not joined to this hallway"))?;
    let stored_spirit: String = row.try_get("spirit")?;
    if stored_spirit != spirit {
        return Err(invalid("session is bound to a different spirit"));
    }
    Ok(row.try_get("read_cursor")?)
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
                return Err(invalid(
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
        created,
        duplicate: !created,
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
        return Err(invalid("room is not allowed in this hallway"));
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
        let matches = row.try_get::<String, _>("spirit")? == request.spirit
            && row.try_get::<String, _>("join_idempotency_key")? == request.idempotency_key
            && row.try_get::<String, _>("join_digest")? == join_digest;
        if !matches {
            return Err(invalid(
                "session is already joined with a different identity or command",
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
        joined,
        duplicate: !joined,
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
    })
}

pub async fn hallway_post(
    pool: &PgPool,
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
    require_presence(
        &mut tx,
        id,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;

    let existing = sqlx::query(
        "SELECT id,sequence,room,spirit,session_id,body,reply_to,created_at::text AS created_at_text,
                body_digest
         FROM hallway_messages
         WHERE hallway_id=$1 AND room=$2 AND session_id=$3 AND idempotency_key=$4",
    )
    .bind(id)
    .bind(&request.room)
    .bind(&request.session)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = existing {
        if row.try_get::<String, _>("body_digest")? != body_digest {
            return Err(invalid(
                "idempotency key was reused with different message content",
            ));
        }
        let message = message_from_row(&row, &request.hallway)?;
        tx.commit().await?;
        return Ok(HallwayPostReceipt {
            ok: true,
            duplicate: true,
            message,
        });
    }

    if let Some(reply_to) = request.reply_to {
        let reply_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM hallway_messages WHERE hallway_id=$1 AND id=$2)",
        )
        .bind(id)
        .bind(reply_to)
        .fetch_one(&mut *tx)
        .await?;
        if !reply_exists {
            return Err(invalid(
                "replyTo does not identify a message in this hallway",
            ));
        }
    }

    let sequence: i64 = sqlx::query_scalar(
        "UPDATE hallway_channels SET next_sequence=next_sequence+1
         WHERE id=$1 RETURNING next_sequence-1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let row = sqlx::query(
        "INSERT INTO hallway_messages(
            hallway_id,sequence,room,spirit,session_id,idempotency_key,body,body_digest,reply_to
         ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id,sequence,room,spirit,session_id,body,reply_to,created_at::text AS created_at_text",
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
    .fetch_one(&mut *tx)
    .await?;
    let message = message_from_row(&row, &request.hallway)?;
    tx.commit().await?;
    Ok(HallwayPostReceipt {
        ok: true,
        duplicate: false,
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
    let previous_cursor = require_presence(
        &mut tx,
        id,
        &request.room,
        &request.spirit,
        &request.session,
    )
    .await?;
    let after = request.after.unwrap_or(previous_cursor);
    let rows = sqlx::query(
        "SELECT id,sequence,room,spirit,session_id,body,reply_to,created_at::text AS created_at_text
         FROM hallway_messages WHERE hallway_id=$1 AND id>$2
         ORDER BY id ASC LIMIT $3",
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
        wake_policy: "manual".into(),
    })
}
