//! The Knock ledger: a room sets its policy, a presence asks a peer room for
//! one bounded turn, the recipient claims that turn and settles it.

use super::channels::{ensure_presence, lookup_id};
use super::errors::{HallwayError, invalid, refusal};
use super::rows;
use crate::sea::idempotency_digest;
use chrono::{DateTime, Duration, Utc};
use hearth::hallway::{
    HallwayKnockClaimReceipt, HallwayKnockClaimRequest, HallwayKnockOutcome, HallwayKnockPointer,
    HallwayKnockPolicyMode, HallwayKnockPolicyReceipt, HallwayKnockPolicyRequest,
    HallwayKnockReceipt, HallwayKnockRequest, HallwayKnockSettleReceipt, HallwayKnockSettleRequest,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

/// How long a fresh Knock waits for an answer before it expires unanswered.
///
/// A Knock is a request for one bounded turn, so its lifetime is a courtesy
/// window for a peer room that may not be awake, not a lease on work.
pub const KNOCK_REQUEST_LIFETIME: Duration = Duration::minutes(15);

// The lease seconds live in a macro because the claim statement splices the
// number into its `INTERVAL` literal at compile time; this keeps one
// declaration serving both the constant below and that SQL.
macro_rules! knock_claim_lease_seconds {
    () => {
        30
    };
}

/// How long a claimed Knock stays claimed before another presence may take the
/// turn.
///
/// Sibling of `cranes::outbox::LEASE_SECONDS`, and the same 30 seconds for the
/// same reason: both are one worker's grip on a claimed row, sized so a crashed
/// claimer frees the item within a human's patience rather than holding it
/// until a restart. They stay two constants because they are two ledgers -- a
/// Hallway turn and an outbox publish may want different patience -- but move
/// one and you should look at the other.
pub const KNOCK_CLAIM_LEASE_SECONDS: i64 = knock_claim_lease_seconds!();

/// The unique constraint 0021_hallway_knock.sql:64 declares on
/// `(message_id, recipient_room)`: one message asks one recipient once.
const ONE_KNOCK_PER_MESSAGE_AND_RECIPIENT: &str = "hallway_knocks_message_id_recipient_room_key";

/// Where a Knock sits in its exchange.
///
/// A root Knock opens an exchange with its own id, turn 1, the caller's
/// ceiling and a fresh lifetime. A child inherits all four from its parent and
/// takes the next turn.
pub(super) struct ChainPosition {
    pub root_knock_id: String,
    pub turn_index: i16,
    pub max_turns: i16,
    pub expires_at: DateTime<Utc>,
}

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
    row: &PgRow,
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

fn knock_pointer_from_row(row: &PgRow) -> Result<HallwayKnockPointer, HallwayError> {
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

async fn knock_pointer(
    tx: &mut Transaction<'_, Postgres>,
    knock_id: &str,
    missing: &'static str,
) -> Result<HallwayKnockPointer, HallwayError> {
    let row = sqlx::query(
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
    .await?
    .ok_or_else(|| refusal("knock_state_conflict", missing))?;
    knock_pointer_from_row(&row)
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

    // The room's state row is the serialization point for its policy history:
    // the upsert takes the row lock whether it inserts or finds the row, so
    // two writers cannot both compute the same next revision.
    sqlx::query(
        "INSERT INTO hallway_room_state(hallway_id,room) VALUES($1,$2)
         ON CONFLICT (hallway_id,room) DO UPDATE SET room=EXCLUDED.room",
    )
    .bind(id)
    .bind(&request.room)
    .execute(&mut *tx)
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
         )
         SELECT hallway_id,room,spirit,session_id,idempotency_key,request_digest,
                mode,allowed_rooms,max_turns,
                COALESCE((SELECT MAX(revision) FROM hallway_knock_policies
                          WHERE hallway_id=$2 AND room=$3),0)+1
         FROM jsonb_populate_record(NULL::hallway_knock_policies,$1)
         RETURNING mode,allowed_rooms,max_turns,revision",
    )
    .bind(rows::knock_policy(id, &request, &request_digest))
    .bind(id)
    .bind(&request.room)
    .fetch_one(&mut *tx)
    .await?;
    let receipt = policy_receipt_from_row(&row, request.hallway, request.room, false)?;
    tx.commit().await?;
    Ok(receipt)
}

/// May this presence knock on that room about this message? Returns the
/// locked message row, which the chain rules read next.
async fn admit(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    request: &HallwayKnockRequest,
) -> Result<PgRow, HallwayError> {
    let message = sqlx::query(
        "SELECT m.room,m.spirit,m.session_id,m.reply_to,m.to_rooms,m.thread_id
         FROM hallway_messages m
         WHERE m.hallway_id=$1 AND m.id=$2
         FOR UPDATE",
    )
    .bind(hallway_id)
    .bind(request.message_id)
    .fetch_optional(&mut **tx)
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

    // One read for the recipient's standing: allowed in this hallway, and its
    // current policy held FOR SHARE so a policy write waits for this Knock.
    let standing = sqlx::query(
        "SELECT a.room IS NOT NULL AS recipient_allowed,p.mode,p.allowed_rooms,p.max_turns
         FROM hallway_channels c
         LEFT JOIN hallway_allowed_rooms a ON a.hallway_id=c.id AND a.room=$2
         LEFT JOIN (
             SELECT mode,allowed_rooms,max_turns FROM hallway_knock_policies
             WHERE hallway_id=$1 AND room=$2 AND superseded_at IS NULL
             FOR SHARE
         ) p ON TRUE
         WHERE c.id=$1",
    )
    .bind(hallway_id)
    .bind(&request.recipient_room)
    .fetch_one(&mut **tx)
    .await?;
    if !standing.try_get::<bool, _>("recipient_allowed")? {
        return Err(refusal(
            "room_not_allowed",
            "Knock recipient is not allowed in this hallway",
        ));
    }
    let Some(mode) = standing.try_get::<Option<String>, _>("mode")? else {
        return Err(refusal(
            "knock_policy_denied",
            "recipient room has not enabled Hallway Knocks from this room",
        ));
    };
    let allowed_rooms: Vec<String> = standing.try_get("allowed_rooms")?;
    let policy_max: i16 = standing.try_get("max_turns")?;
    if mode != "allow_list"
        || !allowed_rooms.iter().any(|room| room == &request.room)
        || i16::from(request.max_turns) > policy_max
    {
        return Err(refusal(
            "knock_policy_denied",
            "recipient room policy does not allow this Hallway Knock",
        ));
    }
    Ok(message)
}

/// A child Knock continues its parent's exchange: it reverses the rooms,
/// replies directly in the parent's thread, inherits the ceiling, takes the
/// next turn, and runs on the parent's clock.
async fn chain_position(
    tx: &mut Transaction<'_, Postgres>,
    hallway_id: i64,
    request: &HallwayKnockRequest,
    message: &PgRow,
    knock_id: &str,
    parent_knock_id: Option<&str>,
) -> Result<ChainPosition, HallwayError> {
    let Some(parent_knock_id) = parent_knock_id else {
        return Ok(ChainPosition {
            root_knock_id: knock_id.to_string(),
            turn_index: 1,
            max_turns: i16::from(request.max_turns),
            expires_at: Utc::now() + KNOCK_REQUEST_LIFETIME,
        });
    };
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
    .bind(hallway_id)
    .fetch_optional(&mut **tx)
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
    let max_turns: i16 = parent.try_get("max_turns")?;
    let turn_index = parent.try_get::<i16, _>("turn_index")? + 1;
    if turn_index > max_turns {
        return Err(refusal(
            "knock_exchange_exhausted",
            "Hallway Knock exchange has reached its maximum turns",
        ));
    }
    if i16::from(request.max_turns) != max_turns {
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
    Ok(ChainPosition {
        root_knock_id: parent.try_get("root_knock_id")?,
        turn_index,
        max_turns,
        expires_at,
    })
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

    if let Some(row) = sqlx::query(
        "SELECT knock_id::text AS knock_id,request_digest FROM hallway_knocks
         WHERE hallway_id=$1 AND from_room=$2 AND request_session=$3 AND idempotency_key=$4",
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
                "idempotency key was reused with a different Hallway Knock request",
            ));
        }
        let knock_id: String = row.try_get("knock_id")?;
        let knock =
            knock_pointer(&mut tx, &knock_id, "recorded Knock could not be read back").await?;
        tx.commit().await?;
        return Ok(HallwayKnockReceipt {
            ok: true,
            duplicate: true,
            knock,
        });
    }

    let message = admit(&mut tx, id, &request).await?;
    let knock_id = Uuid::new_v4().to_string();
    let position = chain_position(
        &mut tx,
        id,
        &request,
        &message,
        &knock_id,
        parent_knock_id.as_deref(),
    )
    .await?;
    sqlx::query(
        "INSERT INTO hallway_knocks(
            knock_id,hallway_id,message_id,from_room,from_spirit,request_session,
            idempotency_key,request_digest,recipient_room,parent_knock_id,
            root_knock_id,turn_index,max_turns,expires_at
         )
         SELECT knock_id,hallway_id,message_id,from_room,from_spirit,request_session,
                idempotency_key,request_digest,recipient_room,parent_knock_id,
                root_knock_id,turn_index,max_turns,expires_at
         FROM jsonb_populate_record(NULL::hallway_knocks,$1)",
    )
    .bind(rows::knock(
        id,
        &knock_id,
        &request,
        &request_digest,
        parent_knock_id.as_deref(),
        &position,
    ))
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database)
            if database.constraint() == Some(ONE_KNOCK_PER_MESSAGE_AND_RECIPIENT) =>
        {
            refusal(
                "knock_already_requested",
                "referenced Hallway message already requested a Knock for this recipient",
            )
        }
        _ => error.into(),
    })?;
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
    let knock = knock_pointer(&mut tx, &knock_id, "created Knock could not be read back").await?;
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
        // The lease window is spliced at compile time from the one constant
        // rather than bound: NOW() must stay the server's clock the way it is
        // today, and a cast on a placeholder would move the lease's encoding
        // into the driver where nothing here can prove it.
        concat!(
            "UPDATE hallway_knocks
         SET status='claimed',claimed_by_room=$2,claimed_by_spirit=$3,
             claimed_by_session=$4,lease_expires_at=NOW()+INTERVAL '",
            knock_claim_lease_seconds!(),
            " seconds'
         WHERE knock_id=$1::uuid"
        ),
    )
    .bind(&knock_id)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .execute(&mut *tx)
    .await?;
    let knock = knock_pointer(&mut tx, &knock_id, "claimed Knock could not be read back").await?;
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

    let lease_expires_at: DateTime<Utc> = row.try_get("lease_expires_at")?;
    let lease_active = status == "claimed" && lease_expires_at > Utc::now();
    let statement = match request.outcome {
        HallwayKnockOutcome::Started => {
            if !lease_active {
                return Err(refusal(
                    "knock_state_conflict",
                    "Knock cannot start outside its active claim lease",
                ));
            }
            "UPDATE hallway_knocks
             SET status='started',started_at=NOW(),started_reason=$2
             WHERE knock_id=$1::uuid"
        }
        HallwayKnockOutcome::Completed => {
            if status != "started" {
                return Err(refusal(
                    "knock_state_conflict",
                    "Knock can complete only after its turn has started",
                ));
            }
            "UPDATE hallway_knocks
             SET status='completed',settled_at=NOW(),settled_reason=$2
             WHERE knock_id=$1::uuid"
        }
        HallwayKnockOutcome::Failed => {
            if status != "started" && !lease_active {
                return Err(refusal(
                    "knock_state_conflict",
                    "Knock cannot fail outside its active claim or started turn",
                ));
            }
            "UPDATE hallway_knocks
             SET status='failed',settled_at=NOW(),settled_reason=$2
             WHERE knock_id=$1::uuid"
        }
    };
    sqlx::query(statement)
        .bind(&knock_id)
        .bind(&request.reason)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(HallwayKnockSettleReceipt {
        ok: true,
        duplicate: false,
        knock_id,
        status: requested_status.into(),
    })
}
