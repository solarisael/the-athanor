mod bm25f_candidates;
mod embedding;
mod pointer_files;
mod semantic_vocabulary;
mod temporal;
mod thread_neighbors;

pub use semantic_vocabulary::refresh_semantic_vocabulary;

use crate::cluster::{cluster_resonance, cluster_staleness};
use crate::config::{
    AppError, Config, EMBED_DIMENSION, EmbeddingMode, HTTP_CLIENT, QUERY_DATE_RE, ROOM_KEY_RE,
};
use crate::settings::RoomSettings;
use bm25f_candidates::{load_bm25f_candidates, load_bm25f_candidates_for_terms};
use chrono::{NaiveDate, Utc};
use embedding::embed_query;
use pointer_files::protocol_pointer_files;
use semantic_vocabulary::{load_semantic_vocabulary_concepts, semantic_vocabulary_terms};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use temporal::{compare_weighted_lane, giga_temporal_factor, weighted_lane_score};
use thread_neighbors::load_thread_neighbors;

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
// enough: this floor is calibrated to the lexical matcher; re-measure the
// corpus before raising it so configuration cannot manufacture false certainty.
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

pub async fn recall(
    pool: &PgPool,
    cfg: &Config,
    params: RecallParams,
) -> Result<RecallResult, AppError> {
    params.validate()?;
    let settings = RoomSettings::load(pool, &params.room).await?;
    let query_dates = query_dates(&params.query);
    let query_terms = query_terms(&params.query);
    let content_patterns = query_terms
        .iter()
        .map(|term| format!("%{term}%"))
        .collect::<Vec<_>>();
    let rooms = vec![params.room.clone(), "house".to_string()];
    let mut warnings = Vec::new();
    let vector_text = match (cfg.embedding_mode, cfg.embed_url.as_deref()) {
        (EmbeddingMode::Disabled, _) => {
            warnings.push("semantic lane absent: embedding disabled in production".to_string());
            None
        }
        (EmbeddingMode::DisabledForTest, _) => {
            warnings.push("semantic lane absent: embedding disabled for isolated test".to_string());
            None
        }
        (EmbeddingMode::Required, Some(url)) => match embed_query(
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
        (EmbeddingMode::Required, None) => {
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
        &settings,
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
        &settings,
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
                   AND COALESCE(m.type,'') <> $5
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
        .bind(origami::boats::MEMORY_KIND)
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
                giga_temporal_factor(&meta, decay_now, &settings)
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
                     AND COALESCE(m.type,'') <> $3
                     AND c.body_embedding IS NOT NULL"#,
            )
            .bind(&vector_text)
            .bind(&rooms)
            .bind(origami::boats::MEMORY_KIND)
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
           AND COALESCE(m.type,'') <> $6
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
    .bind(origami::boats::MEMORY_KIND)
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
            giga_temporal_factor(&meta, decay_now, &settings)
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
               AND COALESCE(type,'') <> $3
               AND dates && $2::date[]
             ORDER BY source_path
             LIMIT 5",
        )
        .bind(&rooms)
        .bind(&query_dates)
        .bind(origami::boats::MEMORY_KIND)
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
           AND COALESCE(m.type,'') <> $4
           AND (t.thread_key ILIKE ANY($3::text[])
                OR ref.context ILIKE ANY($3::text[])
                OR m.source_path ILIKE ANY($3::text[]))
         ORDER BY m.id,rank DESC,t.thread_key
         LIMIT 8",
    )
    .bind(&rooms)
    .bind(&params.query)
    .bind(&content_patterns)
    .bind(origami::boats::MEMORY_KIND)
    .fetch_all(pool)
    .await?;
    let mut fused: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (rank, c) in semantic_chunks.iter().enumerate() {
        let key = format!(
            "{}#{}",
            c["memory_id"].as_i64().unwrap_or(0),
            c["chunk_index"].as_i64().unwrap_or(0)
        );
        let score = (c["sim"].as_f64().unwrap_or(0.0) * settings.recall_semantic_similarity_weight
            + 1.0 / (rank as f64 + 1.0) * settings.recall_semantic_rank_weight)
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
        let score = (c["ws"].as_f64().unwrap_or(0.0) * settings.recall_content_similarity_weight
            + 1.0 / (rank as f64 + 1.0) * settings.recall_content_rank_weight)
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
        let lane_score = normalized * settings.recall_semantic_lexical_score_weight
            + 1.0 / (rank as f64 + 1.0) * settings.recall_semantic_lexical_rank_weight;
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
        let score = settings.recall_thread_base_weight + rank * settings.recall_thread_rank_weight;
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
    let canon_rows = sqlx::query("SELECT name,kind,summary,aliases,weighty,pointer_files, (CASE WHEN lower(name) = $5 OR EXISTS (SELECT 1 FROM unnest(aliases) a1 WHERE lower(a1) = $5) THEN 0 WHEN lower(name) = ANY($2::text[]) OR EXISTS (SELECT 1 FROM unnest(aliases) a2 WHERE lower(a2) = ANY($2::text[])) THEN 1 ELSE 2 END) AS exactness FROM named_entities, websearch_to_tsquery($6::regconfig, $3) AS tsq WHERE room = ANY($1::text[]) AND authority = 'active' AND (lower(name) = $5 OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias) = $5) OR lower(name) = ANY($2::text[]) OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias) = ANY($2::text[])) OR name ILIKE ANY($4::text[]) OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE alias ILIKE ANY($4::text[])) OR summary_tsv @@ tsq) ORDER BY exactness, weighty DESC, name LIMIT 12")
        .bind(&rooms).bind(&query_terms).bind(&params.query).bind(&content_patterns).bind(&query_phrase).bind(&settings.house_language).fetch_all(pool).await?;
    let mut canon_matches = Vec::new();
    let mut named_entities = Vec::new();
    for row in canon_rows {
        let name: String = row.try_get("name")?;
        let kind: String = row.try_get("kind")?;
        let summary: String = row.try_get("summary")?;
        let aliases: Vec<String> = row.try_get("aliases")?;
        let weighty: bool = row.try_get("weighty")?;
        let files: serde_json::Value = row.try_get("pointer_files")?;
        canon_matches.push(serde_json::json!({"termKey":name,"entry":{"type":kind,"summary":bounded_excerpt(&summary),"aliases":aliases,"weighty":weighty,"files":protocol_pointer_files(&files)}}));
        named_entities.push(name);
    }
    let memory_types: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT type
         FROM memories
         WHERE room = ANY($1::text[])
           AND archived_at IS NULL
           AND superseded_by IS NULL
           AND COALESCE(type,'') <> $2
         ORDER BY type
         LIMIT 20",
    )
    .bind(&rooms)
    .bind(origami::boats::MEMORY_KIND)
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
           AND COALESCE(m.type,'') <> $2
         ORDER BY t.thread_key
         LIMIT 20",
    )
    .bind(&rooms)
    .bind(origami::boats::MEMORY_KIND)
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
