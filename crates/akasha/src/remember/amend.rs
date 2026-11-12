use super::thread_join::memory_id;
use crate::config::AppError;
use crate::recall::scope::ScopeFilter;
use chrono::NaiveDate;
use hearth::{MemoryAmendment, RoomKey};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemoryAmendParams {
    pub older_memory_id: String,
    pub newer_memory_id: String,
    pub changed: String,
    #[serde(default)]
    pub keeps: Option<String>,
    pub room: String,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Serialize)]
pub struct MemoryAmendMemory {
    pub id: i64,
    pub title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAmendReceipt {
    pub ok: bool,
    pub room: String,
    pub older: MemoryAmendMemory,
    pub newer: MemoryAmendMemory,
    pub changed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keeps: Option<String>,
    pub already_amended: bool,
}

/// What one amendment write found.
pub(super) struct AmendOutcome {
    pub older_title: String,
    pub newer_title: String,
    pub already_amended: bool,
}

/// Records inside the caller's transaction that `newer_id` corrects or
/// updates `amendment.memory_id`. Both memories must be readable, unarchived
/// memories of `room`, and the newer memory must not be older by memory date.
/// The same pair with the same text changes nothing; other text for the pair
/// refuses. The memories themselves never change.
pub(super) async fn amend_tx(
    tx: &mut Transaction<'_, Postgres>,
    room: &str,
    newer_id: i64,
    amendment: &MemoryAmendment,
    operator: Option<&str>,
    scope: &ScopeFilter,
) -> Result<AmendOutcome, AppError> {
    let older_id = i64::try_from(amendment.memory_id)
        .map_err(|_| AppError::Invalid("amended memory ID must fit PostgreSQL BIGINT".into()))?;

    if older_id == newer_id {
        return Err(AppError::Invalid(format!(
            "memory {newer_id} cannot amend itself"
        )));
    }
    let sql = format!(
        "SELECT m.id,coalesce(m.title,'') AS title,
                COALESCE(m.date,m.created_at::date) AS memory_date
         FROM memories m
         WHERE m.id=ANY($1) AND m.room=$2 AND m.archived_at IS NULL{}
         ORDER BY m.id FOR UPDATE OF m",
        scope.predicate("m", 3)
    );
    let ids = vec![older_id, newer_id];
    let mut found = BTreeMap::new();
    for row in scope
        .bind(sqlx::query(&sql).bind(&ids).bind(room))
        .fetch_all(&mut **tx)
        .await?
    {
        found.insert(
            row.try_get::<i64, _>("id")?,
            (
                row.try_get::<String, _>("title")?,
                row.try_get::<NaiveDate, _>("memory_date")?,
            ),
        );
    }
    let (Some((older_title, older_date)), Some((newer_title, newer_date))) =
        (found.remove(&older_id), found.remove(&newer_id))
    else {
        let absent: Vec<String> = ids
            .iter()
            .filter(|id| !found.contains_key(id))
            .map(ToString::to_string)
            .collect();

        return Err(AppError::Invalid(format!(
            "memories are unavailable in target room {room}: {}",
            absent.join(", ")
        )));
    };

    if newer_date < older_date {
        return Err(AppError::Invalid(format!(
            "memory {newer_id} cannot amend memory {older_id}: its date {newer_date} is earlier than {older_date}"
        )));
    }
    let existing: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT older_id,changed,keeps FROM memory_amendments
         WHERE (older_id=$1 AND newer_id=$2) OR (older_id=$2 AND newer_id=$1)",
    )
    .bind(older_id)
    .bind(newer_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut already_amended = false;

    for (existing_older, changed, keeps) in existing {
        if existing_older == newer_id {
            return Err(AppError::Invalid(format!(
                "memory {older_id} already amends memory {newer_id}; the reverse amendment would make a loop"
            )));
        }
        if changed != amendment.changed || keeps != amendment.keeps {
            return Err(AppError::Invalid(format!(
                "memory {newer_id} already amends memory {older_id} with other text; the stored amendment stays"
            )));
        }
        already_amended = true;
    }
    if !already_amended {
        sqlx::query(
            "INSERT INTO memory_amendments (older_id,newer_id,changed,keeps,created_by)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(older_id)
        .bind(newer_id)
        .bind(&amendment.changed)
        .bind(&amendment.keeps)
        .bind(operator)
        .execute(&mut **tx)
        .await?;
    }

    Ok(AmendOutcome {
        older_title,
        newer_title,
        already_amended,
    })
}

pub async fn memory_amend(
    pool: &PgPool,
    p: MemoryAmendParams,
) -> Result<MemoryAmendReceipt, AppError> {
    let older_id = memory_id("olderMemoryId", &p.older_memory_id)?;
    let newer_id = memory_id("newerMemoryId", &p.newer_memory_id)?;
    let amendment = MemoryAmendment::new(older_id as u64, &p.changed, p.keeps.as_deref())
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    RoomKey::for_memory_write(&p.room).map_err(|error| AppError::Invalid(error.to_string()))?;

    let writer_scope = ScopeFilter::load(pool, p.operator.as_deref(), &mut Vec::new()).await?;
    let mut tx = pool.begin().await?;
    let outcome = amend_tx(
        &mut tx,
        &p.room,
        newer_id,
        &amendment,
        p.operator.as_deref(),
        &writer_scope,
    )
    .await?;
    tx.commit().await?;

    Ok(MemoryAmendReceipt {
        ok: true,
        room: p.room,
        older: MemoryAmendMemory {
            id: older_id,
            title: outcome.older_title,
        },
        newer: MemoryAmendMemory {
            id: newer_id,
            title: outcome.newer_title,
        },
        changed: amendment.changed,
        keeps: amendment.keeps,
        already_amended: outcome.already_amended,
    })
}
