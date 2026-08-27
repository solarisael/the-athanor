use super::bounded_excerpt;
use super::temporal::{compare_weighted_lane, giga_temporal_factor};
use crate::bm25f::{
    self, BODY as BM25F_BODY, FieldValue as Bm25fFieldValue, HEADING as BM25F_HEADING,
    MEMORY_TYPE as BM25F_MEMORY_TYPE, SOURCE_PATH as BM25F_SOURCE_PATH, THREADS as BM25F_THREADS,
    TITLE as BM25F_TITLE,
};
use crate::config::AppError;
use crate::settings::RoomSettings;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};

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

pub(super) async fn load_bm25f_candidates_for_terms(
    pool: &PgPool,
    rooms: &[String],
    terms: &[String],
    temporal_decay: bool,
    decay_now: DateTime<Utc>,
    settings: &RoomSettings,
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
             AND COALESCE(m.type,'') <> $2"#,
    )
    .bind(rooms)
    .bind(origami::boats::MEMORY_KIND)
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
               AND COALESCE(m.type,'') <> $3
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
    .bind(origami::boats::MEMORY_KIND)
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
             AND COALESCE(m.type,'') <> $4
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
    .bind(origami::boats::MEMORY_KIND)
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
            giga_temporal_factor(&meta, decay_now, settings)
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

pub(super) async fn load_bm25f_candidates(
    pool: &PgPool,
    rooms: &[String],
    query: &str,
    temporal_decay: bool,
    decay_now: DateTime<Utc>,
    settings: &RoomSettings,
    warnings: &mut Vec<String>,
) -> Result<Vec<serde_json::Value>, AppError> {
    load_bm25f_candidates_for_terms(
        pool,
        rooms,
        &bm25f::query_terms(query),
        temporal_decay,
        decay_now,
        settings,
        warnings,
    )
    .await
}
