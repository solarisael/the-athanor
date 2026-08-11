use crate::config::AppError;
use chrono::{DateTime, NaiveDate, Utc};
use house_core::{
    CanonAttribution, CanonAuthority, CanonReadRequest, CanonSelector, CanonWriteReceipt,
    CanonWriteRequest,
};
use house_protocol::{CanonEntityResult, CanonReadResult, CanonWriteResult};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};

fn postgres_id(value: u64, field: &str) -> Result<i64, AppError> {
    i64::try_from(value)
        .map_err(|_| AppError::Invalid(format!("{field} is out of PostgreSQL BIGINT range")))
}

fn canon_database_error(error: sqlx::Error) -> AppError {
    let Some(database) = error.as_database_error() else {
        return AppError::Database(error);
    };
    match database.constraint() {
        Some("named_entities_active_room_name_key") => AppError::Invalid(
            "an active canon entity with this room/name already exists; explicitly supersede its ID"
                .into(),
        ),
        Some("named_entities_superseded_by_fkey") => {
            AppError::Invalid("canon predecessor does not exist".into())
        }
        _ => AppError::Database(error),
    }
}

async fn lock_predecessors(
    tx: &mut Transaction<'_, Postgres>,
    request: &CanonWriteRequest,
    predecessor_ids: &[i64],
) -> Result<Vec<String>, AppError> {
    if predecessor_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT id,room,name,aliases,authority FROM named_entities WHERE id=ANY($1::bigint[]) ORDER BY id FOR UPDATE",
    )
    .bind(predecessor_ids)
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() != predecessor_ids.len() {
        return Err(AppError::Invalid(
            "every superseded canon entity ID must exist".into(),
        ));
    }
    let mut prior_names = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let room: String = row.try_get("room")?;
        let authority: String = row.try_get("authority")?;
        let name: String = row.try_get("name")?;
        let aliases: Vec<String> = row.try_get("aliases")?;
        if room != request.room().as_str() {
            return Err(AppError::Invalid(format!(
                "canon entity {id} belongs to another room and cannot be superseded"
            )));
        }
        if authority != "active" {
            return Err(AppError::Invalid(format!(
                "canon entity {id} is {authority}; only active authority may be superseded"
            )));
        }
        prior_names.push(name);
        prior_names.extend(aliases);
    }
    Ok(prior_names)
}

pub async fn canon_write(
    pool: &PgPool,
    request: CanonWriteRequest,
) -> Result<CanonWriteResult, AppError> {
    let predecessor_ids = request
        .supersedes()
        .iter()
        .map(|id| postgres_id(*id, "supersedes ID"))
        .collect::<Result<Vec<_>, _>>()?;
    let summary_as_of = request
        .summary_as_of()
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| AppError::Invalid("summaryAsOf must be a real YYYY-MM-DD date".into()))
        })
        .transpose()?;
    let pointer_files = Value::Array(
        request
            .pointer_files()
            .iter()
            .map(|pointer| match pointer.lines() {
                Some((start, end)) => json!({"file": pointer.file(), "lines": [start, end]}),
                None => json!({"file": pointer.file()}),
            })
            .collect(),
    );

    let mut tx = pool.begin().await?;
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended(jsonb_build_array($1::text,lower($2::text))::text,0))",
    )
    .bind(request.room().as_str())
    .bind(request.name())
    .execute(&mut *tx)
    .await?;
    let prior_names = lock_predecessors(&mut tx, &request, &predecessor_ids).await?;
    let conflicting_ids = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM named_entities WHERE room=$1 AND authority='active' AND lower(name)=lower($2) ORDER BY id FOR UPDATE",
    )
    .bind(request.room().as_str())
    .bind(request.name())
    .fetch_all(&mut *tx)
    .await?;
    if conflicting_ids
        .iter()
        .any(|id| !predecessor_ids.contains(id))
    {
        return Err(AppError::Invalid(
            "an active canon entity with this room/name already exists; explicitly supersede its ID"
                .into(),
        ));
    }
    let mut aliases = request.aliases().to_vec();
    for prior_name in prior_names {
        if !prior_name.eq_ignore_ascii_case(request.name())
            && !aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&prior_name))
        {
            aliases.push(prior_name);
        }
    }
    let entity_id: i64 =
        sqlx::query_scalar("SELECT nextval(pg_get_serial_sequence('named_entities','id'))::bigint")
            .fetch_one(&mut *tx)
            .await?;

    if !predecessor_ids.is_empty() {
        let updated = sqlx::query(
            "UPDATE named_entities SET authority='superseded',superseded_by=$1 WHERE id=ANY($2::bigint[]) AND authority='active'",
        )
        .bind(entity_id)
        .bind(&predecessor_ids)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != predecessor_ids.len() as u64 {
            return Err(AppError::Invalid(
                "canon predecessors changed before they could be superseded".into(),
            ));
        }
    }

    let meta = json!({
        "canon_writer": {
            "actor": request.attribution().actor(),
            "origin": request.attribution().origin(),
        }
    });
    sqlx::query(
        "INSERT INTO named_entities (id,room,name,kind,summary,aliases,search_boost,weighty,pointer_files,summary_as_of,meta,authority,supersedes,attributed_by,attribution_origin) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'active',$12,$13,$14)",
    )
    .bind(entity_id)
    .bind(request.room().as_str())
    .bind(request.name())
    .bind(request.kind())
    .bind(request.summary())
    .bind(&aliases)
    .bind(request.search_boost())
    .bind(request.weighty())
    .bind(Json(pointer_files))
    .bind(summary_as_of)
    .bind(Json(meta))
    .bind(&predecessor_ids)
    .bind(request.attribution().actor())
    .bind(request.attribution().origin())
    .execute(&mut *tx)
    .await
    .map_err(canon_database_error)?;

    sqlx::query("SELECT substrate_refresh_semantic_vocabulary_sources()")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let receipt = CanonWriteReceipt::new(
        u64::try_from(entity_id).map_err(|_| {
            AppError::Invalid("database returned an invalid canon entity ID".into())
        })?,
        request.room().to_string(),
        request.name().into(),
        request.supersedes().to_vec(),
        CanonAttribution::new(
            request.attribution().actor().into(),
            request.attribution().origin().into(),
        )
        .map_err(|error| AppError::Invalid(error.to_string()))?,
    )
    .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(receipt.into())
}

const ENTITY_COLUMNS: &str = "id,room,name,kind,summary,aliases,search_boost,weighty,pointer_files,summary_as_of,meta,authority,superseded_by,supersedes,attributed_by,attribution_origin,created_at,updated_at";
const ENTITY_COLUMNS_QUALIFIED: &str = "entity.id,entity.room,entity.name,entity.kind,entity.summary,entity.aliases,entity.search_boost,entity.weighty,entity.pointer_files,entity.summary_as_of,entity.meta,entity.authority,entity.superseded_by,entity.supersedes,entity.attributed_by,entity.attribution_origin,entity.created_at,entity.updated_at";

fn entity_result(row: sqlx::postgres::PgRow) -> Result<CanonEntityResult, AppError> {
    let id: i64 = row.try_get("id")?;
    let superseded_by: Option<i64> = row.try_get("superseded_by")?;
    let supersedes: Vec<i64> = row.try_get("supersedes")?;
    let summary_as_of: Option<NaiveDate> = row.try_get("summary_as_of")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    let authority = CanonAuthority::parse(&row.try_get::<String, _>("authority")?)
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(CanonEntityResult {
        entity_id: id.to_string(),
        room: row.try_get("room")?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        summary: row.try_get("summary")?,
        aliases: row.try_get("aliases")?,
        search_boost: row.try_get("search_boost")?,
        weighty: row.try_get("weighty")?,
        pointer_files: row.try_get::<Json<Value>, _>("pointer_files")?.0,
        summary_as_of: summary_as_of.map(|date| date.to_string()),
        meta: row.try_get::<Json<Value>, _>("meta")?.0,
        authority: authority.as_str().into(),
        superseded_by: superseded_by.map(|value| value.to_string()),
        supersedes: supersedes
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
        attributed_by: row.try_get("attributed_by")?,
        attribution_origin: row.try_get("attribution_origin")?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
    })
}

async fn read_by_id(
    pool: &PgPool,
    room: &str,
    id: i64,
    include_history: bool,
) -> Result<Vec<sqlx::postgres::PgRow>, AppError> {
    if include_history {
        let sql = format!(
            "WITH RECURSIVE lineage AS (SELECT {ENTITY_COLUMNS} FROM named_entities WHERE room=$1 AND id=$2 UNION SELECT {ENTITY_COLUMNS_QUALIFIED} FROM named_entities entity JOIN lineage current ON entity.room=current.room AND (entity.id=current.superseded_by OR entity.superseded_by=current.id)) SELECT {ENTITY_COLUMNS} FROM lineage ORDER BY created_at,id"
        );
        Ok(sqlx::query(&sql)
            .bind(room)
            .bind(id)
            .fetch_all(pool)
            .await?)
    } else {
        let sql = format!("SELECT {ENTITY_COLUMNS} FROM named_entities WHERE room=$1 AND id=$2");
        Ok(sqlx::query(&sql)
            .bind(room)
            .bind(id)
            .fetch_all(pool)
            .await?)
    }
}

async fn read_by_name(
    pool: &PgPool,
    room: &str,
    name: &str,
    include_history: bool,
) -> Result<Vec<sqlx::postgres::PgRow>, AppError> {
    if include_history {
        let sql = format!(
            "WITH RECURSIVE lineage AS (SELECT {ENTITY_COLUMNS} FROM named_entities WHERE room=$1 AND (lower(name)=lower($2) OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias)=lower($2))) UNION SELECT {ENTITY_COLUMNS_QUALIFIED} FROM named_entities entity JOIN lineage current ON entity.room=current.room AND (entity.id=current.superseded_by OR entity.superseded_by=current.id)) SELECT {ENTITY_COLUMNS} FROM lineage ORDER BY created_at,id"
        );
        Ok(sqlx::query(&sql)
            .bind(room)
            .bind(name)
            .fetch_all(pool)
            .await?)
    } else {
        let sql = format!(
            "SELECT {ENTITY_COLUMNS} FROM named_entities WHERE room=$1 AND authority='active' AND (lower(name)=lower($2) OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias)=lower($2))) ORDER BY id"
        );
        Ok(sqlx::query(&sql)
            .bind(room)
            .bind(name)
            .fetch_all(pool)
            .await?)
    }
}

pub async fn canon_read(
    pool: &PgPool,
    request: CanonReadRequest,
) -> Result<CanonReadResult, AppError> {
    let rows = match request.selector() {
        CanonSelector::Id(id) => {
            read_by_id(
                pool,
                request.room().as_str(),
                postgres_id(*id, "id")?,
                request.include_history(),
            )
            .await?
        }
        CanonSelector::Name(name) => {
            read_by_name(
                pool,
                request.room().as_str(),
                name,
                request.include_history(),
            )
            .await?
        }
    };
    Ok(CanonReadResult {
        ok: true,
        entities: rows
            .into_iter()
            .map(entity_result)
            .collect::<Result<Vec<_>, _>>()?,
    })
}
