use crate::bm25f::{
    self, BODY as BM25F_BODY, FieldValue as Bm25fFieldValue, HEADING as BM25F_HEADING,
    MEMORY_TYPE as BM25F_MEMORY_TYPE, SOURCE_PATH as BM25F_SOURCE_PATH, THREADS as BM25F_THREADS,
    TITLE as BM25F_TITLE,
};
use crate::cluster::{cluster_resonance, cluster_staleness};
use crate::config::{AppError, Config, EMBED_DIMENSION, HTTP_CLIENT, QUERY_DATE_RE, ROOM_KEY_RE};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

fn default_semantic_top_k() -> u32 {
    8
}
// Calibration bound to the embedding model, not a strictness knob. On
// Nemotron-3-Embed-1B/Q4 correct rank-1 matches land at 0.42-0.56 and noise tops
// out at 0.24, so the earlier 0.50 sat inside the signal band and cut correct
// hits. Re-measure on any model/quantization change — coding lesson 222.
fn default_semantic_min_similarity() -> f64 {
    0.40
}
fn default_content_top_k() -> u32 {
    8
}
fn default_content_min_similarity() -> f64 {
    0.30
}
fn default_temporal_decay() -> bool {
    false
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecallParams {
    pub room: String,
    pub query: String,
    #[serde(default = "default_semantic_top_k")]
    pub semantic_top_k: u32,
    #[serde(default = "default_semantic_min_similarity")]
    pub semantic_min_similarity: f64,
    #[serde(default = "default_temporal_decay")]
    pub temporal_decay: bool,
    #[serde(default = "default_content_top_k")]
    pub content_top_k: u32,
    #[serde(default = "default_content_min_similarity")]
    pub content_min_similarity: f64,
}

impl RecallParams {
    pub fn validate(&self) -> Result<(), AppError> {
        if !ROOM_KEY_RE.is_match(&self.room) || self.room == "house" {
            return Err(AppError::Invalid(
                "room must be a lowercase slug and cannot be house".into(),
            ));
        }
        if self.query.trim().is_empty() {
            return Err(AppError::Invalid("query must not be empty".into()));
        }
        for (name, value) in [
            ("semantic_top_k", self.semantic_top_k),
            ("content_top_k", self.content_top_k),
        ] {
            if value == 0 || value > 1000 {
                return Err(AppError::Invalid(format!(
                    "{name} must be positive and at most 1000"
                )));
            }
        }
        for (name, value) in [
            ("semantic_min_similarity", self.semantic_min_similarity),
            ("content_min_similarity", self.content_min_similarity),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(AppError::Invalid(format!(
                    "{name} must be finite and in [0, 1]"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct RecallResult {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: &'static str,
    pub warnings: Vec<String>,
    #[serde(rename = "retrievalCandidates")]
    pub retrieval_candidates: Vec<serde_json::Value>,
    #[serde(rename = "canonMatches")]
    pub canon_matches: Vec<serde_json::Value>,
    #[serde(rename = "semanticChunks")]
    pub semantic_chunks: Vec<serde_json::Value>,
    #[serde(rename = "contentChunks")]
    pub content_chunks: Vec<serde_json::Value>,
    #[serde(rename = "dateMatches")]
    pub date_matches: Vec<serde_json::Value>,
    #[serde(rename = "queryDates")]
    pub query_dates: Vec<serde_json::Value>,
    pub taxonomy: serde_json::Value,
    #[serde(rename = "clusterStaleness", skip_serializing_if = "Option::is_none")]
    pub cluster_staleness: Option<serde_json::Value>,
    #[serde(rename = "clusterResonance", skip_serializing_if = "Option::is_none")]
    pub cluster_resonance: Option<serde_json::Value>,
}

fn query_dates(query: &str) -> Vec<NaiveDate> {
    QUERY_DATE_RE
        .captures_iter(query)
        .filter_map(|c| {
            NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
pub(crate) fn query_terms(query: &str) -> Vec<String> {
    let mut terms = query
        .split(|c: char| !c.is_alphanumeric())
        .map(|term| term.trim().to_lowercase())
        .filter(|term| term.len() >= 2)
        .collect::<BTreeSet<_>>();
    // Compound tokens split only on whitespace so `-`, `/`, `.`, `_` survive
    // inside a token (pais/mais, queue-maintenance); edge punctuation that is
    // not part of a compound (quotes, commas, trailing periods) is trimmed.
    terms.extend(
        query
            .split_whitespace()
            .map(|token| {
                token
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|token| token.len() >= 2),
    );
    terms.into_iter().collect()
}

pub(crate) fn term_evidence(terms: &[String], fields: &[&str]) -> (Vec<String>, Vec<String>) {
    let haystack = fields.join(" ").to_lowercase();
    let matched = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let missing = terms
        .iter()
        .filter(|term| !matched.contains(term))
        .cloned()
        .collect::<Vec<_>>();
    (matched, missing)
}

// The `query: ` / `passage: ` prefix pair is load-bearing. Do not remove it.
// Without prefixes this model floors unrelated text near 0.50: absolute values
// look higher while separating nothing, and the true/noise gap goes negative.
// The indexer's `passage: ` and this `query: ` are one decision — project
// lesson 126 (the-athanor) carries the measured comparison.
async fn embed_texts(
    client: &Client,
    url: &str,
    model: &str,
    prefix: &str,
    texts: &[String],
    dim: usize,
    timeout: Duration,
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    // The embedder is on the hot path — every user prompt embeds a query. Ollama's
    // default 5 minute keep_alive evicted it during idle gaps and charged a cold
    // reload on the next turn, which is felt directly as latency. Residency costs
    // 2.1 GB of VRAM; measured 2026-07-25.
    #[derive(Serialize)]
    struct Input<'a> {
        model: &'a str,
        input: Vec<String>,
        keep_alive: &'a str,
    }
    let value: serde_json::Value = client
        .post(url)
        .timeout(timeout)
        .json(&Input {
            model,
            input: texts
                .iter()
                .map(|text| format!("{prefix}: {text}"))
                .collect(),
            keep_alive: "30m",
        })
        .send()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let items = value
        .get("embeddings")
        .or_else(|| value.get("data"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| AppError::Embedding("response lacks embeddings".into()))?;
    if items.len() != texts.len() {
        return Err(AppError::Embedding(format!(
            "embedding count {} != {}",
            items.len(),
            texts.len()
        )));
    }
    items
        .iter()
        .map(|item| {
            let values = item
                .as_array()
                .or_else(|| item.get("embedding").and_then(serde_json::Value::as_array))
                .ok_or_else(|| AppError::Embedding("invalid embedding".into()))?;
            let row = values
                .iter()
                .map(|value| {
                    value
                        .as_f64()
                        .map(|number| number as f32)
                        .ok_or_else(|| AppError::Embedding("non-numeric embedding".into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if row.len() != dim {
                return Err(AppError::Embedding(format!(
                    "embedding dimension {} != {}",
                    row.len(),
                    dim
                )));
            }
            Ok(row)
        })
        .collect()
}

async fn embed_text(
    client: &Client,
    url: &str,
    model: &str,
    prefix: &str,
    text: &str,
    dim: usize,
) -> Result<Vec<f32>, AppError> {
    embed_texts(
        client,
        url,
        model,
        prefix,
        &[text.to_owned()],
        dim,
        Duration::from_secs(20),
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| AppError::Embedding("response lacks embedding".into()))
}

async fn embed_query(
    client: &Client,
    url: &str,
    model: &str,
    query: &str,
    dim: usize,
) -> Result<Vec<f32>, AppError> {
    embed_text(client, url, model, "query", query, dim).await
}

pub(crate) fn bounded_excerpt(body: &str) -> String {
    const MAX: usize = 1200;
    let excerpt: String = body.chars().take(MAX).collect();
    if body.chars().count() > MAX {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

pub(crate) fn candidate_terms(
    terms: &[String],
    fields: &[&str],
) -> (Vec<String>, Vec<String>, f64) {
    let (matched, missing) = term_evidence(terms, fields);
    let coverage = if terms.is_empty() {
        0.0
    } else {
        matched.len() as f64 / terms.len() as f64
    };
    (matched, missing, coverage)
}

fn giga_temporal_factor(meta: &serde_json::Value, now: DateTime<Utc>) -> (Option<f64>, f64) {
    let Some(object) = meta.as_object() else {
        return (None, 1.0);
    };
    if object.get("origin").and_then(|value| value.as_str()) != Some("giga-promotion") {
        return (None, 1.0);
    }
    let Some(giga) = object.get("giga").and_then(|value| value.as_object()) else {
        return (None, 1.0);
    };
    let Some(durability) = giga.get("durability").and_then(|value| value.as_f64()) else {
        return (None, 1.0);
    };
    if !durability.is_finite()
        || !(0.0..=1.0).contains(&durability)
        || giga.get("decay_anchor").and_then(|value| value.as_str()) != Some("candidate_created_at")
    {
        return (None, 1.0);
    }
    let Some(anchor) = giga
        .get("decay_anchor_at")
        .and_then(|value| value.as_str())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
    else {
        return (None, 1.0);
    };
    let age_days = (now - anchor).num_seconds() as f64 / 86_400.0;
    if age_days <= 0.0 || durability >= 1.0 {
        return (Some(durability), 1.0);
    }
    let factor = (-std::f64::consts::LN_2 * age_days * (1.0 - durability).powi(2) / 7.0).exp();
    (
        Some(durability),
        if factor.is_finite() { factor } else { 1.0 },
    )
}

fn weighted_lane_score(chunk: &serde_json::Value, score_field: &str) -> f64 {
    chunk[score_field].as_f64().unwrap_or(0.0) * chunk["temporal_weight"].as_f64().unwrap_or(1.0)
}

fn compare_weighted_lane(
    left: &serde_json::Value,
    right: &serde_json::Value,
    score_field: &str,
) -> std::cmp::Ordering {
    weighted_lane_score(right, score_field)
        .partial_cmp(&weighted_lane_score(left, score_field))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            left["source_path"]
                .as_str()
                .cmp(&right["source_path"].as_str())
        })
        .then_with(|| {
            left["chunk_index"]
                .as_i64()
                .cmp(&right["chunk_index"].as_i64())
        })
}
const BM25F_TOP_K: usize = 8;
const BM25F_MAX_CANDIDATES: i64 = 512;

#[derive(Clone, Copy)]
struct Bm25fAverageLengths {
    title: f64,
    heading: f64,
    source_path: f64,
    threads: f64,
    body: f64,
    memory_type: f64,
}

async fn load_bm25f_candidates_for_terms(
    pool: &PgPool,
    rooms: &[String],
    terms: &[String],
    temporal_decay: bool,
    decay_now: DateTime<Utc>,
    warnings: &mut Vec<String>,
) -> Result<Vec<serde_json::Value>, AppError> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let stats = sqlx::query(
        r#"SELECT count(*)::bigint AS document_count,
                  coalesce(avg(m.bm25f_title_length)::double precision,1.0) AS avg_title,
                  coalesce(avg(c.bm25f_heading_length)::double precision,1.0) AS avg_heading,
                  coalesce(avg(m.bm25f_source_path_length)::double precision,1.0) AS avg_source_path,
                  coalesce(avg(m.bm25f_threads_length)::double precision,1.0) AS avg_threads,
                  coalesce(avg(c.bm25f_body_length)::double precision,1.0) AS avg_body,
                  coalesce(avg(m.bm25f_type_length)::double precision,1.0) AS avg_memory_type
           FROM memory_chunks c
           JOIN memories m ON m.id=c.memory_id
           WHERE m.room = ANY($1::text[])
             AND m.archived_at IS NULL
             AND m.superseded_by IS NULL
             AND COALESCE(m.type,'') <> 'paper-boat'"#,
    )
    .bind(rooms)
    .fetch_one(pool)
    .await?;
    let document_count: i64 = stats.try_get("document_count")?;
    if document_count == 0 {
        return Ok(Vec::new());
    }
    let averages = Bm25fAverageLengths {
        title: stats.try_get("avg_title")?,
        heading: stats.try_get("avg_heading")?,
        source_path: stats.try_get("avg_source_path")?,
        threads: stats.try_get("avg_threads")?,
        body: stats.try_get("avg_body")?,
        memory_type: stats.try_get("avg_memory_type")?,
    };

    let frequency_rows = sqlx::query(
        r#"WITH corpus AS MATERIALIZED (
             SELECT c.bm25f_text_tsv,m.bm25f_meta_tsv
             FROM memory_chunks c
             JOIN memories m ON m.id=c.memory_id
             WHERE m.room = ANY($1::text[])
               AND m.archived_at IS NULL
               AND m.superseded_by IS NULL
               AND COALESCE(m.type,'') <> 'paper-boat'
           )
           SELECT term,
                  count(*) FILTER (
                    WHERE corpus.bm25f_text_tsv @@ plainto_tsquery('simple',term)
                       OR corpus.bm25f_meta_tsv @@ plainto_tsquery('simple',term)
                  )::bigint AS document_frequency
           FROM unnest($2::text[]) AS term
           CROSS JOIN corpus
           GROUP BY term
           ORDER BY term"#,
    )
    .bind(rooms)
    .bind(&terms)
    .fetch_all(pool)
    .await?;
    let document_frequency = frequency_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("term")?,
                row.try_get::<i64, _>("document_frequency")? as u64,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()?;

    let rows = sqlx::query(
        r#"WITH terms AS MATERIALIZED (
             SELECT term,plainto_tsquery('simple',term) AS query
             FROM unnest($2::text[]) AS term
           )
           SELECT m.id AS memory_id,m.source_path,coalesce(m.title,'') AS title,
                  coalesce(c.heading_path,'') AS heading_path,c.body,c.chunk_index,m.meta,
                  array_to_string(m.threads,' ') AS threads,m.type AS memory_type,
                  count(*) OVER()::bigint AS total_matches
           FROM memory_chunks c
           JOIN memories m ON m.id=c.memory_id
           WHERE m.room = ANY($1::text[])
             AND m.archived_at IS NULL
             AND m.superseded_by IS NULL
             AND COALESCE(m.type,'') <> 'paper-boat'
             AND EXISTS (
               SELECT 1 FROM terms
               WHERE c.bm25f_text_tsv @@ terms.query
                  OR m.bm25f_meta_tsv @@ terms.query
             )
           ORDER BY (
             SELECT coalesce(sum(ts_rank_cd(
               c.bm25f_text_tsv || m.bm25f_meta_tsv,
               terms.query,
               32
             )),0.0)
             FROM terms
             WHERE c.bm25f_text_tsv @@ terms.query
                OR m.bm25f_meta_tsv @@ terms.query
           ) DESC,(
             SELECT count(*) FROM terms
             WHERE c.bm25f_text_tsv @@ terms.query
                OR m.bm25f_meta_tsv @@ terms.query
           ) DESC,m.id,c.chunk_index
           LIMIT $3"#,
    )
    .bind(rooms)
    .bind(&terms)
    .bind(BM25F_MAX_CANDIDATES)
    .fetch_all(pool)
    .await?;
    if terms.len() == 1
        && rows
            .first()
            .and_then(|row| row.try_get::<i64, _>("total_matches").ok())
            .is_some_and(|total| total > BM25F_MAX_CANDIDATES)
    {
        warnings.push(format!(
            "BM25F single-term candidate pool truncated at {BM25F_MAX_CANDIDATES} documents"
        ));
    }

    let mut candidates = Vec::with_capacity(rows.len());
    for row in rows {
        let title: String = row.try_get("title")?;
        let heading_path: String = row.try_get("heading_path")?;
        let source_path: String = row.try_get("source_path")?;
        let threads: String = row.try_get("threads")?;
        let body: String = row.try_get("body")?;
        let memory_type: String = row.try_get("memory_type")?;
        let score = bm25f::score(
            &terms,
            document_count as u64,
            &document_frequency,
            &[
                Bm25fFieldValue {
                    name: "title",
                    text: &title,
                    average_length: averages.title,
                    config: BM25F_TITLE,
                },
                Bm25fFieldValue {
                    name: "heading",
                    text: &heading_path,
                    average_length: averages.heading,
                    config: BM25F_HEADING,
                },
                Bm25fFieldValue {
                    name: "source_path",
                    text: &source_path,
                    average_length: averages.source_path,
                    config: BM25F_SOURCE_PATH,
                },
                Bm25fFieldValue {
                    name: "threads",
                    text: &threads,
                    average_length: averages.threads,
                    config: BM25F_THREADS,
                },
                Bm25fFieldValue {
                    name: "body",
                    text: &body,
                    average_length: averages.body,
                    config: BM25F_BODY,
                },
                Bm25fFieldValue {
                    name: "type",
                    text: &memory_type,
                    average_length: averages.memory_type,
                    config: BM25F_MEMORY_TYPE,
                },
            ],
        );
        if score.value == 0.0 {
            continue;
        }
        let meta: serde_json::Value = row.try_get("meta")?;
        let (durability, temporal_weight) = if temporal_decay {
            giga_temporal_factor(&meta, decay_now)
        } else {
            (None, 1.0)
        };
        let missing_terms = terms
            .iter()
            .filter(|term| !score.matched_terms.contains(term))
            .cloned()
            .collect::<Vec<_>>();
        let coverage = score.matched_terms.len() as f64 / terms.len() as f64;
        candidates.push(serde_json::json!({
            "memory_id": row.try_get::<i64,_>("memory_id")?,
            "source_path": source_path,
            "title": title,
            "heading_path": heading_path,
            "body": bounded_excerpt(&body),
            "chunk_index": row.try_get::<i32,_>("chunk_index")?,
            "bm25f_score": score.value,
            "bm25f_fields": score.matched_fields,
            "durability": durability,
            "temporal_weight": temporal_weight,
            "matched_terms": score.matched_terms,
            "missing_terms": missing_terms,
            "term_coverage": coverage,
        }));
    }
    candidates.sort_by(|left, right| compare_weighted_lane(left, right, "bm25f_score"));
    let mut seen_memories = BTreeSet::new();
    candidates.retain(|candidate| {
        candidate["memory_id"]
            .as_i64()
            .is_some_and(|memory_id| seen_memories.insert(memory_id))
    });
    candidates.truncate(BM25F_TOP_K);
    Ok(candidates)
}

async fn load_bm25f_candidates(
    pool: &PgPool,
    rooms: &[String],
    query: &str,
    temporal_decay: bool,
    decay_now: DateTime<Utc>,
    warnings: &mut Vec<String>,
) -> Result<Vec<serde_json::Value>, AppError> {
    load_bm25f_candidates_for_terms(
        pool,
        rooms,
        &bm25f::query_terms(query),
        temporal_decay,
        decay_now,
        warnings,
    )
    .await
}

const SEMANTIC_VOCABULARY_TOP_K: i64 = 3;
// Terse concept vectors score lower than memory passages: measured paraphrases
// land at 0.30-0.37 while direct domain phrases exceed 0.45.
const SEMANTIC_VOCABULARY_MIN_SIMILARITY: f64 = 0.30;
const SEMANTIC_VOCABULARY_MAX_TERMS: usize = 12;

fn semantic_vocabulary_terms(concepts: &[serde_json::Value]) -> Vec<String> {
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

async fn load_semantic_vocabulary_concepts(
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
    let url = cfg.embed_url.as_deref().ok_or_else(|| {
        AppError::Config("semantic vocabulary refresh requires SOLARISAEL_EMBED_URL".into())
    })?;
    if cfg.test_embedding_disabled {
        return Err(AppError::Config(
            "semantic vocabulary refresh is unavailable while embedding is disabled".into(),
        ));
    }
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

async fn load_thread_neighbors(
    pool: &PgPool,
    memory_ids: &[i64],
) -> Result<BTreeMap<i64, Vec<serde_json::Value>>, AppError> {
    if memory_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = sqlx::query(
        r#"WITH neighbor_edges AS (
             SELECT current_event.memory_id AS candidate_memory_id,
                    t.thread_key,
                    'previous'::text AS direction,
                    0 AS direction_order,
                    neighbor_event.id AS neighbor_event_id,
                    neighbor.id,coalesce(neighbor.title,'') AS title,
                    neighbor.source_path,left(coalesce(neighbor.body,''),1200) AS body,
                    CASE
                      WHEN neighbor.archived_at IS NULL AND neighbor.superseded_by IS NULL
                      THEN 'active' ELSE 'historical'
                    END AS authority_state,
                    neighbor.superseded_by
             FROM thread_events current_event
             JOIN threads t ON t.id=current_event.thread_id
             JOIN thread_event_links link
               ON link.thread_id=current_event.thread_id
              AND link.next_event_id=current_event.id
             JOIN thread_events neighbor_event
               ON neighbor_event.thread_id=link.thread_id
              AND neighbor_event.id=link.previous_event_id
             JOIN memories neighbor ON neighbor.id=neighbor_event.memory_id
             WHERE current_event.memory_id = ANY($1::bigint[])
             UNION ALL
             SELECT current_event.memory_id AS candidate_memory_id,
                    t.thread_key,
                    'next'::text AS direction,
                    1 AS direction_order,
                    neighbor_event.id AS neighbor_event_id,
                    neighbor.id,coalesce(neighbor.title,'') AS title,
                    neighbor.source_path,left(coalesce(neighbor.body,''),1200) AS body,
                    CASE
                      WHEN neighbor.archived_at IS NULL AND neighbor.superseded_by IS NULL
                      THEN 'active' ELSE 'historical'
                    END AS authority_state,
                    neighbor.superseded_by
             FROM thread_events current_event
             JOIN threads t ON t.id=current_event.thread_id
             JOIN thread_event_links link
               ON link.thread_id=current_event.thread_id
              AND link.previous_event_id=current_event.id
             JOIN thread_events neighbor_event
               ON neighbor_event.thread_id=link.thread_id
              AND neighbor_event.id=link.next_event_id
             JOIN memories neighbor ON neighbor.id=neighbor_event.memory_id
             WHERE current_event.memory_id = ANY($1::bigint[])
           ), ranked AS (
             SELECT *,
                    row_number() OVER (
                      PARTITION BY candidate_memory_id
                      ORDER BY thread_key,direction_order,neighbor_event_id,id
                    ) AS ordinal
             FROM neighbor_edges
           )
           SELECT candidate_memory_id,thread_key,direction,id,title,source_path,body,
                  authority_state,superseded_by
           FROM ranked WHERE ordinal <= 6
           ORDER BY candidate_memory_id,ordinal"#,
    )
    .bind(memory_ids)
    .fetch_all(pool)
    .await?;
    let mut by_memory = BTreeMap::new();
    for row in rows {
        let candidate_memory_id: i64 = row.try_get("candidate_memory_id")?;
        let body: String = row.try_get("body")?;
        by_memory
            .entry(candidate_memory_id)
            .or_insert_with(Vec::new)
            .push(serde_json::json!({
                "thread": row.try_get::<String, _>("thread_key")?,
                "direction": row.try_get::<String, _>("direction")?,
                "id": row.try_get::<i64, _>("id")?,
                "title": row.try_get::<String, _>("title")?,
                "source_path": row.try_get::<String, _>("source_path")?,
                "excerpt": bounded_excerpt(&body),
                "authority_state": row.try_get::<String, _>("authority_state")?,
                "superseded_by": row.try_get::<Option<i64>, _>("superseded_by")?,
            }));
    }
    Ok(by_memory)
}

pub async fn recall(
    pool: &PgPool,
    cfg: &Config,
    params: RecallParams,
) -> Result<RecallResult, AppError> {
    params.validate()?;
    let query_dates = query_dates(&params.query);
    let query_terms = query_terms(&params.query);
    let content_patterns = query_terms
        .iter()
        .map(|term| format!("%{term}%"))
        .collect::<Vec<_>>();
    let rooms = vec![params.room.clone(), "house".to_string()];
    let mut warnings = Vec::new();
    let vector_text = match (cfg.test_embedding_disabled, cfg.embed_url.as_deref()) {
        (true, _) => {
            warnings.push("semantic lane absent: embedding disabled".to_string());
            None
        }
        (false, Some(url)) => match embed_query(
            &HTTP_CLIENT,
            url,
            &cfg.embed_model,
            &params.query,
            EMBED_DIMENSION,
        )
        .await
        {
            Ok(vector) => Some(format!(
                "[{}]",
                vector
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )),
            Err(e) => {
                warnings.push(format!("semantic lane absent: {e}"));
                None
            }
        },
        (false, None) => {
            warnings.push("semantic lane absent: embedding endpoint is required".to_string());
            None
        }
    };
    let decay_now = Utc::now();
    let bm25f_candidates = load_bm25f_candidates(
        pool,
        &rooms,
        &params.query,
        params.temporal_decay,
        decay_now,
        &mut warnings,
    )
    .await?;
    let semantic_vocabulary_concepts = match vector_text.as_deref() {
        Some(vector) => match load_semantic_vocabulary_concepts(pool, &rooms, vector, cfg).await {
            Ok(concepts) => concepts,
            Err(error) => {
                // A missing/unmigrated/stale vocabulary must never impair exact recall.
                warnings.push(format!("semantic lexical bridge absent: {error}"));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    let semantic_vocabulary_terms = semantic_vocabulary_terms(&semantic_vocabulary_concepts);
    let semantic_lexical_candidates = load_bm25f_candidates_for_terms(
        pool,
        &rooms,
        &semantic_vocabulary_terms,
        params.temporal_decay,
        decay_now,
        &mut warnings,
    )
    .await?;
    let semantic_fetch_limit = (!params.temporal_decay).then_some(i64::from(params.semantic_top_k));
    let content_fetch_limit = (!params.temporal_decay).then_some(i64::from(params.content_top_k));
    let mut semantic_chunks = Vec::new();
    if let Some(vector_text) = vector_text.clone() {
        let semantic_rows = sqlx::query(
            r#"SELECT memory_id,source_path,title,heading_path,body,char_start,char_end,chunk_index,meta,sim
               FROM (
                 SELECT m.source_path,
                        m.id AS memory_id,
                        coalesce(m.title,'') AS title,
                        coalesce(c.heading_path,'') AS heading_path,
                        c.body,c.char_start,c.char_end,c.chunk_index,m.meta AS meta,
                        (1-(c.body_embedding <=> $1::vector))::double precision AS sim
                 FROM memory_chunks c
                 JOIN memories m ON m.id=c.memory_id
                 WHERE m.room = ANY($2::text[])
                   AND m.archived_at IS NULL
                   AND m.superseded_by IS NULL
                   AND COALESCE(m.type,'') <> 'paper-boat'
                   AND c.body_embedding IS NOT NULL
               ) ranked
               WHERE sim >= $3
               ORDER BY sim DESC,source_path,chunk_index
               LIMIT $4"#,
        )
        .bind(&vector_text)
        .bind(&rooms)
        .bind(params.semantic_min_similarity)
        .bind(semantic_fetch_limit)
        .fetch_all(pool)
        .await?;
        for row in semantic_rows {
            let sim: f64 = row.try_get("sim")?;
            if sim < params.semantic_min_similarity {
                continue;
            }
            let source_path: String = row.try_get("source_path")?;
            let title: Option<String> = row.try_get("title")?;
            let heading_path: Option<String> = row.try_get("heading_path")?;
            let body: String = row.try_get("body")?;
            let meta: serde_json::Value = row.try_get("meta")?;
            let (durability, temporal_weight) = if params.temporal_decay {
                giga_temporal_factor(&meta, decay_now)
            } else {
                (None, 1.0)
            };
            let (matched_terms, missing_terms, coverage) = candidate_terms(
                &query_terms,
                &[
                    &source_path,
                    title.as_deref().unwrap_or(""),
                    heading_path.as_deref().unwrap_or(""),
                    &body,
                ],
            );
            semantic_chunks.push(serde_json::json!({"memory_id":row.try_get::<i64,_>("memory_id")?,"source_path":source_path,"title":title,"heading_path":heading_path,"body":bounded_excerpt(&body),"char_start":row.try_get::<i32,_>("char_start")?,"char_end":row.try_get::<i32,_>("char_end")?,"chunk_index":row.try_get::<i32,_>("chunk_index")?,"sim":sim,"durability":durability,"temporal_weight":temporal_weight,"matched_terms":matched_terms,"missing_terms":missing_terms,"term_coverage":coverage,"evidence":"semantic cosine similarity"}));
        }
        if semantic_chunks.is_empty() {
            // The floor query returned nothing; a second cheap roundtrip is
            // acceptable only in this empty case so silence is impossible.
            let top_sim: Option<f64> = sqlx::query_scalar(
                r#"SELECT MAX((1-(c.body_embedding <=> $1::vector))::double precision)
                   FROM memory_chunks c
                   JOIN memories m ON m.id=c.memory_id
                   WHERE m.room = ANY($2::text[])
                     AND m.archived_at IS NULL
                     AND m.superseded_by IS NULL
                     AND COALESCE(m.type,'') <> 'paper-boat'
                     AND c.body_embedding IS NOT NULL"#,
            )
            .bind(&vector_text)
            .bind(&rooms)
            .fetch_one(pool)
            .await?;
            warnings.push(match top_sim {
                Some(top) => format!(
                    "semantic lane empty: top sim {top:.2} < floor {:.2} (rooms {})",
                    params.semantic_min_similarity,
                    rooms.join(",")
                ),
                None => format!(
                    "semantic lane empty: no embedded chunks (rooms {})",
                    rooms.join(",")
                ),
            });
        }
    }
    let content_rows = sqlx::query(
        "SELECT m.id AS memory_id,m.source_path,coalesce(m.title,'') AS title,
                coalesce(c.heading_path,'') AS heading_path,c.body,c.char_start,c.char_end,
                c.chunk_index,m.meta AS meta,word_similarity($1,c.body)::double precision AS sim
         FROM memory_chunks c
         JOIN memories m ON m.id=c.memory_id
         WHERE m.room = ANY($2::text[])
           AND m.archived_at IS NULL
           AND m.superseded_by IS NULL
           AND COALESCE(m.type,'') <> 'paper-boat'
           AND ($5::text[] = '{}'::text[] OR c.body ILIKE ANY($5::text[]))
           AND word_similarity($1,c.body) >= $3
         ORDER BY sim DESC,m.source_path,c.chunk_index
         LIMIT $4",
    )
    .bind(&params.query)
    .bind(&rooms)
    .bind(params.content_min_similarity)
    .bind(content_fetch_limit)
    .bind(&content_patterns)
    .fetch_all(pool)
    .await?;
    let mut content_chunks = Vec::new();
    for row in content_rows {
        let sim: f64 = row.try_get("sim")?;
        let source_path: String = row.try_get("source_path")?;
        let title: Option<String> = row.try_get("title")?;
        let heading_path: Option<String> = row.try_get("heading_path")?;
        let body: String = row.try_get("body")?;
        let meta: serde_json::Value = row.try_get("meta")?;
        let (durability, temporal_weight) = if params.temporal_decay {
            giga_temporal_factor(&meta, decay_now)
        } else {
            (None, 1.0)
        };
        let (matched_terms, missing_terms, coverage) = candidate_terms(
            &query_terms,
            &[
                &source_path,
                title.as_deref().unwrap_or(""),
                heading_path.as_deref().unwrap_or(""),
                &body,
            ],
        );
        content_chunks.push(serde_json::json!({"memory_id":row.try_get::<i64,_>("memory_id")?,"source_path":source_path,"title":title,"heading_path":heading_path,"body":bounded_excerpt(&body),"char_start":row.try_get::<i32,_>("char_start")?,"char_end":row.try_get::<i32,_>("char_end")?,"chunk_index":row.try_get::<i32,_>("chunk_index")?,"ws":sim,"durability":durability,"temporal_weight":temporal_weight,"matched_terms":matched_terms,"missing_terms":missing_terms,"term_coverage":coverage,"evidence":"lexical word_similarity"}));
    }
    if params.temporal_decay {
        semantic_chunks.sort_by(|left, right| compare_weighted_lane(left, right, "sim"));
        content_chunks.sort_by(|left, right| compare_weighted_lane(left, right, "ws"));
    }
    semantic_chunks.truncate(params.semantic_top_k as usize);
    content_chunks.truncate(params.content_top_k as usize);
    let mut date_matches = Vec::new();
    if !query_dates.is_empty() {
        let rows = sqlx::query(
            "SELECT source_path,title,body,date,dates
             FROM memories
             WHERE room = ANY($1::text[])
               AND archived_at IS NULL
               AND superseded_by IS NULL
               AND COALESCE(type,'') <> 'paper-boat'
               AND dates && $2::date[]
             ORDER BY source_path
             LIMIT 5",
        )
        .bind(&rooms)
        .bind(&query_dates)
        .fetch_all(pool)
        .await?;
        for row in rows {
            let source_path: String = row.try_get("source_path")?;
            let title: Option<String> = row.try_get("title")?;
            let body: String = row.try_get("body")?;
            let dates: Vec<NaiveDate> = row.try_get("dates")?;
            let (matched_terms, missing_terms, coverage) = candidate_terms(
                &query_terms,
                &[&source_path, title.as_deref().unwrap_or(""), &body],
            );
            date_matches.push(serde_json::json!({"source_path":source_path,"title":title,"body_excerpt":bounded_excerpt(&body),"excerpt":bounded_excerpt(&body),"date":row.try_get::<Option<NaiveDate>,_>("date")?.map(|d|d.to_string()),"dates":dates.into_iter().map(|d|d.to_string()).collect::<Vec<_>>(),"score":1.0,"reason":"date match","matched_terms":matched_terms,"missing_terms":missing_terms,"term_coverage":coverage}));
        }
    }
    let thread_rows = sqlx::query(
        "SELECT DISTINCT ON (m.id) m.id AS memory_id,m.source_path,
                coalesce(m.title,'') AS title,t.thread_key,left(m.body,1200) AS body,
                GREATEST(similarity(t.thread_key,$2),
                         similarity(coalesce(ref.context,''),$2))::double precision AS rank
         FROM memory_thread_refs ref
         JOIN thread_events event ON event.id=ref.event_id
         JOIN threads t ON t.id=event.thread_id
         JOIN memories m ON m.id=event.memory_id
         WHERE m.room = ANY($1::text[]) AND m.archived_at IS NULL
           AND m.superseded_by IS NULL
           AND COALESCE(m.type,'') <> 'paper-boat'
           AND (t.thread_key ILIKE ANY($3::text[])
                OR ref.context ILIKE ANY($3::text[])
                OR m.source_path ILIKE ANY($3::text[]))
         ORDER BY m.id,rank DESC,t.thread_key
         LIMIT 8",
    )
    .bind(&rooms)
    .bind(&params.query)
    .bind(&content_patterns)
    .fetch_all(pool)
    .await?;
    let mut fused: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (rank, c) in semantic_chunks.iter().enumerate() {
        let key = format!(
            "{}#{}",
            c["memory_id"].as_i64().unwrap_or(0),
            c["chunk_index"].as_i64().unwrap_or(0)
        );
        let score = (c["sim"].as_f64().unwrap_or(0.0) * 0.6 + 1.0 / (rank as f64 + 1.0) * 0.4)
            * c["temporal_weight"].as_f64().unwrap_or(1.0);
        let mut reasons = vec!["semantic cosine similarity"];
        if c["temporal_weight"].as_f64().unwrap_or(1.0) < 1.0 {
            reasons.push("durability-shaped temporal decay");
        }
        fused.insert(key, serde_json::json!({"memory_id":c["memory_id"],"source_path":c["source_path"],"title":c["title"],"heading_path":c["heading_path"],"excerpt":c["body"],"sources":[c["source_path"]],"term_coverage":c["term_coverage"],"matched_terms":c["matched_terms"],"missing_terms":c["missing_terms"],"score":score,"semantic_score":c["sim"],"durability":c["durability"],"temporal_weight":c["temporal_weight"],"reasons":reasons,"source":"semantic","chunk_index":c["chunk_index"]}));
    }
    for (rank, c) in content_chunks.iter().enumerate() {
        let key = format!(
            "{}#{}",
            c["memory_id"].as_i64().unwrap_or(0),
            c["chunk_index"].as_i64().unwrap_or(0)
        );
        let score = (c["ws"].as_f64().unwrap_or(0.0) * 0.6 + 1.0 / (rank as f64 + 1.0) * 0.4)
            * c["temporal_weight"].as_f64().unwrap_or(1.0);
        let decayed = c["temporal_weight"].as_f64().unwrap_or(1.0) < 1.0;
        let decay_reason = if decayed {
            vec!["durability-shaped temporal decay"]
        } else {
            Vec::new()
        };
        if let Some(existing) = fused.get_mut(&key) {
            existing["score"] =
                serde_json::json!(existing["score"].as_f64().unwrap_or(0.0) + score);
            existing["content_score"] = c["ws"].clone();
            existing["source"] = serde_json::json!("semantic+content");
            existing["durability"] = c["durability"].clone();
            existing["temporal_weight"] = c["temporal_weight"].clone();
            let mut reasons = vec!["semantic cosine similarity", "lexical word_similarity"];
            if decayed {
                reasons.push("durability-shaped temporal decay");
            }
            existing["reasons"] = serde_json::json!(reasons);
        } else {
            fused.insert(key, serde_json::json!({"memory_id":c["memory_id"],"source_path":c["source_path"],"title":c["title"],"heading_path":c["heading_path"],"excerpt":c["body"],"sources":[c["source_path"]],"term_coverage":c["term_coverage"],"matched_terms":c["matched_terms"],"missing_terms":c["missing_terms"],"score":score,"content_score":c["ws"],"durability":c["durability"],"temporal_weight":c["temporal_weight"],"reasons":if decay_reason.is_empty() { serde_json::json!(["lexical word_similarity"]) } else { serde_json::json!(["lexical word_similarity","durability-shaped temporal decay"]) },"source":"content","chunk_index":c["chunk_index"]}));
        }
    }
    let max_bm25f_score = bm25f_candidates
        .first()
        .map(|candidate| weighted_lane_score(candidate, "bm25f_score"))
        .unwrap_or(1.0)
        .max(f64::EPSILON);
    for (rank, candidate) in bm25f_candidates.iter().enumerate() {
        let memory_id = candidate["memory_id"].as_i64().unwrap_or_default();
        let chunk_index = candidate["chunk_index"].as_i64().unwrap_or_default();
        let exact_key = format!("{memory_id}#{chunk_index}");
        let existing_key = if fused.contains_key(&exact_key) {
            Some(exact_key.clone())
        } else {
            fused
                .iter()
                .find(|(_, entry)| entry["memory_id"].as_i64() == Some(memory_id))
                .map(|(key, _)| key.clone())
        };
        let normalized = weighted_lane_score(candidate, "bm25f_score") / max_bm25f_score;
        let lane_score = normalized * 0.6 + 1.0 / (rank as f64 + 1.0) * 0.4;
        if let Some(existing_key) = existing_key {
            let existing = fused
                .get_mut(&existing_key)
                .expect("selected BM25F fusion key must exist");
            existing["score"] =
                serde_json::json!(existing["score"].as_f64().unwrap_or_default() + lane_score);
            existing["bm25f_score"] = candidate["bm25f_score"].clone();
            existing["bm25f_fields"] = candidate["bm25f_fields"].clone();
            let source = existing["source"].as_str().unwrap_or("candidate");
            if !source.split('+').any(|part| part == "bm25f") {
                existing["source"] = serde_json::json!(format!("{source}+bm25f"));
            }
            if let Some(reasons) = existing["reasons"].as_array_mut()
                && !reasons
                    .iter()
                    .any(|reason| reason.as_str() == Some("BM25F field-aware lexical score"))
            {
                reasons.push(serde_json::json!("BM25F field-aware lexical score"));
            }
            continue;
        }
        let decayed = candidate["temporal_weight"].as_f64().unwrap_or(1.0) < 1.0;
        let reasons = if decayed {
            serde_json::json!([
                "BM25F field-aware lexical score",
                "durability-shaped temporal decay"
            ])
        } else {
            serde_json::json!(["BM25F field-aware lexical score"])
        };
        fused.insert(
            exact_key,
            serde_json::json!({
                "memory_id": candidate["memory_id"],
                "source_path": candidate["source_path"],
                "title": candidate["title"],
                "heading_path": candidate["heading_path"],
                "excerpt": candidate["body"],
                "sources": [candidate["source_path"].clone()],
                "term_coverage": candidate["term_coverage"],
                "matched_terms": candidate["matched_terms"],
                "missing_terms": candidate["missing_terms"],
                "score": lane_score,
                "bm25f_score": candidate["bm25f_score"],
                "bm25f_fields": candidate["bm25f_fields"],
                "durability": candidate["durability"],
                "temporal_weight": candidate["temporal_weight"],
                "reasons": reasons,
                "source": "bm25f",
                "chunk_index": candidate["chunk_index"],
            }),
        );
    }
    let max_semantic_lexical_score = semantic_lexical_candidates
        .first()
        .map(|candidate| weighted_lane_score(candidate, "bm25f_score"))
        .unwrap_or(1.0)
        .max(f64::EPSILON);
    for (rank, candidate) in semantic_lexical_candidates.iter().enumerate() {
        let memory_id = candidate["memory_id"].as_i64().unwrap_or_default();
        // Exact BM25F has already been fused. It always owns a matching memory;
        // this lane can only introduce otherwise-unseen candidates.
        if fused
            .values()
            .any(|entry| entry["memory_id"].as_i64() == Some(memory_id))
        {
            continue;
        }
        let normalized = weighted_lane_score(candidate, "bm25f_score") / max_semantic_lexical_score;
        let lane_score = normalized * 0.15 + 1.0 / (rank as f64 + 1.0) * 0.05;
        let chunk_index = candidate["chunk_index"].as_i64().unwrap_or_default();
        fused.insert(
            format!("{memory_id}#{chunk_index}"),
            serde_json::json!({
                "memory_id": candidate["memory_id"],
                "source_path": candidate["source_path"],
                "title": candidate["title"],
                "heading_path": candidate["heading_path"],
                "excerpt": candidate["body"],
                "sources": [candidate["source_path"].clone()],
                "term_coverage": candidate["term_coverage"],
                "matched_terms": candidate["matched_terms"],
                "missing_terms": candidate["missing_terms"],
                "score": lane_score,
                "semantic_lexical_score": candidate["bm25f_score"],
                "semantic_lexical_fields": candidate["bm25f_fields"],
                "semantic_lexical_concepts": &semantic_vocabulary_concepts,
                "durability": candidate["durability"],
                "temporal_weight": candidate["temporal_weight"],
                "reasons": ["semantic vocabulary expansion BM25F score"],
                "source": "semantic_lexical_bm25f",
                "chunk_index": candidate["chunk_index"],
            }),
        );
    }
    for row in &thread_rows {
        let memory_id: i64 = row.try_get("memory_id")?;
        let source_path: String = row.try_get("source_path")?;
        let thread_key: String = row.try_get("thread_key")?;
        let title: String = row.try_get("title")?;
        let body: String = row.try_get("body")?;
        let rank: f64 = row.try_get("rank")?;
        // A named thread is a deliberate authoring act, so it carries weight even when
        // trigram similarity is low; the floor keeps a real key from scoring as noise.
        let score = 0.35 + rank * 0.55;
        if let Some(existing) = fused
            .values_mut()
            .find(|entry| entry["memory_id"].as_i64() == Some(memory_id))
        {
            existing["score"] =
                serde_json::json!(existing["score"].as_f64().unwrap_or(0.0) + score);
            existing["thread_key"] = serde_json::json!(thread_key);
            if let Some(reasons) = existing["reasons"].as_array_mut() {
                reasons.push(serde_json::json!("lexical thread key"));
            }
            continue;
        }
        let (matched_terms, missing_terms, coverage) =
            candidate_terms(&query_terms, &[&source_path, &title, &thread_key, &body]);
        fused.insert(
            format!("{memory_id}#thread"),
            serde_json::json!({"memory_id":memory_id,"source_path":source_path.clone(),"title":title,"heading_path":"","excerpt":bounded_excerpt(&body),"sources":[source_path],"term_coverage":coverage,"matched_terms":matched_terms,"missing_terms":missing_terms,"score":score,"thread_key":thread_key,"reasons":["lexical thread key"],"source":"thread","chunk_index":0}),
        );
    }
    let mut retrieval_candidates: Vec<_> = fused.into_values().collect();
    retrieval_candidates.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["score"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a["source_path"].as_str().cmp(&b["source_path"].as_str()))
            .then_with(|| a["chunk_index"].as_i64().cmp(&b["chunk_index"].as_i64()))
            .then_with(|| a["memory_id"].as_i64().cmp(&b["memory_id"].as_i64()))
    });
    retrieval_candidates.truncate(params.semantic_top_k.max(params.content_top_k) as usize);
    let memory_ids = retrieval_candidates
        .iter()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut neighbors = load_thread_neighbors(pool, &memory_ids).await?;
    for candidate in &mut retrieval_candidates {
        let memory_id = candidate["memory_id"].as_i64().unwrap_or_default();
        candidate["thread_neighbors"] =
            serde_json::Value::Array(neighbors.remove(&memory_id).unwrap_or_default());
    }
    // Canon lookup matches three ways, ranked. Tokens alone can never match a
    // multi-word name or a hyphenated alias, which is how 42 of 109 rows went
    // dark; widening to ILIKE/tsvector then lets fuzzy hits evict the row the
    // caller literally named. So: whole-phrase exact, then token exact, then
    // similarity — and only the last tier competes for leftover LIMIT slots.
    let query_phrase = params.query.trim().to_lowercase();
    let canon_rows = sqlx::query("SELECT name,kind,summary,aliases,weighty,pointer_files, (CASE WHEN lower(name) = $5 OR EXISTS (SELECT 1 FROM unnest(aliases) a1 WHERE lower(a1) = $5) THEN 0 WHEN lower(name) = ANY($2::text[]) OR EXISTS (SELECT 1 FROM unnest(aliases) a2 WHERE lower(a2) = ANY($2::text[])) THEN 1 ELSE 2 END) AS exactness FROM named_entities, websearch_to_tsquery('portuguese', $3) AS tsq WHERE room = ANY($1::text[]) AND authority = 'active' AND (lower(name) = $5 OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias) = $5) OR lower(name) = ANY($2::text[]) OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias) = ANY($2::text[])) OR name ILIKE ANY($4::text[]) OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE alias ILIKE ANY($4::text[])) OR summary_tsv @@ tsq) ORDER BY exactness, weighty DESC, name LIMIT 12")
        .bind(&rooms).bind(&query_terms).bind(&params.query).bind(&content_patterns).bind(&query_phrase).fetch_all(pool).await?;
    let mut canon_matches = Vec::new();
    let mut named_entities = Vec::new();
    for row in canon_rows {
        let name: String = row.try_get("name")?;
        let kind: String = row.try_get("kind")?;
        let summary: String = row.try_get("summary")?;
        let aliases: Vec<String> = row.try_get("aliases")?;
        let weighty: bool = row.try_get("weighty")?;
        let files: serde_json::Value = row.try_get("pointer_files")?;
        canon_matches.push(serde_json::json!({"termKey":name,"entry":{"type":kind,"summary":bounded_excerpt(&summary),"aliases":aliases,"weighty":weighty,"files":files}}));
        named_entities.push(name);
    }
    let memory_types: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT type
         FROM memories
         WHERE room = ANY($1::text[])
           AND archived_at IS NULL
           AND superseded_by IS NULL
           AND COALESCE(type,'') <> 'paper-boat'
         ORDER BY type
         LIMIT 20",
    )
    .bind(&rooms)
    .fetch_all(pool)
    .await?;
    let thread_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT t.thread_key
         FROM threads t
         JOIN thread_events event ON event.thread_id=t.id
         JOIN memories m ON m.id=event.memory_id
         WHERE m.room = ANY($1::text[])
           AND m.archived_at IS NULL
           AND m.superseded_by IS NULL
           AND COALESCE(m.type,'') <> 'paper-boat'
         ORDER BY t.thread_key
         LIMIT 20",
    )
    .bind(&rooms)
    .fetch_all(pool)
    .await?;
    let taxonomy = serde_json::json!({"rooms":rooms,"memoryTypes":memory_types,"threadKeys":thread_keys,"namedEntities":named_entities});
    let cluster_staleness = cluster_staleness(pool, None)
        .await
        .ok()
        .and_then(|s| serde_json::to_value(s).ok());
    let cluster_resonance = if let Some(v) = vector_text.as_deref() {
        cluster_resonance(pool, v, &rooms).await.ok()
    } else {
        None
    };
    Ok(RecallResult {
        ok: true,
        query: params.query,
        found: !retrieval_candidates.is_empty()
            || !canon_matches.is_empty()
            || !date_matches.is_empty(),
        source: "rust-postgres",
        warnings,
        retrieval_candidates,
        canon_matches,
        semantic_chunks,
        content_chunks,
        date_matches,
        query_dates: query_dates
            .into_iter()
            .map(|d| serde_json::json!(d.to_string()))
            .collect(),
        taxonomy,
        cluster_staleness,
        cluster_resonance,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        SEMANTIC_VOCABULARY_MAX_TERMS, SEMANTIC_VOCABULARY_TOP_K, giga_temporal_factor,
        semantic_vocabulary_terms,
    };
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::{Value, json};

    fn giga_meta(durability: Value, anchor: &str) -> Value {
        json!({
            "origin": "giga-promotion",
            "giga": {
                "durability": durability,
                "decay_anchor": "candidate_created_at",
                "decay_anchor_at": anchor,
            },
        })
    }

    #[test]
    fn temporal_factor_treats_unsafe_metadata_as_legacy_weight() {
        let now = Utc.with_ymd_and_hms(2030, 2, 1, 0, 0, 0).single().unwrap();
        let anchor = (now - Duration::days(7)).to_rfc3339();
        let cases = [
            json!({}),
            giga_meta(json!("not-a-number"), &anchor),
            json!({
                "origin": "manual",
                "giga": {
                    "durability": 0.0,
                    "decay_anchor": "candidate_created_at",
                    "decay_anchor_at": anchor,
                },
            }),
        ];

        for meta in cases {
            assert_eq!(giga_temporal_factor(&meta, now), (None, 1.0));
        }
    }

    #[test]
    fn temporal_factor_follows_the_durability_shaped_half_life() {
        let now = Utc.with_ymd_and_hms(2030, 2, 1, 0, 0, 0).single().unwrap();
        let seven_days_ago = (now - Duration::days(7)).to_rfc3339();
        let twenty_eight_days_ago = (now - Duration::days(28)).to_rfc3339();

        let (durability_zero, factor_zero) =
            giga_temporal_factor(&giga_meta(json!(0.0), &seven_days_ago), now);
        assert_eq!(durability_zero, Some(0.0));
        assert!((factor_zero - 0.5).abs() < 1e-12);

        let (durability_half, factor_half) =
            giga_temporal_factor(&giga_meta(json!(0.5), &twenty_eight_days_ago), now);
        assert_eq!(durability_half, Some(0.5));
        assert!((factor_half - 0.5).abs() < 1e-12);
    }

    #[test]
    fn temporal_factor_never_decays_permanent_or_future_memories() {
        let now = Utc.with_ymd_and_hms(2030, 2, 1, 0, 0, 0).single().unwrap();
        let old_anchor = (now - Duration::days(365)).to_rfc3339();
        let future_anchor = (now + Duration::days(1)).to_rfc3339();

        assert_eq!(
            giga_temporal_factor(&giga_meta(json!(1.0), &old_anchor), now),
            (Some(1.0), 1.0)
        );
        assert_eq!(
            giga_temporal_factor(&giga_meta(json!(0.0), &future_anchor), now),
            (Some(0.0), 1.0)
        );
    }
    #[test]
    fn semantic_vocabulary_terms_are_deduplicated_and_hard_capped() {
        assert_eq!(SEMANTIC_VOCABULARY_TOP_K, 3);
        let concepts = (0..4)
            .map(|concept| {
                json!({
                    "concept": format!("concept-{concept}"),
                    "terms": (0..4).map(|term| format!("term-{concept}-{term}")).collect::<Vec<_>>(),
                    "source_kind": "named_entity",
                    "similarity": 0.5,
                })
            })
            .collect::<Vec<_>>();
        let terms = semantic_vocabulary_terms(&concepts);
        assert_eq!(terms.len(), SEMANTIC_VOCABULARY_MAX_TERMS);
        assert_eq!(terms.first().map(String::as_str), Some("term-0-0"));
        assert_eq!(terms.last().map(String::as_str), Some("term-2-3"));
    }
}
