use super::clock::timestamp;
use super::error::domain_error;
use crate::AppError;
use chrono::{DateTime, Utc};
use hearth::{GigaScope, GigaSourceRange, GigaSourceRef, GigaSourceType, GigaVisibility};
use protocol::{GigaScopeParams, GigaSourceRangeParams, GigaSourceRefParams, RequiredNullable};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};

pub(super) fn scope_parts(
    source: &GigaSourceRef,
) -> (Option<String>, Option<String>, &'static str, bool) {
    (
        source.scope().room().map(ToString::to_string),
        source.scope().project().map(str::to_owned),
        source.scope().visibility().as_str(),
        source.scope().publication_review_required(),
    )
}

pub(super) fn range_parts(source: &GigaSourceRef) -> Result<(Option<i64>, Option<i64>), AppError> {
    source.range().map_or(Ok((None, None)), |range| {
        let start = i64::try_from(range.start())
            .map_err(|_| AppError::Invalid("GIGA source range exceeds database bounds".into()))?;
        let end = i64::try_from(range.end())
            .map_err(|_| AppError::Invalid("GIGA source range exceeds database bounds".into()))?;
        Ok((Some(start), Some(end)))
    })
}

pub(super) async fn insert_event_source(
    tx: &mut Transaction<'_, Postgres>,
    event_id: &str,
    source_ordinal: usize,
    source: &GigaSourceRef,
) -> Result<(), AppError> {
    let (room, project, visibility, review_required) = scope_parts(source);
    let (range_start, range_end) = range_parts(source)?;
    sqlx::query(
        "INSERT INTO giga_event_sources
         (event_id, source_ordinal, source_type, source_id, source_role, content_hash,
          scope_room, scope_project, scope_visibility, publication_review_required,
          range_start, range_end, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(event_id)
    .bind(
        i32::try_from(source_ordinal)
            .map_err(|_| AppError::Invalid("GIGA source ordinal exceeds database bounds".into()))?,
    )
    .bind(source.source_type().as_str())
    .bind(source.source_id())
    .bind(source.role())
    .bind(source.content_hash())
    .bind(room)
    .bind(project)
    .bind(visibility)
    .bind(review_required)
    .bind(range_start)
    .bind(range_end)
    .bind(timestamp(source.timestamp())?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) fn stored_source_matches(row: &PgRow, source: &GigaSourceRef) -> Result<bool, AppError> {
    let (room, project, visibility, review_required) = scope_parts(source);
    let (range_start, range_end) = range_parts(source)?;
    Ok(
        row.try_get::<String, _>("source_type")? == source.source_type().as_str()
            && row.try_get::<String, _>("source_id")? == source.source_id()
            && row.try_get::<String, _>("source_role")? == source.role()
            && row.try_get::<String, _>("content_hash")? == source.content_hash()
            && row.try_get::<Option<String>, _>("scope_room")? == room
            && row.try_get::<Option<String>, _>("scope_project")? == project
            && row.try_get::<String, _>("scope_visibility")? == visibility
            && row.try_get::<bool, _>("publication_review_required")? == review_required
            && row.try_get::<Option<i64>, _>("range_start")? == range_start
            && row.try_get::<Option<i64>, _>("range_end")? == range_end
            && row.try_get::<DateTime<Utc>, _>("source_created_at")?
                == timestamp(source.timestamp())?,
    )
}

pub(super) async fn verify_event_source(
    tx: &mut Transaction<'_, Postgres>,
    event_id: &str,
    source: &GigaSourceRef,
) -> Result<(), AppError> {
    let row = sqlx::query(
        "SELECT source_type,source_id,source_role,content_hash,scope_room,scope_project,
                scope_visibility,publication_review_required,range_start,range_end,
                created_at AS source_created_at
         FROM giga_event_sources WHERE event_id=$1 AND source_type=$2 AND source_id=$3",
    )
    .bind(event_id)
    .bind(source.source_type().as_str())
    .bind(source.source_id())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA source is not part of its named event".into()))?;
    if !stored_source_matches(&row, source)? {
        return Err(AppError::Invalid(
            "GIGA source identity does not match its named event".into(),
        ));
    }
    Ok(())
}

pub(super) fn source_params(row: &sqlx::postgres::PgRow) -> Result<GigaSourceRefParams, AppError> {
    let start: Option<i64> = row.try_get("range_start")?;
    let end: Option<i64> = row.try_get("range_end")?;
    Ok(GigaSourceRefParams {
        source_type: row.try_get("source_type")?,
        source_id: row.try_get("source_id")?,
        role: row
            .try_get::<Option<String>, _>("source_role")?
            .unwrap_or_else(|| "source".into()),
        timestamp: row
            .try_get::<DateTime<Utc>, _>("source_created_at")?
            .to_rfc3339(),
        content_hash: row.try_get("content_hash")?,
        scope: GigaScopeParams {
            room: RequiredNullable(row.try_get("scope_room")?),
            project: RequiredNullable(row.try_get("scope_project")?),
            visibility: row.try_get("scope_visibility")?,
            publication_review_required: row.try_get("publication_review_required")?,
        },
        range: RequiredNullable(match (start, end) {
            (Some(start), Some(end)) => Some(GigaSourceRangeParams {
                start: start as u64,
                end: end as u64,
            }),
            _ => None,
        }),
    })
}

pub(super) fn source_ref_params(source: &GigaSourceRef) -> GigaSourceRefParams {
    GigaSourceRefParams {
        source_type: source.source_type().as_str().into(),
        source_id: source.source_id().into(),
        role: source.role().into(),
        timestamp: source.timestamp().into(),
        content_hash: source.content_hash().into(),
        scope: GigaScopeParams {
            room: RequiredNullable(source.scope().room().map(ToString::to_string)),
            project: RequiredNullable(source.scope().project().map(str::to_owned)),
            visibility: source.scope().visibility().as_str().into(),
            publication_review_required: source.scope().publication_review_required(),
        },
        range: RequiredNullable(source.range().map(|range| GigaSourceRangeParams {
            start: range.start(),
            end: range.end(),
        })),
    }
}

pub(super) fn source_from_row(row: &PgRow) -> Result<GigaSourceRef, AppError> {
    let range_start: Option<i64> = row.try_get("range_start")?;
    let range_end: Option<i64> = row.try_get("range_end")?;
    let range = match (range_start, range_end) {
        (None, None) => None,
        (Some(start), Some(end)) => Some(
            GigaSourceRange::new(
                u64::try_from(start)
                    .map_err(|_| AppError::Invalid("stored GIGA source range is invalid".into()))?,
                u64::try_from(end)
                    .map_err(|_| AppError::Invalid("stored GIGA source range is invalid".into()))?,
            )
            .map_err(domain_error)?,
        ),
        _ => {
            return Err(AppError::Invalid(
                "stored GIGA source range is incomplete".into(),
            ));
        }
    };
    let visibility = GigaVisibility::parse(&row.try_get::<String, _>("scope_visibility")?)
        .map_err(domain_error)?;
    let scope = GigaScope::new(
        row.try_get("scope_room")?,
        row.try_get("scope_project")?,
        visibility,
        row.try_get("publication_review_required")?,
    )
    .map_err(domain_error)?;
    GigaSourceRef::new(
        GigaSourceType::parse(&row.try_get::<String, _>("source_type")?).map_err(domain_error)?,
        row.try_get("source_id")?,
        row.try_get("source_role")?,
        row.try_get::<DateTime<Utc>, _>("source_created_at")?
            .to_rfc3339(),
        row.try_get("content_hash")?,
        scope,
        range,
    )
    .map_err(domain_error)
}

pub(super) fn candidate_scope(row: &PgRow) -> Result<GigaScope, AppError> {
    GigaScope::new(
        row.try_get("scope_room")?,
        row.try_get("scope_project")?,
        GigaVisibility::parse(&row.try_get::<String, _>("scope_visibility")?)
            .map_err(domain_error)?,
        row.try_get("publication_review_required")?,
    )
    .map_err(domain_error)
}

pub(super) async fn fresh_candidate_sources(
    tx: &mut Transaction<'_, Postgres>,
    candidate_id: &str,
) -> Result<Vec<GigaSourceRef>, AppError> {
    let rows = sqlx::query(
        "SELECT es.source_type,es.source_id,es.source_role,es.content_hash,
                es.scope_room,es.scope_project,es.scope_visibility,
                es.publication_review_required,es.range_start,es.range_end,
                es.created_at AS source_created_at,
                (
                  cs.source_role=es.source_role
                  AND cs.content_hash=es.content_hash
                  AND cs.scope_room IS NOT DISTINCT FROM es.scope_room
                  AND cs.scope_project IS NOT DISTINCT FROM es.scope_project
                  AND cs.scope_visibility=es.scope_visibility
                  AND cs.publication_review_required=es.publication_review_required
                  AND cs.range_start IS NOT DISTINCT FROM es.range_start
                  AND cs.range_end IS NOT DISTINCT FROM es.range_end
                ) AS exact
         FROM giga_candidate_sources cs
         JOIN giga_event_sources es
           ON es.event_id=cs.event_id
          AND es.source_type=cs.source_type
          AND es.source_id=cs.source_id
         WHERE cs.candidate_id=$1
         ORDER BY es.source_type,es.source_id",
    )
    .bind(candidate_id)
    .fetch_all(&mut **tx)
    .await?;
    if rows.is_empty()
        || rows
            .iter()
            .any(|row| !row.try_get::<bool, _>("exact").unwrap_or(false))
    {
        return Err(AppError::Invalid(
            "GIGA candidate sources no longer exactly match the parent event".into(),
        ));
    }
    rows.iter().map(source_from_row).collect()
}
