use super::embedding::embed_texts;
use crate::bm25f;
use crate::config::{AppError, Config, EmbeddingMode, HTTP_CLIENT};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use std::time::Duration;

pub(super) const SEMANTIC_VOCABULARY_TOP_K: i64 = 3;
// Terse concept vectors score lower than memory passages: measured paraphrases
// land at 0.30-0.37 while direct domain phrases exceed 0.45.
const SEMANTIC_VOCABULARY_MIN_SIMILARITY: f64 = 0.30;
pub(super) const SEMANTIC_VOCABULARY_MAX_TERMS: usize = 12;

pub(super) fn semantic_vocabulary_terms(concepts: &[serde_json::Value]) -> Vec<String> {
    concepts
        .iter()
        .flat_map(|concept| {
            concept["terms"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
        })
        .flat_map(bm25f::query_terms)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(SEMANTIC_VOCABULARY_MAX_TERMS)
        .collect()
}

pub(super) async fn load_semantic_vocabulary_concepts(
    pool: &PgPool,
    rooms: &[String],
    vector_text: &str,
    cfg: &Config,
) -> Result<Vec<serde_json::Value>, AppError> {
    let rows =
        sqlx::query(
            r#"SELECT concept, lexical_terms, source_kind,
                  (1 - (embedding <=> $1::vector))::double precision AS similarity
           FROM semantic_vocabulary
           WHERE room = ANY($2::text[])
             AND approved_source_kind = source_kind
             AND embedding IS NOT NULL
             AND embedding_model = $3
             AND embedding_dimension = $4
             AND embedded_at >= source_updated_at
           ORDER BY similarity DESC, concept, source_kind, source_key
           LIMIT $5"#,
        )
        .bind(vector_text)
        .bind(rooms)
        .bind(&cfg.embed_model)
        .bind(i32::try_from(cfg.embed_dimension).map_err(|_| {
            AppError::Config("embedding dimension exceeds PostgreSQL integer".into())
        })?)
        .bind(SEMANTIC_VOCABULARY_TOP_K)
        .fetch_all(pool)
        .await?;
    let mut concepts = Vec::with_capacity(rows.len());
    for row in rows {
        let similarity: f64 = row.try_get("similarity")?;
        if similarity < SEMANTIC_VOCABULARY_MIN_SIMILARITY {
            continue;
        }
        concepts.push(serde_json::json!({
            "concept": row.try_get::<String, _>("concept")?,
            "terms": row.try_get::<Vec<String>, _>("lexical_terms")?,
            "source_kind": row.try_get::<String, _>("source_kind")?,
            "similarity": similarity,
        }));
    }
    Ok(concepts)
}

pub async fn refresh_semantic_vocabulary(pool: &PgPool, cfg: &Config) -> Result<usize, AppError> {
    match cfg.embedding_mode {
        EmbeddingMode::Required => {}
        EmbeddingMode::Disabled => {
            return Err(AppError::Config(
                "semantic vocabulary refresh is unavailable while embedding is disabled in production"
                    .into(),
            ));
        }
        EmbeddingMode::DisabledForTest => {
            return Err(AppError::Config(
                "semantic vocabulary refresh is unavailable while embedding is disabled for isolated test"
                    .into(),
            ));
        }
    }
    let url = cfg.embed_url.as_deref().ok_or_else(|| {
        AppError::Config("semantic vocabulary refresh requires SOLARISAEL_EMBED_URL".into())
    })?;
    sqlx::query("SELECT substrate_refresh_semantic_vocabulary_sources()")
        .execute(pool)
        .await?;
    let rows =
        sqlx::query(
            r#"SELECT room, source_kind, source_key, concept, lexical_terms
           FROM semantic_vocabulary
           WHERE embedding IS NULL
              OR embedding_model IS DISTINCT FROM $1
              OR embedding_dimension IS DISTINCT FROM $2
              OR embedded_at < source_updated_at
           ORDER BY room, source_kind, source_key"#,
        )
        .bind(&cfg.embed_model)
        .bind(i32::try_from(cfg.embed_dimension).map_err(|_| {
            AppError::Config("embedding dimension exceeds PostgreSQL integer".into())
        })?)
        .fetch_all(pool)
        .await?;
    const VOCABULARY_EMBED_BATCH_SIZE: usize = 16;
    for batch in rows.chunks(VOCABULARY_EMBED_BATCH_SIZE) {
        let passages = batch
            .iter()
            .map(|row| {
                let concept = row.try_get::<String, _>("concept")?;
                let terms = row.try_get::<Vec<String>, _>("lexical_terms")?;
                Ok(format!("{concept}\n{}", terms.join(" ")))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let vectors = embed_texts(
            &HTTP_CLIENT,
            url,
            &cfg.embed_model,
            "passage",
            &passages,
            cfg.embed_dimension,
            Duration::from_secs(120),
        )
        .await?;
        for (row, vector) in batch.iter().zip(vectors) {
            let vector_text = format!(
                "[{}]",
                vector
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            sqlx::query(
                r#"UPDATE semantic_vocabulary
                   SET embedding = $1::vector, embedding_model = $2, embedding_dimension = $3, embedded_at = NOW()
                   WHERE room = $4 AND source_kind = $5 AND source_key = $6"#,
            )
            .bind(vector_text)
            .bind(&cfg.embed_model)
            .bind(i32::try_from(cfg.embed_dimension).map_err(|_| AppError::Config("embedding dimension exceeds PostgreSQL integer".into()))?)
            .bind(row.try_get::<String, _>("room")?)
            .bind(row.try_get::<String, _>("source_kind")?)
            .bind(row.try_get::<String, _>("source_key")?)
            .execute(pool)
            .await?;
        }
    }
    Ok(rows.len())
}
