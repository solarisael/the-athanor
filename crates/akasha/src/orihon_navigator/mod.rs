use crate::bm25f::{self, FieldValue};
use hearth::orihon::{Kind, ReadRequest};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

const POOL_LIMIT: usize = 512;
const ROW_BYTES: usize = 64 * 1024;
const RESULT_BYTES: usize = 128 * 1024;

#[derive(FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub seq: i64,
    pub turn_no: i64,
    pub speaker: String,
    pub text: String,
    pub said_at: String,
    #[sqlx(skip)]
    pub score: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidates {
    pub authority: &'static str,
    pub candidates: Vec<Candidate>,
    pub complete: bool,
    pub warnings: Vec<String>,
}

fn terms(query: &str) -> Result<Vec<String>, String> {
    if query.trim().is_empty() || query.len() > 4096 || query.contains('\0') {
        return Err("Orihon query must contain 1 through 4096 UTF-8 bytes and no NUL".into());
    }
    let terms = bm25f::query_terms(query);

    if terms.len() > 32 {
        return Err("Orihon query contains more than 32 subject terms".into());
    }

    Ok(terms)
}

fn rank(
    mut rows: Vec<Candidate>,
    terms: &[String],
    mut complete: bool,
    result_limit: usize,
) -> Candidates {
    let mut frequencies = BTreeMap::new();
    let mut lengths = 0;

    for row in &rows {
        let tokens = bm25f::tokens(&row.text);
        lengths += tokens.len();

        for term in terms {
            if tokens.contains(term) {
                *frequencies.entry(term.clone()).or_insert(0) += 1;
            }
        }
    }
    let count = rows.len();
    let average = lengths as f64 / count.max(1) as f64;

    for row in &mut rows {
        row.score = bm25f::score(
            terms,
            count as u64,
            &frequencies,
            &[FieldValue {
                name: "user",
                text: &row.text,
                average_length: average,
                config: bm25f::BODY,
            }],
        )
        .value;
    }
    rows.retain(|row| row.score > 0.0);
    rows.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| b.seq.cmp(&a.seq)));
    let mut candidates = Vec::new();
    let mut bytes = 0;

    for row in rows {
        if candidates.len() == result_limit || bytes + row.text.len() > RESULT_BYTES {
            complete = false;
            continue;
        }
        bytes += row.text.len();
        candidates.push(row);
    }
    let warnings = if complete {
        Vec::new()
    } else {
        vec!["Orihon search is bounded; some matches exceed the candidate or byte limits. Narrow the query. Source text was not clipped.".into()]
    };

    Candidates {
        authority: "provenance",
        candidates,
        complete,
        warnings,
    }
}

/// Rank matching user rows only. Database I/O stays bound to the authenticated room.
pub async fn search_pg(
    pool: &PgPool,
    room: &str,
    query: &str,
    exclude_session: Option<&str>,
    result_limit: u32,
) -> Result<Candidates, String> {
    if result_limit == 0 {
        return Err("limit must be positive".into());
    }

    let terms = terms(query)?;
    let result_limit = result_limit as usize;
    let pool_limit = POOL_LIMIT.max(result_limit);

    if terms.is_empty() {
        return Ok(rank(Vec::new(), &terms, true, result_limit));
    }
    let patterns: Vec<String> = terms
        .iter()
        .map(|term| {
            format!(
                "%{}%",
                term.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        })
        .collect();
    // enough: bounded lexical scan of user pages; add a search index when measured room latency requires it.
    let mut rows = sqlx::query_as::<_, Candidate>(
        "SELECT seq,turn_no,speaker,text,source_time AS said_at FROM orihon
         WHERE room=$1 AND kind='user' AND text ILIKE ANY($2)
         AND octet_length(text)<=$3 AND ($5::text IS NULL OR source_session<>$5) ORDER BY seq DESC LIMIT $4",
    )
    .bind(room)
    .bind(&patterns)
    .bind(ROW_BYTES as i64)
    .bind(pool_limit as i64 + 1)
    .bind(exclude_session)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let oversized: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM orihon WHERE room=$1 AND kind='user'
         AND text ILIKE ANY($2) AND octet_length(text)>$3 AND ($4::text IS NULL OR source_session<>$4))",
    )
    .bind(room)
    .bind(&patterns)
    .bind(ROW_BYTES as i64)
    .bind(exclude_session)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?;
    let complete = rows.len() <= pool_limit && !oversized;
    rows.truncate(pool_limit);

    Ok(rank(rows, &terms, complete, result_limit))
}

/// Read and rank a Vault book. Call from a blocking worker, not the async executor.
pub fn search_file(
    directory: &Path,
    room: &str,
    query: &str,
    exclude_session: Option<&str>,
    result_limit: u32,
) -> Result<Candidates, String> {
    if result_limit == 0 {
        return Err("limit must be positive".into());
    }

    let result_limit = result_limit as usize;
    let pool_limit = POOL_LIMIT.max(result_limit);
    let terms = terms(query)?;
    let mut rows = VecDeque::new();
    let mut after_seq = None;
    let mut complete = true;

    if terms.is_empty() {
        return Ok(rank(Vec::new(), &terms, complete, result_limit));
    }

    loop {
        let receipt = origami::orihon::read_file(
            directory,
            room,
            ReadRequest {
                after_seq,
                until_seq: None,
                turn_no: None,
                day: None,
                limit: 200,
            },
        )
        .map_err(|error| error.to_string())?;

        for row in receipt.rows {
            if row.kind != Kind::User || exclude_session == Some(row.source_session.as_str()) {
                continue;
            }
            let lower = row.text.to_lowercase();

            if !terms.iter().any(|term| lower.contains(term)) {
                continue;
            }
            if row.text.len() > ROW_BYTES {
                complete = false;
                continue;
            }
            rows.push_back(Candidate {
                seq: row.seq,
                turn_no: row.turn_no,
                speaker: row.speaker,
                text: row.text,
                said_at: row.said_at,
                score: 0.0,
            });

            if rows.len() > pool_limit {
                rows.pop_front();
                complete = false;
            }
        }
        if !receipt.has_more {
            break;
        }
        after_seq = receipt.next_after;
    }

    Ok(rank(
        rows.into_iter().collect(),
        &terms,
        complete,
        result_limit,
    ))
}
