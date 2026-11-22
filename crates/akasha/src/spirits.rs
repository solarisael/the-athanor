//! The spirit record: one row in table `spirits` for each room of the House
//! (migration 0051). The Host serves the active rows and follows each change
//! through NOTIFY on [`SPIRITS_CHANNEL`]. Identity texts stay in `spirit_texts`.

use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

/// The NOTIFY channel. Each payload is the key of one changed room.
pub const SPIRITS_CHANNEL: &str = "athanor_spirits";

/// A row of the room in one of these tables means that the room has woken.
const WOKEN_TABLES: [&str; 6] = [
    "memories",
    "orihon",
    "orihon_units",
    "giga_candidates",
    "anamnesis",
    "threads",
];

/// Each statement moves the rows of room `$1` to room `$2`. The foreign keys
/// of the Hallway cannot change in place, so the new parent rows come first,
/// the children move, and the old parent rows go last.
const ROOM_MOVES: [&str; 16] = [
    "INSERT INTO hallway_allowed_rooms (hallway_id, room, created_at)
     SELECT hallway_id, $2, created_at FROM hallway_allowed_rooms WHERE room = $1",
    "INSERT INTO hallway_presences (hallway_id, room, spirit, session_id, join_idempotency_key,
         join_digest, read_cursor, joined_at, last_seen_at)
     SELECT hallway_id, $2, spirit, session_id, join_idempotency_key,
         join_digest, read_cursor, joined_at, last_seen_at
     FROM hallway_presences WHERE room = $1",
    "UPDATE hallway_messages SET room = $2 WHERE room = $1",
    "DELETE FROM hallway_presences WHERE room = $1",
    "UPDATE hallway_room_state SET room = $2 WHERE room = $1",
    "UPDATE hallway_knock_policies SET room = $2 WHERE room = $1",
    "UPDATE hallway_knocks SET recipient_room = $2 WHERE recipient_room = $1",
    "DELETE FROM hallway_allowed_rooms WHERE room = $1",
    "UPDATE hallway_knock_policies SET allowed_rooms = array_replace(allowed_rooms, $1, $2)
     WHERE $1 = ANY (allowed_rooms)",
    "UPDATE hallway_messages SET to_rooms = array_replace(to_rooms, $1, $2)
     WHERE $1 = ANY (to_rooms)",
    "UPDATE hallway_knocks SET from_room = $2 WHERE from_room = $1",
    "UPDATE hallway_knocks SET claimed_by_room = $2 WHERE claimed_by_room = $1",
    "UPDATE hallway_notifications SET recipient_room = $2 WHERE recipient_room = $1",
    "UPDATE hallway_channels SET created_by_room = $2 WHERE created_by_room = $1",
    "UPDATE room_settings SET room_key = $2 WHERE room_key = $1",
    "UPDATE spellbook_pages SET scope = $2 WHERE scope = $1",
];

/// One room of the House: the shape of `athanor spirits` and of the Host
/// endpoint `GET /room/<room>/spirits`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpiritRecord {
    pub room: String,
    pub spirit: String,
    pub operator: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SpiritChange {
    Summoned,
    Revived,
    Renamed,
    Retired,
    Unchanged,
}

/// What one summon, retire, or rename did, and the record after it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpiritWrite {
    pub change: SpiritChange,
    pub spirit: SpiritRecord,
}

#[derive(Debug, thiserror::Error)]
pub enum SpiritError {
    #[error("invalid spirit record: {0}")]
    Invalid(String),
    #[error("refused: {0}")]
    Refused(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl SpiritRecord {
    pub fn validate(&self) -> Result<(), SpiritError> {
        room_key(&self.room)?;
        spirit_name(&self.spirit)?;
        hearth::OperatorKey::new(self.operator.as_str()).map_err(|_| {
            SpiritError::Invalid(format!(
                "operator {:?} must be lowercase words joined by single hyphens",
                self.operator
            ))
        })?;

        Ok(())
    }
}

fn room_key(room: &str) -> Result<(), SpiritError> {
    if protocol::is_safe_room_key(room) {
        return Ok(());
    }

    Err(SpiritError::Invalid(format!(
        "room {room:?} must be lowercase words joined by single hyphens, and not house"
    )))
}

// The room-state file and the OMP adapter refuse a longer name or one with
// `|`; the record must not hold a name that its room cannot show.
fn spirit_name(spirit: &str) -> Result<(), SpiritError> {
    if spirit.is_empty()
        || spirit.trim() != spirit
        || spirit.encode_utf16().count() > 80
        || spirit.chars().any(|character| character.is_control() || character == '|')
    {
        return Err(SpiritError::Invalid(format!(
            "spirit {spirit:?} must be 1 to 80 characters, without outer space, control characters, or |"
        )));
    }

    Ok(())
}

// One lock for every write keeps each check-then-write free of races.
async fn lock(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('athanor_spirits'))")
        .execute(connection)
        .await?;

    Ok(())
}

/// The active rooms in byte order of the room key, the order of the Host list.
pub async fn active_spirits(pool: &PgPool) -> Result<Vec<SpiritRecord>, SpiritError> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        r#"SELECT room, spirit, operator FROM spirits
           WHERE retired_at IS NULL ORDER BY room COLLATE "C""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(room, spirit, operator)| SpiritRecord {
            room,
            spirit,
            operator,
        })
        .collect())
}

/// A summon or a rename whose transaction stays open until
/// [`PendingSpirit::commit`], so that the caller can prepare the room folder
/// before a Host hears the change. A drop rolls the transaction back.
pub struct PendingSpirit {
    transaction: Transaction<'static, Postgres>,
    write: SpiritWrite,
}

impl PendingSpirit {
    pub fn write(&self) -> &SpiritWrite {
        &self.write
    }

    pub async fn commit(self) -> Result<SpiritWrite, SpiritError> {
        self.transaction.commit().await?;

        Ok(self.write)
    }
}

/// Writes the record of a new room, or makes a retired room active again.
/// An active identical record changes nothing. A different spirit or operator
/// for the room refuses. An unchanged summon still notifies the channel, so
/// that a Host tries again to open a room that it could not open.
pub async fn begin_summon(
    pool: &PgPool,
    record: &SpiritRecord,
    summoned_by: &str,
) -> Result<PendingSpirit, SpiritError> {
    record.validate()?;
    if summoned_by.trim().is_empty() {
        return Err(SpiritError::Invalid("summoned_by must not be blank".into()));
    }

    let mut transaction = pool.begin().await?;
    lock(&mut transaction).await?;
    let existing = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT spirit, operator, retired_at IS NOT NULL FROM spirits WHERE room = $1",
    )
    .bind(&record.room)
    .fetch_optional(&mut *transaction)
    .await?;

    let change = match existing {
        None => {
            sqlx::query(
                "INSERT INTO spirits (room, spirit, operator, summoned_by) VALUES ($1, $2, $3, $4)",
            )
            .bind(&record.room)
            .bind(&record.spirit)
            .bind(&record.operator)
            .bind(summoned_by)
            .execute(&mut *transaction)
            .await?;
            SpiritChange::Summoned
        }
        Some((spirit, operator, _)) if spirit != record.spirit || operator != record.operator => {
            return Err(SpiritError::Refused(format!(
                "room {} already belongs to spirit {spirit:?} of operator {operator}",
                record.room
            )));
        }
        Some((_, _, true)) => {
            sqlx::query(
                "UPDATE spirits SET retired_at = NULL, summoned_by = $2, summoned_at = NOW()
                 WHERE room = $1",
            )
            .bind(&record.room)
            .bind(summoned_by)
            .execute(&mut *transaction)
            .await?;
            SpiritChange::Revived
        }
        Some(_) => {
            sqlx::query("SELECT pg_notify($1, $2)")
                .bind(SPIRITS_CHANNEL)
                .bind(&record.room)
                .execute(&mut *transaction)
                .await?;
            SpiritChange::Unchanged
        }
    };

    Ok(PendingSpirit {
        transaction,
        write: SpiritWrite {
            change,
            spirit: record.clone(),
        },
    })
}

/// Marks the room retired. A second run changes nothing.
pub async fn retire_spirit(pool: &PgPool, room: &str) -> Result<SpiritWrite, SpiritError> {
    room_key(room)?;

    let mut transaction = pool.begin().await?;
    lock(&mut transaction).await?;
    let Some((spirit, operator, retired)) = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT spirit, operator, retired_at IS NOT NULL FROM spirits WHERE room = $1",
    )
    .bind(room)
    .fetch_optional(&mut *transaction)
    .await?
    else {
        return Err(SpiritError::Refused(format!(
            "room {room} has no spirit record"
        )));
    };

    if !retired {
        sqlx::query("UPDATE spirits SET retired_at = NOW() WHERE room = $1")
            .bind(room)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;

    Ok(SpiritWrite {
        change: if retired {
            SpiritChange::Unchanged
        } else {
            SpiritChange::Retired
        },
        spirit: SpiritRecord {
            room: room.to_owned(),
            spirit,
            operator,
        },
    })
}

/// Moves a room that never woke from `old` to `new`: the record and each
/// Hallway, room-setting, spirit-text, and spellbook row of the room. With
/// `spirit`, the rename also gives the spirit that name. The rename refuses
/// when `new` has a record, or when `old` or `new` has rows in a table of a
/// woken room.
pub async fn begin_rename(
    pool: &PgPool,
    old: &str,
    new: &str,
    spirit: Option<&str>,
) -> Result<PendingSpirit, SpiritError> {
    room_key(old)?;
    room_key(new)?;
    if old == new {
        return Err(SpiritError::Invalid(
            "the new room key must differ from the old room key".into(),
        ));
    }
    if let Some(spirit) = spirit {
        spirit_name(spirit)?;
    }

    let mut transaction = pool.begin().await?;
    lock(&mut transaction).await?;
    let Some((current, operator)) = sqlx::query_as::<_, (String, String)>(
        "SELECT spirit, operator FROM spirits WHERE room = $1",
    )
    .bind(old)
    .fetch_optional(&mut *transaction)
    .await?
    else {
        return Err(SpiritError::Refused(format!(
            "room {old} has no spirit record"
        )));
    };
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM spirits WHERE room = $1)")
        .bind(new)
        .fetch_one(&mut *transaction)
        .await?;

    if taken {
        return Err(SpiritError::Refused(format!(
            "room {new} already has a spirit record"
        )));
    }
    for room in [old, new] {
        if let Some(table) = woken_table(&mut transaction, room).await? {
            return Err(SpiritError::Refused(format!(
                "room {room} has rows in {table}; rename moves only a room that never woke"
            )));
        }
    }

    let spirit = spirit.unwrap_or(&current).to_owned();
    sqlx::query("UPDATE spirits SET room = $2, spirit = $3 WHERE room = $1")
        .bind(old)
        .bind(new)
        .bind(&spirit)
        .execute(&mut *transaction)
        .await?;
    for statement in ROOM_MOVES {
        sqlx::query(statement)
            .bind(old)
            .bind(new)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query(
        "UPDATE spirit_texts SET room = $2, spirit = CASE WHEN spirit = $3 THEN $4 ELSE spirit END
         WHERE room = $1",
    )
    .bind(old)
    .bind(new)
    .bind(&current)
    .bind(&spirit)
    .execute(&mut *transaction)
    .await?;

    Ok(PendingSpirit {
        transaction,
        write: SpiritWrite {
            change: SpiritChange::Renamed,
            spirit: SpiritRecord {
                room: new.to_owned(),
                spirit,
                operator,
            },
        },
    })
}

async fn woken_table(
    connection: &mut PgConnection,
    room: &str,
) -> Result<Option<&'static str>, sqlx::Error> {
    for table in WOKEN_TABLES {
        let query = format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE room = $1)");
        let woken: bool = sqlx::query_scalar(&query)
            .bind(room)
            .fetch_one(&mut *connection)
            .await?;

        if woken {
            return Ok(Some(table));
        }
    }

    Ok(None)
}

/// Writes `records` in one transaction when `spirits` has no row at all,
/// retired rows included. Returns the number of rows that it wrote.
pub async fn import_spirits(
    pool: &PgPool,
    records: &[SpiritRecord],
    summoned_by: &str,
) -> Result<usize, SpiritError> {
    for record in records {
        record.validate()?;
    }
    if records.is_empty() {
        return Ok(0);
    }

    let mut transaction = pool.begin().await?;
    lock(&mut transaction).await?;
    let filled: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM spirits)")
        .fetch_one(&mut *transaction)
        .await?;

    if filled {
        return Ok(0);
    }
    for record in records {
        sqlx::query(
            "INSERT INTO spirits (room, spirit, operator, summoned_by) VALUES ($1, $2, $3, $4)",
        )
        .bind(&record.room)
        .bind(&record.spirit)
        .bind(&record.operator)
        .bind(summoned_by)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;

    Ok(records.len())
}
