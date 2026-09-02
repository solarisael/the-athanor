use super::valid_doc_type;
use crate::config::AppError;
use crate::lesson::coerce::deserialize_optional_i64;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DesignDocumentWriteParams {
    pub system: String,
    pub doc_type: String,
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default = "empty_object")]
    pub values: Value,
    #[serde(default)]
    pub body: String,
    #[serde(default = "empty_object")]
    pub provenance: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub supersedes: Option<i64>,
    #[serde(default)]
    pub allow_identity_change: bool,
}
fn empty_object() -> Value {
    serde_json::json!({})
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentWriteReceipt {
    pub ok: bool,
    pub system: String,
    pub doc_type: String,
    pub name: String,
    pub superseded: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
fn design_refusal(
    p: &DesignDocumentWriteParams,
    error: impl Into<String>,
) -> DesignDocumentWriteReceipt {
    DesignDocumentWriteReceipt {
        ok: false,
        system: p.system.clone(),
        doc_type: p.doc_type.clone(),
        name: p.name.clone(),
        superseded: vec![],
        id: None,
        error: Some(error.into()),
    }
}
pub async fn design_document_write(
    pool: &PgPool,
    p: DesignDocumentWriteParams,
) -> Result<DesignDocumentWriteReceipt, AppError> {
    if p.system.trim().is_empty() {
        return Ok(design_refusal(&p, "system is required"));
    }
    if !valid_doc_type(&p.doc_type) {
        return Ok(design_refusal(
            &p,
            "doc_type must be one of: token, component, contract, guideline",
        ));
    }
    if p.name.trim().is_empty() {
        return Ok(design_refusal(&p, "name is required"));
    }
    if !p.values.is_object() {
        return Ok(design_refusal(&p, "values must be a JSON object"));
    }
    if !p.provenance.is_object() {
        return Ok(design_refusal(&p, "provenance must be a JSON object"));
    }
    if p.supersedes.is_some_and(|id| id <= 0) {
        return Ok(design_refusal(&p, "supersedes must be a positive integer"));
    }
    let system = p.system.trim().to_string();
    let name = p.name.trim().to_string();
    let mut tx = pool.begin().await?;
    if let Some(old) = p.supersedes {
        let row = sqlx::query("SELECT system,doc_type,name,superseded_by FROM design_documents WHERE id=$1 FOR UPDATE")
            .bind(old).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return Ok(design_refusal(&p, "superseded document not found"));
        };
        let superseded: Option<i64> = row.try_get("superseded_by")?;
        if superseded.is_some() {
            return Ok(design_refusal(
                &p,
                "superseded document is already superseded",
            ));
        }
        let previous_system: String = row.try_get("system")?;
        let previous_type: String = row.try_get("doc_type")?;
        let previous_name: String = row.try_get("name")?;
        let same =
            previous_system == system && previous_type == p.doc_type && previous_name == name;
        if !same && !p.allow_identity_change {
            return Ok(design_refusal(
                &p,
                "superseded document identity differs; pass --allow-identity-change",
            ));
        }
    }
    let id: i64 = sqlx::query_scalar("INSERT INTO design_documents(system,doc_type,name,group_name,\"values\",body,provenance,tags) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(&system).bind(&p.doc_type).bind(&name).bind(&p.group).bind(&p.values)
        .bind(&p.body).bind(&p.provenance).bind(&p.tags).fetch_one(&mut *tx).await?;
    if let Some(old) = p.supersedes {
        let changed = sqlx::query(
            "UPDATE design_documents SET superseded_by=$1 WHERE id=$2 AND superseded_by IS NULL",
        )
        .bind(id)
        .bind(old)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed != 1 {
            return Ok(design_refusal(
                &p,
                "supersession affected an unexpected number of rows",
            ));
        }
    }
    tx.commit().await?;
    Ok(DesignDocumentWriteReceipt {
        ok: true,
        system,
        doc_type: p.doc_type,
        name,
        superseded: p.supersedes.into_iter().collect(),
        id: Some(id),
        error: None,
    })
}
