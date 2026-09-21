mod bm25f_candidates;
mod content_lane;
mod embedding;
mod memory_reference;
mod pointer_files;
mod profile;
mod semantic_vocabulary;
mod temporal;
mod thread_neighbors;

pub use semantic_vocabulary::refresh_semantic_vocabulary;

use crate::bm25f;
use crate::cluster::{cluster_resonance, cluster_staleness};
use crate::config::{AppError, Config, EMBED_DIMENSION, EmbeddingMode, HTTP_CLIENT, QUERY_DATE_RE};
use crate::insula::OutcomeClass;
use crate::insula_writer::{EmitterSpan, end_span};
use crate::settings::RoomSettings;
use bm25f_candidates::{load_bm25f_candidates_for_terms, load_bm25f_candidates_for_terms_broad};
use chrono::{NaiveDate, Utc};
use content_lane::content_lane_rows;
use embedding::{EmbedError, embed_query};
use hearth::RecallRequest;
use memory_reference::{
    apply_manual_record_cap, hydrate_record_bodies, memory_references, resolve_memory_references,
};
use pointer_files::protocol_pointer_files;
use profile::{CanonOrder, apply_and_order, for_mode};
use semantic_vocabulary::{load_semantic_vocabulary_concepts, semantic_vocabulary_terms};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use temporal::{compare_weighted_lane, giga_temporal_factor, weighted_lane_score};
use thread_neighbors::load_thread_neighbors;

#[derive(Debug, Serialize)]
pub struct RecallResult {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: &'static str,
    /// `auto` or `manual`: which projection shaped `retrievalCandidates`.
    pub projection: &'static str,
    /// The ranking mode this recall resolved to. Echoed even while the profile
    /// gate is off, so telemetry can split on it.
    pub mode: &'static str,
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
    #[serde(rename = "rerankCandidates", skip_serializing_if = "Option::is_none")]
    pub rerank_candidates: Option<Vec<serde_json::Value>>,
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

const EXCERPT_MAX_CHARS: usize = 1200;

pub(crate) fn exceeds_excerpt(body: &str) -> bool {
    body.chars().count() > EXCERPT_MAX_CHARS
}

pub(crate) fn bounded_excerpt(body: &str) -> String {
    let excerpt: String = body.chars().take(EXCERPT_MAX_CHARS).collect();
    if exceeds_excerpt(body) {
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

/// Keep the reranking sidecar deliberately narrower than the regular recall
/// result. It receives only candidate cards with bounded excerpts; raw lane
/// arrays, record bodies, canon, taxonomy, and neighbors never enter it.
fn bounded_rerank_candidate(candidate: &serde_json::Value) -> Option<serde_json::Value> {
    let memory_id = candidate["memory_id"].as_i64()?;
    let excerpt = candidate
        .get("excerpt")
        .or_else(|| candidate.get("body"))
        .and_then(serde_json::Value::as_str)?;
    let mut projected = serde_json::Map::new();
    for key in [
        "memory_id",
        "source_path",
        "title",
        "heading_path",
        "sources",
        "term_coverage",
        "matched_terms",
        "missing_terms",
        "score",
        "semantic_score",
        "content_score",
        "bm25f_score",
        "bm25f_fields",
        "semantic_lexical_score",
        "semantic_lexical_fields",
        "durability",
        "temporal_weight",
        "reasons",
        "source",
        "chunk_index",
    ] {
        if let Some(value) = candidate.get(key) {
            projected.insert(key.to_owned(), value.clone());
        }
    }
    projected.insert("memory_id".to_owned(), serde_json::Value::from(memory_id));
    projected.insert(
        "excerpt".to_owned(),
        serde_json::Value::String(bounded_excerpt(excerpt)),
    );
    if !projected.contains_key("score") {
        let score = candidate
            .get("bm25f_score")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default();
        projected.insert("score".to_owned(), serde_json::Value::from(score));
    }
    Some(serde_json::Value::Object(projected))
}

fn should_build_rerank_candidates(projection: hearth::RecallProjection, top_k: u32) -> bool {
    projection.is_auto() && top_k > 0
}

fn build_rerank_candidates(
    baseline: &[serde_json::Value],
    broad: &[serde_json::Value],
    exact_memory_ids: &BTreeSet<i64>,
    top_k: usize,
) -> Vec<serde_json::Value> {
    let mut seen = BTreeSet::new();
    baseline
        .iter()
        .chain(broad)
        .filter_map(bounded_rerank_candidate)
        .filter(|candidate| {
            let Some(memory_id) = candidate["memory_id"].as_i64() else {
                return false;
            };
            !exact_memory_ids.contains(&memory_id) && seen.insert(memory_id)
        })
        .take(top_k)
        .collect()
}

/// One phase of a recall, observed as a child of the request's `recall`
/// span. An unfinished phase dropped by an early `?` exit ends as an error;
/// the parent span carries the exact error class.
struct Phase(Option<EmitterSpan>);

impl Phase {
    fn start(parent: Option<&EmitterSpan>, operation: &'static str) -> Self {
        Self(parent.and_then(|span| span.child(operation)))
    }

    fn bytes_in(&mut self, bytes: usize) {
        if let Some(span) = &mut self.0 {
            span.set_bytes_in(bytes);
        }
    }

    fn ok(mut self) {
        end_span(self.0.take(), OutcomeClass::Ok, None);
    }

    fn degraded(mut self, class: &'static str) {
        end_span(self.0.take(), OutcomeClass::Degraded, Some(class));
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        end_span(
            self.0.take(),
            OutcomeClass::Error,
            Some("recall.phase_aborted"),
        );
    }
}

pub async fn recall(
    pool: &PgPool,
    cfg: &Config,
    request: RecallRequest,
    span: Option<&EmitterSpan>,
) -> Result<RecallResult, AppError> {
    let room = request.room().as_str();
    let query = request.query();
    let temporal_decay = request.temporal_decay();
    let projection = request.projection();
    let mode = request.mode();
    let semantic_top_k = request.semantic_top_k();
    let content_top_k = request.content_top_k();
    let semantic_min_similarity = request.semantic_min_similarity();
    let content_min_similarity = request.content_min_similarity();
    let phase = Phase::start(span, "recall.settings");
    let settings = RoomSettings::load(pool, room).await?;
    phase.ok();
    let profile = for_mode(mode, settings.recall_mode_profiles_enabled);
    let rooms = vec![room.to_owned(), "house".to_owned()];
    let mut warnings = Vec::new();
    // An explicit memory reference is the cheapest cross-reference the House
    // has, so it is answered by primary key before any ranked lane runs; the
    // reference tokens then leave the ranked vocabulary so a resolved ID can
    // never be reported as a missing term.
    let phase = Phase::start(span, "recall.reference");
    let references = memory_references(query);
    let mut exact_candidates =
        resolve_memory_references(pool, &rooms, &references, projection, &mut warnings).await?;
    phase.ok();
    // A request that names references and nothing else (`memories 4520, 4516
    // and 999`) is a primary-key read: the resolved records in reference
    // order, the missing/refused warnings, their thread neighbors, and no
    // ranked lane. The words `memories` and `and` are not evidence of
    // anything, so they must never rank unrelated filler under the answer.
    if references.only_references(query) {
        if projection.is_manual() {
            apply_manual_record_cap(&mut exact_candidates, &mut warnings);
        }
        let mut retrieval_candidates = exact_candidates;
        attach_thread_neighbors(pool, span, &mut retrieval_candidates).await?;
        return Ok(RecallResult {
            ok: true,
            query: query.to_owned(),
            found: !retrieval_candidates.is_empty(),
            source: "rust-postgres",
            projection: projection.as_str(),
            mode: mode.as_str(),
            warnings,
            retrieval_candidates,
            canon_matches: Vec::new(),
            semantic_chunks: Vec::new(),
            content_chunks: Vec::new(),
            date_matches: Vec::new(),
            query_dates: Vec::new(),
            taxonomy: serde_json::json!({"rooms":rooms,"memoryTypes":[],"threadKeys":[],"namedEntities":[]}),
            cluster_staleness: None,
            cluster_resonance: None,
            rerank_candidates: None,
        });
    }
    let query_dates = query_dates(query);
    let query_terms = references.strip_terms(query_terms(query));
    let content_patterns = query_terms
        .iter()
        .map(|term| format!("%{term}%"))
        .collect::<Vec<_>>();
    let vector_text = match (cfg.embedding_mode, cfg.embed_url.as_deref()) {
        (EmbeddingMode::Disabled, _) => {
            warnings.push("semantic lane absent: embedding disabled in production".to_string());
            None
        }
        (EmbeddingMode::DisabledForTest, _) => {
            warnings.push("semantic lane absent: embedding disabled for isolated test".to_string());
            None
        }
        (EmbeddingMode::Required, Some(url)) => {
            let mut phase = Phase::start(span, "recall.embed");
            phase.bytes_in(query.len());
            match embed_query(&HTTP_CLIENT, url, &cfg.embed_model, query, EMBED_DIMENSION).await {
                Ok(vector) => {
                    phase.ok();
                    Some(format!(
                        "[{}]",
                        vector
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    ))
                }
                Err(e) => {
                    phase.degraded(match e {
                        EmbedError::Timeout(_) => "embed_timeout",
                        EmbedError::Failed(_) => "app_error.embedding",
                    });
                    warnings.push(format!("semantic lane absent: {e}"));
                    None
                }
            }
        }
        (EmbeddingMode::Required, None) => {
            warnings.push("semantic lane absent: embedding endpoint is required".to_string());
            None
        }
    };
    let decay_now = Utc::now();
    let phase = Phase::start(span, "recall.lexical");
    let bm25f_candidates = load_bm25f_candidates_for_terms(
        pool,
        &rooms,
        &references.strip_terms(bm25f::query_terms(query)),
        temporal_decay,
        decay_now,
        &settings,
        &mut warnings,
    )
    .await?;
    phase.ok();
    let semantic_vocabulary_concepts = match vector_text.as_deref() {
        Some(vector) => {
            let phase = Phase::start(span, "recall.vocabulary");
            match load_semantic_vocabulary_concepts(pool, &rooms, vector, cfg).await {
                Ok(concepts) => {
                    phase.ok();
                    concepts
                }
                Err(error) => {
                    // A missing/unmigrated/stale vocabulary must never impair exact recall.
                    phase.degraded("recall.vocabulary_absent");
                    warnings.push(format!("semantic lexical bridge absent: {error}"));
                    Vec::new()
                }
            }
        }
        None => Vec::new(),
    };
    let semantic_vocabulary_terms = semantic_vocabulary_terms(&semantic_vocabulary_concepts);
    let phase = Phase::start(span, "recall.semantic_lexical");
    let semantic_lexical_candidates = load_bm25f_candidates_for_terms(
        pool,
        &rooms,
        &semantic_vocabulary_terms,
        temporal_decay,
        decay_now,
        &settings,
        &mut warnings,
    )
    .await?;
    phase.ok();
    let semantic_fetch_limit = (!temporal_decay).then_some(i64::from(semantic_top_k));
    let content_fetch_limit = (!temporal_decay).then_some(i64::from(content_top_k));
    let mut semantic_chunks = Vec::new();
    if let Some(vector_text) = vector_text.clone() {
        let phase = Phase::start(span, "recall.semantic");
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
        .bind(semantic_min_similarity)
        .bind(semantic_fetch_limit)
        .bind(origami::boats::MEMORY_KIND)
        .fetch_all(pool)
        .await?;
        for row in semantic_rows {
            let sim: f64 = row.try_get("sim")?;
            if sim < semantic_min_similarity {
                continue;
            }
            let source_path: String = row.try_get("source_path")?;
            let title: Option<String> = row.try_get("title")?;
            let heading_path: Option<String> = row.try_get("heading_path")?;
            let body: String = row.try_get("body")?;
            let meta: serde_json::Value = row.try_get("meta")?;
            let (durability, temporal_weight) = if temporal_decay {
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
                    "semantic lane empty: top sim {top:.2} < floor {semantic_min_similarity:.2} (rooms {})",
                    rooms.join(",")
                ),
                None => format!(
                    "semantic lane empty: no embedded chunks (rooms {})",
                    rooms.join(",")
                ),
            });
        }
        phase.ok();
    }
    let phase = Phase::start(span, "recall.content");
    let content_rows = content_lane_rows(
        pool,
        query,
        &rooms,
        content_min_similarity,
        content_fetch_limit,
        &content_patterns,
        origami::boats::MEMORY_KIND,
    )
    .await?;
    phase.ok();
    let mut content_chunks = Vec::new();
    for row in content_rows {
        let sim: f64 = row.try_get("sim")?;
        let source_path: String = row.try_get("source_path")?;
        let title: Option<String> = row.try_get("title")?;
        let heading_path: Option<String> = row.try_get("heading_path")?;
        let body: String = row.try_get("body")?;
        let meta: serde_json::Value = row.try_get("meta")?;
        let (durability, temporal_weight) = if temporal_decay {
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
    if temporal_decay {
        semantic_chunks.sort_by(|left, right| compare_weighted_lane(left, right, "sim"));
        content_chunks.sort_by(|left, right| compare_weighted_lane(left, right, "ws"));
    }
    semantic_chunks.truncate(semantic_top_k as usize);
    content_chunks.truncate(content_top_k as usize);
    let mut date_matches = Vec::new();
    if !query_dates.is_empty() {
        let phase = Phase::start(span, "recall.dates");
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
        phase.ok();
    }
    let phase = Phase::start(span, "recall.threads");
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
    .bind(query)
    .bind(&content_patterns)
    .bind(origami::boats::MEMORY_KIND)
    .fetch_all(pool)
    .await?;
    phase.ok();
    let phase = Phase::start(span, "recall.fuse");
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
            // Only this lane selects the memory's type and age, so it hands
            // them to an entry another lane opened.
            existing["memory_type"] = candidate["memory_type"].clone();
            existing["memory_age_days"] = candidate["memory_age_days"].clone();
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
                "memory_type": candidate["memory_type"],
                "memory_age_days": candidate["memory_age_days"],
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
                "memory_type": candidate["memory_type"],
                "memory_age_days": candidate["memory_age_days"],
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
    // An exact reference already owns its memory: a ranked chunk of the same
    // row would only repeat it below, so the exact row takes the memory's one
    // seat and leads the evidence ahead of the ranked cap.
    let exact_memory_ids = exact_candidates
        .iter()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect::<BTreeSet<_>>();
    let mut retrieval_candidates: Vec<_> = fused
        .into_values()
        .filter(|candidate| {
            candidate["memory_id"]
                .as_i64()
                .is_none_or(|memory_id| !exact_memory_ids.contains(&memory_id))
        })
        .collect();
    // The mode profile rides the fused score. Only the two BM25F lanes select
    // `m.type` and the memory's age; semantic, content and thread rows that no
    // BM25F row touched carry neither and take the flat multiplier.
    apply_and_order(&profile, &mut retrieval_candidates);
    retrieval_candidates.truncate(semantic_top_k.max(content_top_k) as usize);
    retrieval_candidates.splice(0..0, exact_candidates);
    let rerank_candidates =
        if should_build_rerank_candidates(projection, request.rerank_candidate_top_k()) {
            let phase = Phase::start(span, "recall.rerank_pool");
            match load_bm25f_candidates_for_terms_broad(
                pool,
                &rooms,
                &references.strip_terms(bm25f::query_terms(query)),
                temporal_decay,
                decay_now,
                &settings,
            )
            .await
            {
                Ok(broad) => {
                    let candidates = build_rerank_candidates(
                        &retrieval_candidates,
                        &broad,
                        &exact_memory_ids,
                        request.rerank_candidate_top_k() as usize,
                    );
                    phase.ok();
                    Some(candidates)
                }
                Err(_) => {
                    phase.degraded("recall.rerank_pool_unavailable");
                    None
                }
            }
        } else {
            None
        };
    phase.ok();
    // A manual projection reads records, not chunks: the selected memories are
    // capped by count and then carry their complete bodies. The auto
    // projection keeps the bounded excerpt under the working-set budget.
    if projection.is_manual() {
        apply_manual_record_cap(&mut retrieval_candidates, &mut warnings);
        let phase = Phase::start(span, "recall.hydrate");
        hydrate_record_bodies(pool, &rooms, &mut retrieval_candidates, &mut warnings).await?;
        phase.ok();
    }
    attach_thread_neighbors(pool, span, &mut retrieval_candidates).await?;
    // Canon lookup matches three ways, ranked. Tokens alone can never match a
    // multi-word name or a hyphenated alias, which is how 42 of 109 rows went
    // dark; widening to ILIKE/tsvector then lets fuzzy hits evict the row the
    // caller literally named. So: whole-phrase exact, then token or in-query
    // mention exact, then similarity — and only the last tier competes for
    // leftover LIMIT slots. An exact row is authority the caller named, so it
    // carries its complete active assertion; only the similarity tier is
    // excerpted, and that excerpt says so.
    let phase = Phase::start(span, "recall.canon");
    let query_phrase = query.trim().to_lowercase();
    let canon_rows = sqlx::query(
        r#"WITH tiered AS (
             SELECT id,name,kind,summary,aliases,weighty,pointer_files,
                    (CASE
                       WHEN lower(name) = $5
                         OR EXISTS (SELECT 1 FROM unnest(aliases) a1 WHERE lower(a1) = $5)
                       THEN 0
                       WHEN lower(name) = ANY($2::text[])
                         OR EXISTS (SELECT 1 FROM unnest(aliases) a2 WHERE lower(a2) = ANY($2::text[]))
                         OR (length(name) >= 3 AND strpos($5, lower(name)) > 0)
                         OR EXISTS (SELECT 1 FROM unnest(aliases) a3
                                    WHERE length(a3) >= 3 AND strpos($5, lower(a3)) > 0)
                       THEN 1
                       ELSE 2
                     END) AS exactness
             FROM named_entities, websearch_to_tsquery($6::regconfig, $3) AS tsq
             WHERE room = ANY($1::text[])
               AND authority = 'active'
               AND (lower(name) = $5
                    OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias) = $5)
                    OR lower(name) = ANY($2::text[])
                    OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE lower(alias) = ANY($2::text[]))
                    OR (length(name) >= 3 AND strpos($5, lower(name)) > 0)
                    OR EXISTS (SELECT 1 FROM unnest(aliases) alias
                               WHERE length(alias) >= 3 AND strpos($5, lower(alias)) > 0)
                    OR name ILIKE ANY($4::text[])
                    OR EXISTS (SELECT 1 FROM unnest(aliases) alias WHERE alias ILIKE ANY($4::text[]))
                    OR summary_tsv @@ tsq)
           )
           SELECT id,name,kind,summary,aliases,weighty,pointer_files,exactness
           FROM tiered
           ORDER BY exactness, weighty DESC, name
           LIMIT 12"#,
    )
    .bind(&rooms)
    .bind(&query_terms)
    .bind(query)
    .bind(&content_patterns)
    .bind(&query_phrase)
    .bind(&settings.house_language)
    .fetch_all(pool)
    .await?;
    // The SQL tiers bound the rows; the profile reorders inside one tier, so a
    // mode lifts its own kinds without ever passing an exact row the caller
    // named.
    struct CanonRow {
        id: i64,
        name: String,
        kind: String,
        summary: String,
        aliases: Vec<String>,
        weighty: bool,
        files: serde_json::Value,
        exactness: i32,
    }
    let mut canon_ordered = Vec::with_capacity(canon_rows.len());
    for row in canon_rows {
        canon_ordered.push(CanonRow {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            kind: row.try_get("kind")?,
            summary: row.try_get("summary")?,
            aliases: row.try_get("aliases")?,
            weighty: row.try_get("weighty")?,
            files: row.try_get("pointer_files")?,
            exactness: row.try_get("exactness")?,
        });
    }
    canon_ordered.sort_by(|left, right| {
        profile.compare_canon(
            &CanonOrder {
                exactness: left.exactness,
                kind: &left.kind,
                weighty: left.weighty,
                name: &left.name,
            },
            &CanonOrder {
                exactness: right.exactness,
                kind: &right.kind,
                weighty: right.weighty,
                name: &right.name,
            },
        )
    });
    let mut canon_matches = Vec::new();
    let mut named_entities = Vec::new();
    for canon in canon_ordered {
        let CanonRow {
            id,
            name,
            kind,
            summary,
            aliases,
            weighty,
            files,
            exactness,
        } = canon;
        let exact = exactness <= 1;
        let truncated = !exact && exceeds_excerpt(&summary);
        let projected = if truncated {
            bounded_excerpt(&summary)
        } else {
            summary.clone()
        };
        canon_matches.push(serde_json::json!({
            "termKey": name,
            "entry": {
                "id": id,
                "type": kind,
                "summary": projected,
                "aliases": aliases,
                "weighty": weighty,
                "exact": exact,
                "truncated": truncated,
                "full_read": truncated.then(|| format!("canon_read {id}")),
                "files": protocol_pointer_files(&files),
            }
        }));
        named_entities.push(name);
    }
    phase.ok();
    let phase = Phase::start(span, "recall.taxonomy");
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
    phase.ok();
    let phase = Phase::start(span, "recall.cluster");
    let cluster_staleness = cluster_staleness(pool, None)
        .await
        .ok()
        .and_then(|s| serde_json::to_value(s).ok());
    let cluster_resonance = if let Some(v) = vector_text.as_deref() {
        cluster_resonance(pool, v, &rooms).await.ok()
    } else {
        None
    };
    if cluster_staleness.is_some() && (vector_text.is_none() || cluster_resonance.is_some()) {
        phase.ok();
    } else {
        phase.degraded("recall.cluster_absent");
    }
    Ok(RecallResult {
        ok: true,
        query: query.to_owned(),
        found: !retrieval_candidates.is_empty()
            || !canon_matches.is_empty()
            || !date_matches.is_empty(),
        source: "rust-postgres",
        projection: projection.as_str(),
        mode: mode.as_str(),
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
        rerank_candidates,
    })
}

/// Thread neighbors for every candidate that owns a memory: the visible
/// shoulder of the thread a record sits in, keyed by memory ID.
async fn attach_thread_neighbors(
    pool: &PgPool,
    span: Option<&EmitterSpan>,
    retrieval_candidates: &mut [serde_json::Value],
) -> Result<(), AppError> {
    let memory_ids = retrieval_candidates
        .iter()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let phase = Phase::start(span, "recall.neighbors");
    let mut neighbors = load_thread_neighbors(pool, &memory_ids).await?;
    for candidate in retrieval_candidates.iter_mut() {
        let memory_id = candidate["memory_id"].as_i64().unwrap_or_default();
        candidate["thread_neighbors"] =
            serde_json::Value::Array(neighbors.remove(&memory_id).unwrap_or_default());
    }
    phase.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_rerank_candidates, should_build_rerank_candidates};
    use hearth::RecallProjection;
    use std::collections::BTreeSet;

    #[test]
    fn rerank_pool_is_sidecar_only_and_does_not_mutate_baseline() {
        let baseline = vec![
            serde_json::json!({
                "memory_id": 7,
                "source_path": "memory/7",
                "title": "baseline",
                "heading_path": "",
                "excerpt": "bounded baseline",
                "score": 0.9,
                "thread_neighbors": [{"id": 8}],
                "body": "manual body must not cross the sidecar boundary",
                "semanticChunks": [{"body": "raw lane"}],
            }),
            serde_json::json!({
                "memory_id": 9,
                "source_path": "memory/9",
                "title": "exact",
                "excerpt": "exact reference",
                "score": 1.0,
            }),
        ];
        let before = baseline.clone();
        let broad = vec![serde_json::json!({
            "memory_id": 11,
            "source_path": "memory/11",
            "title": "broad",
            "heading_path": "",
            "body": "x".repeat(2_000),
            "bm25f_score": 0.7,
            "canonMatches": [{"termKey": "forbidden"}],
            "contentChunks": [{"body": "raw lane"}],
            "taxonomy": {"rooms": ["lab"]},
        })];
        let sidecar = build_rerank_candidates(&baseline, &broad, &BTreeSet::from([9]), 64);

        assert_eq!(baseline, before);
        assert_eq!(
            sidecar
                .iter()
                .filter_map(|candidate| candidate["memory_id"].as_i64())
                .collect::<Vec<_>>(),
            vec![7, 11]
        );
        assert!(sidecar.iter().all(|candidate| {
            candidate.get("body").is_none()
                && candidate.get("thread_neighbors").is_none()
                && candidate.get("canonMatches").is_none()
                && candidate.get("semanticChunks").is_none()
                && candidate.get("contentChunks").is_none()
                && candidate.get("taxonomy").is_none()
                && candidate["excerpt"]
                    .as_str()
                    .is_some_and(|excerpt| excerpt.chars().count() <= 1_201)
        }));
    }

    #[test]
    fn rerank_pool_honors_requested_bound_and_exact_precedence() {
        let broad = (1..=10)
            .map(|memory_id| {
                serde_json::json!({
                    "memory_id": memory_id,
                    "source_path": format!("memory/{memory_id}"),
                    "title": "",
                    "excerpt": "candidate",
                    "score": memory_id as f64,
                })
            })
            .collect::<Vec<_>>();
        let sidecar = build_rerank_candidates(&[], &broad, &BTreeSet::from([10]), 3);
        assert_eq!(sidecar.len(), 3);
        assert!(!sidecar.iter().any(|candidate| candidate["memory_id"] == 10));
    }

    #[test]
    fn rerank_sidecar_is_automatic_only() {
        assert!(should_build_rerank_candidates(RecallProjection::Auto, 1));
        assert!(!should_build_rerank_candidates(RecallProjection::Auto, 0));
        assert!(!should_build_rerank_candidates(
            RecallProjection::Manual,
            64
        ));
    }
}
