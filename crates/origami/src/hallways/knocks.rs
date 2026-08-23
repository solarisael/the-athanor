
use super::channels::{ensure_presence, lookup_id};
use super::errors::{HallwayError, invalid, refusal};
use crate::sea::idempotency_digest;
use chrono::{DateTime, Duration, Utc};
use hearth::hallway::{
    HallwayKnockClaimReceipt, HallwayKnockClaimRequest, HallwayKnockOutcome, HallwayKnockPointer,
    HallwayKnockPolicyMode, HallwayKnockPolicyReceipt, HallwayKnockPolicyRequest,
    HallwayKnockReceipt, HallwayKnockRequest, HallwayKnockSettleReceipt, HallwayKnockSettleRequest,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

fn policy_mode_from_db(value: &str) -> Result<HallwayKnockPolicyMode, HallwayError> {
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
) -> Result<HallwayKnockPolicyReceipt, HallwayError> {
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

fn knock_pointer_from_row(row: &sqlx::postgres::PgRow) -> Result<HallwayKnockPointer, HallwayError> {
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
) -> Result<Option<sqlx::postgres::PgRow>, HallwayError> {
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
) -> Result<Option<sqlx::postgres::PgRow>, HallwayError> {
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

pub async fn policy(
    pool: &PgPool,
    mut request: HallwayKnockPolicyRequest,
) -> Result<HallwayKnockPolicyReceipt, HallwayError> {
    request.validate().map_err(invalid)?;
    request.allowed_rooms.sort_unstable();
    let allowed_digest = request.allowed_rooms.join("\0");
    let request_digest = idempotency_digest(&[
        &request.hallway,
        &request.room,
        &request.spirit,
        &request.session,
        request.mode.as_str(),
        &allowed_digest,
        &request.max_turns.to_string(),
    ]);
    let mut tx = pool.begin().await?;
    let id = lookup_id(&mut tx, &request.hallway).await?;
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

pub async fn knock(
    pool: &PgPool,
    request: HallwayKnockRequest,
) -> Result<HallwayKnockReceipt, HallwayError> {
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
    let request_digest = idempotency_digest(&[
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

pub async fn claim(
    pool: &PgPool,
    request: HallwayKnockClaimRequest,
) -> Result<HallwayKnockClaimReceipt, HallwayError> {
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

pub async fn settle(
    pool: &PgPool,
    request: HallwayKnockSettleRequest,
) -> Result<HallwayKnockSettleReceipt, HallwayError> {
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
