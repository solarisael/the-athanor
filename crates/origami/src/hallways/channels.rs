//! Cost and state (coding#195): every function here opens its own
//! transaction unless it takes one; [`lookup_id`] and [`ensure_presence`]
//! take a caller's transaction and may take row locks inside it.

use super::errors::{HallwayError, invalid, refusal};
use crate::sea::idempotency_digest;
use house_core::hallway::{
    HallwayCreateDisposition, HallwayCreateRequest, HallwayJoinDisposition, HallwayJoinRequest,
    HallwayPresenceReceipt, HallwayReceipt,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

pub(crate) async fn lookup_id(
    tx: &mut Transaction<'_, Postgres>,
    hallway: &str,
) -> Result<i64, HallwayError> {
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
pub async fn ensure_presence(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    hallway_key: &str,
    room: &str,
    spirit: &str,
    session: &str,
) -> Result<i64, HallwayError> {
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
        let join_digest = idempotency_digest(&[hallway_key, room, spirit, session]);
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

/// The creating room joins in the same transaction; a repeat of the exact
/// same create command is a duplicate success, a different one is refused.
pub async fn create(
    pool: &PgPool,
    request: HallwayCreateRequest,
) -> Result<HallwayReceipt, HallwayError> {
    request.validate().map_err(invalid)?;
    let mut allowed_rooms = request.allowed_rooms.clone();
    allowed_rooms.sort();
    let rooms_digest = allowed_rooms.join("\n");
    let create_digest = idempotency_digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
        &rooms_digest,
    ]);
    let join_digest = idempotency_digest(&[
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

pub async fn join(
    pool: &PgPool,
    request: HallwayJoinRequest,
) -> Result<HallwayPresenceReceipt, HallwayError> {
    request.validate().map_err(invalid)?;
    let join_digest = idempotency_digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
    ]);
    let mut tx = pool.begin().await?;
    let id = lookup_id(&mut tx, &request.hallway).await?;
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
