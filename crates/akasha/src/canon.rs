use crate::{canon_scope, config::AppError};
use chrono::{DateTime, NaiveDate, Utc};
use hearth::{
    CanonAttribution, CanonAuthority, CanonReadRequest, CanonScope, CanonSelector,
    CanonWriteReceipt, CanonWriteRequest,
};
use protocol::{CanonEntityResult, CanonReadResult, CanonWriteResult};
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
        Some("named_entities_active_room_subject_name_key") => AppError::Invalid(
            "an active canon entity with this room/name already exists; explicitly supersede its ID"
                .into(),
        ),
        Some("named_entities_superseded_by_fkey") => AppError::Invalid(
            "every superseded canon entity must exist in the destination room".into(),
        ),
        Some("named_entities_subject_fkey") => {
            AppError::Invalid("the subject is not a registered operator".into())
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

    let sql = format!(
        "SELECT entity.id,entity.name,entity.aliases,entity.authority,entity.subject,entity.scope FROM named_entities entity WHERE entity.id=ANY($1::bigint[]) AND entity.room=$2{} ORDER BY entity.id FOR UPDATE",
        canon_scope::visible("entity", 3)
    );
    let rows = sqlx::query(&sql)
        .bind(predecessor_ids)
        .bind(request.room().as_str())
        .bind(request.writer().map(|writer| writer.as_str()))
        .fetch_all(&mut **tx)
        .await?;

    if rows.len() != predecessor_ids.len() {
        return Err(AppError::Invalid(
            "every superseded canon entity must exist in the destination room".into(),
        ));
    }

    let successor_is_public = request.subject().is_none() || request.scope() == CanonScope::Company;

    let mut prior_names = Vec::new();

    for row in rows {
        let id: i64 = row.try_get("id")?;
        let authority: String = row.try_get("authority")?;

        if authority != "active" {
            return Err(AppError::Invalid(format!(
                "canon entity {id} is {authority}; only active authority may be superseded"
            )));
        }

        if successor_is_public {
            let subject: Option<&str> = row.try_get("subject")?;
            let scope: &str = row.try_get("scope")?;

            if subject.is_some() && scope != "company" {
                // Inherited labels must be visible to every reader of the successor.
                continue;
            }
        }

        let name: String = row.try_get("name")?;
        let aliases: Vec<String> = row.try_get("aliases")?;

        prior_names.push(name);
        prior_names.extend(aliases);
    }

    Ok(prior_names)
}

// The canon write set, named once and spent twice by the INSERT below. It is
// deliberately not ENTITY_COLUMNS: summary_tsv is GENERATED ALWAYS
// (0001_initial.sql:73) and created_at/updated_at keep their DDL DEFAULT NOW()
// (0001_initial.sql:78-79), so none of the three may travel in the record.
const ENTITY_INSERT_COLUMNS: &str = "id,room,subject,scope,min_elevation,name,kind,summary,aliases,search_boost,weighty,pointer_files,summary_as_of,meta,authority,supersedes,attributed_by,attribution_origin";

// Keys are named_entities column names; the table is the only schema
// (0001_initial.sql:63-81, 0015_canon_authority.sql:3-8). jsonb_populate_record
// matches by NAME, so a wrong or missing key dies as a NOT NULL refusal here
// instead of a silent positional swap. A new NOT NULL column must be added to
// this function and to ENTITY_INSERT_COLUMNS; the first write refuses loudly.
fn entity_row(
    entity_id: i64,
    request: &CanonWriteRequest,
    aliases: &[String],
    pointer_files: &Value,
    summary_as_of: Option<NaiveDate>,
    predecessor_ids: &[i64],
) -> Value {
    json!({
        "id": entity_id,
        "room": request.room().as_str(),
        "subject": request.subject().map(|subject| subject.as_str()),
        "scope": request.scope().as_str(),
        "min_elevation": request.min_elevation(),
        "name": request.name(),
        "kind": request.kind(),
        "summary": request.summary(),
        "aliases": aliases,
        "search_boost": request.search_boost(),
        "weighty": request.weighty(),
        "pointer_files": pointer_files,
        "summary_as_of": summary_as_of,
        "meta": {
            "canon_writer": {
                "actor": request.attribution().actor(),
                "origin": request.attribution().origin(),
            }
        },
        "authority": "active",
        "supersedes": predecessor_ids,
        "attributed_by": request.attribution().actor(),
        "attribution_origin": request.attribution().origin(),
    })
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

    let subject = request.subject().map(|subject| subject.as_str());
    let writer = request.writer().map(|writer| writer.as_str());

    let mut tx = pool.begin().await?;
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended(jsonb_build_array($1::text,COALESCE($2::text,''),lower($3::text))::text,0))",
    )
    .bind(request.room().as_str())
    .bind(subject)
    .bind(request.name())
    .execute(&mut *tx)
    .await?;
    let prior_names = lock_predecessors(&mut tx, &request, &predecessor_ids).await?;
    let sql = format!(
        "SELECT entity.id FROM named_entities entity WHERE entity.room=$1 AND COALESCE(entity.subject,'')=COALESCE($2::text,'') AND entity.authority='active' AND lower(entity.name)=lower($3){} ORDER BY entity.id FOR UPDATE",
        canon_scope::visible("entity", 4)
    );
    let conflicting_ids = sqlx::query_scalar::<_, i64>(&sql)
        .bind(request.room().as_str())
        .bind(subject)
        .bind(request.name())
        .bind(writer)
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

    let row = entity_row(
        entity_id,
        &request,
        &aliases,
        &pointer_files,
        summary_as_of,
        &predecessor_ids,
    );
    let sql = format!(
        "INSERT INTO named_entities ({ENTITY_INSERT_COLUMNS}) SELECT {ENTITY_INSERT_COLUMNS} FROM jsonb_populate_record(NULL::named_entities,$1)"
    );
    sqlx::query(&sql)
        .bind(row)
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

const ENTITY_COLUMNS: &str = "entity.id,entity.room,entity.subject,entity.scope,entity.name,entity.kind,entity.summary,entity.aliases,entity.search_boost,entity.weighty,entity.pointer_files,entity.summary_as_of,entity.meta,entity.authority,entity.attributed_by,entity.attribution_origin,entity.created_at,entity.updated_at";

fn visible_entity_columns(param: usize) -> String {
    format!(
        "{ENTITY_COLUMNS},{} AS superseded_by,{} AS supersedes",
        canon_scope::visible_successor("entity", param),
        canon_scope::visible_predecessors("entity", param)
    )
}

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
        subject: row.try_get("subject")?,
        scope: row.try_get("scope")?,
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
    reader: Option<&str>,
) -> Result<Vec<sqlx::postgres::PgRow>, AppError> {
    let visibility = canon_scope::visible("entity", 3);
    let columns = visible_entity_columns(3);
    let sql = if include_history {
        format!(
            "WITH RECURSIVE lineage AS (
                SELECT {ENTITY_COLUMNS},entity.superseded_by,entity.supersedes
                FROM named_entities entity
                WHERE entity.room=$1 AND entity.id=$2{visibility}
                UNION
                SELECT {ENTITY_COLUMNS},entity.superseded_by,entity.supersedes
                FROM named_entities entity
                JOIN lineage current ON (entity.id=current.superseded_by OR entity.superseded_by=current.id)
                WHERE entity.room=current.room{visibility}
             )
             SELECT {columns} FROM lineage entity ORDER BY entity.created_at,entity.id"
        )
    } else {
        format!(
            "SELECT {columns} FROM named_entities entity WHERE entity.room=$1 AND entity.id=$2{visibility}"
        )
    };

    Ok(sqlx::query(&sql)
        .bind(room)
        .bind(id)
        .bind(reader)
        .fetch_all(pool)
        .await?)
}

async fn read_by_name(
    pool: &PgPool,
    room: &str,
    name: &str,
    include_history: bool,
    reader: Option<&str>,
) -> Result<Vec<sqlx::postgres::PgRow>, AppError> {
    let visibility = canon_scope::visible("entity", 3);
    let columns = visible_entity_columns(3);
    let sql = if include_history {
        format!(
            "WITH RECURSIVE lineage AS (
                SELECT {ENTITY_COLUMNS},entity.superseded_by,entity.supersedes
                FROM named_entities entity
                WHERE entity.room=$1 AND (lower(entity.name)=lower($2) OR EXISTS (SELECT 1 FROM unnest(entity.aliases) alias WHERE lower(alias)=lower($2))){visibility}
                UNION
                SELECT {ENTITY_COLUMNS},entity.superseded_by,entity.supersedes
                FROM named_entities entity
                JOIN lineage current ON (entity.id=current.superseded_by OR entity.superseded_by=current.id)
                WHERE entity.room=current.room{visibility}
             )
             SELECT {columns} FROM lineage entity ORDER BY entity.created_at,entity.id"
        )
    } else {
        format!(
            "SELECT {columns} FROM named_entities entity WHERE entity.room=$1 AND entity.authority='active' AND (lower(entity.name)=lower($2) OR EXISTS (SELECT 1 FROM unnest(entity.aliases) alias WHERE lower(alias)=lower($2))){visibility} ORDER BY entity.id"
        )
    };

    Ok(sqlx::query(&sql)
        .bind(room)
        .bind(name)
        .bind(reader)
        .fetch_all(pool)
        .await?)
}

async fn read_by_subject(
    pool: &PgPool,
    room: &str,
    subject: &str,
    name: Option<&str>,
    include_history: bool,
    reader: Option<&str>,
) -> Result<Vec<sqlx::postgres::PgRow>, AppError> {
    let visibility = canon_scope::visible("entity", 4);
    let columns = visible_entity_columns(4);
    let sql = if include_history {
        format!(
            "WITH RECURSIVE lineage AS (
                SELECT {ENTITY_COLUMNS},entity.superseded_by,entity.supersedes
                FROM named_entities entity
                WHERE entity.room IN ($1,'house') AND entity.subject=$2 AND ($3::text IS NULL OR lower(entity.name)=lower($3) OR EXISTS (SELECT 1 FROM unnest(entity.aliases) alias WHERE lower(alias)=lower($3))){visibility}
                UNION
                SELECT {ENTITY_COLUMNS},entity.superseded_by,entity.supersedes
                FROM named_entities entity
                JOIN lineage current ON (entity.id=current.superseded_by OR entity.superseded_by=current.id)
                WHERE entity.room=current.room{visibility}
             )
             SELECT {columns} FROM lineage entity ORDER BY entity.created_at,entity.id"
        )
    } else {
        format!(
            "SELECT {columns} FROM named_entities entity WHERE entity.room IN ($1,'house') AND entity.subject=$2 AND entity.authority='active' AND ($3::text IS NULL OR lower(entity.name)=lower($3) OR EXISTS (SELECT 1 FROM unnest(entity.aliases) alias WHERE lower(alias)=lower($3))){visibility} ORDER BY entity.id"
        )
    };

    Ok(sqlx::query(&sql)
        .bind(room)
        .bind(subject)
        .bind(name)
        .bind(reader)
        .fetch_all(pool)
        .await?)
}

pub async fn canon_read(
    pool: &PgPool,
    request: CanonReadRequest,
) -> Result<CanonReadResult, AppError> {
    let reader = request.reader().map(|reader| reader.as_str());

    let rows = match request.selector() {
        CanonSelector::Id(id) => {
            read_by_id(
                pool,
                request.room().as_str(),
                postgres_id(*id, "id")?,
                request.include_history(),
                reader,
            )
            .await?
        }
        CanonSelector::Name(name) => {
            read_by_name(
                pool,
                request.room().as_str(),
                name,
                request.include_history(),
                reader,
            )
            .await?
        }
        CanonSelector::Subject { subject, name } => {
            read_by_subject(
                pool,
                request.room().as_str(),
                subject.as_str(),
                name.as_deref(),
                request.include_history(),
                reader,
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
