use super::valid_doc_type;
use crate::config::AppError;
use crate::lesson::defaults::default_twelve;
use crate::settings::RoomSettings;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignDocumentQueryParams {
    pub system: String,
    #[serde(default)]
    pub doc_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub include_superseded: bool,
    #[serde(default = "default_twelve")]
    pub limit: u32,
}
#[derive(Debug, Serialize)]
pub struct DesignDocument {
    pub id: i64,
    pub system: String,
    pub doc_type: String,
    pub name: String,
    pub group_name: Option<String>,
    pub values: Value,
    pub body: String,
    pub provenance: Value,
    pub tags: Vec<String>,
    pub superseded_by: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentTaxonomy {
    pub doc_type: String,
    pub count: i64,
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentFilters {
    pub doc_type: Option<String>,
    pub name: Option<String>,
    pub group: Option<String>,
    pub query: Option<String>,
    pub include_superseded: bool,
    pub limit: u32,
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentQueryResult {
    pub ok: bool,
    pub system: String,
    pub filters: DesignDocumentFilters,
    pub documents: Vec<DesignDocument>,
    pub taxonomy: Vec<DesignDocumentTaxonomy>,
}
pub async fn design_document_query(
    pool: &PgPool,
    params: DesignDocumentQueryParams,
) -> Result<DesignDocumentQueryResult, AppError> {
    if params.system.trim().is_empty() {
        return Err(AppError::Invalid("system is required".into()));
    }
    if params.doc_type.as_ref().is_some_and(|v| !valid_doc_type(v)) {
        return Err(AppError::Invalid(
            "docType must be token, component, contract, or guideline".into(),
        ));
    }
    if !(1..=50).contains(&params.limit) {
        return Err(AppError::Invalid(
            "limit must be an integer from 1 through 50".into(),
        ));
    }
    let settings = RoomSettings::load(pool, "house").await?;
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT id,system,doc_type,name,group_name,\"values\",body,provenance,tags,superseded_by,created_at,updated_at FROM design_documents WHERE system=",
    );
    qb.push_bind(&params.system);
    if let Some(v) = params.doc_type.as_ref() {
        qb.push(" AND doc_type=").push_bind(v);
    }
    if let Some(v) = params.name.as_ref() {
        qb.push(" AND name=").push_bind(v);
    }
    if let Some(v) = params.group.as_ref() {
        qb.push(" AND group_name=").push_bind(v);
    }
    if !params.include_superseded {
        qb.push(" AND superseded_by IS NULL");
    }
    if let Some(v) = params.query.as_ref().filter(|v| !v.is_empty()) {
        qb.push(" AND search_tsv @@ plainto_tsquery(")
            .push_bind(settings.house_language.clone())
            .push("::regconfig,")
            .push_bind(v)
            .push(")");
    }
    let rank = params.query.clone().unwrap_or_default();
    qb.push(" ORDER BY CASE WHEN ")
        .push_bind(rank.clone())
        .push("<>'' THEN ts_rank(search_tsv,plainto_tsquery(")
        .push_bind(settings.house_language)
        .push("::regconfig,")
        .push_bind(rank)
        .push(")) ELSE 0 END DESC,updated_at DESC,id LIMIT ")
        .push_bind(i64::from(params.limit));
    let documents = qb
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|r| {
            Ok(DesignDocument {
                id: r.try_get("id")?,
                system: r.try_get("system")?,
                doc_type: r.try_get("doc_type")?,
                name: r.try_get("name")?,
                group_name: r.try_get("group_name")?,
                values: r.try_get("values")?,
                body: r.try_get("body")?,
                provenance: r.try_get("provenance")?,
                tags: r.try_get("tags")?,
                superseded_by: r.try_get("superseded_by")?,
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let mut tq = QueryBuilder::<Postgres>::new(
        "SELECT doc_type,COUNT(*) AS count FROM design_documents WHERE system=",
    );
    tq.push_bind(&params.system);
    if let Some(v) = params.doc_type.as_ref() {
        tq.push(" AND doc_type=").push_bind(v);
    }
    if !params.include_superseded {
        tq.push(" AND superseded_by IS NULL");
    }
    tq.push(" GROUP BY doc_type ORDER BY count DESC,doc_type");
    let taxonomy = tq
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|r| {
            Ok(DesignDocumentTaxonomy {
                doc_type: r.try_get("doc_type")?,
                count: r.try_get("count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(DesignDocumentQueryResult {
        ok: true,
        system: params.system,
        filters: DesignDocumentFilters {
            doc_type: params.doc_type,
            name: params.name,
            group: params.group,
            query: params.query,
            include_superseded: params.include_superseded,
            limit: params.limit,
        },
        documents,
        taxonomy,
    })
}
