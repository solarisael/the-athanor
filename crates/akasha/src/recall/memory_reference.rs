use super::bounded_excerpt;
use crate::config::AppError;
use hearth::{MANUAL_RECORD_CAP, RecallProjection};
use regex::Regex;
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// `#4197`, `memory 4197`, `memory #4197`, `memória 4197`, `[4197]`.
static EXPLICIT_REFERENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:#|\bmem(?:ory|ories|[óo]ria|[óo]rias)?\s*#?|\[)(\d{1,18})\b")
        .expect("memory reference regex must compile")
});
/// Words that continue a list of references: `memories 4197, 4198 and 4199`.
const LIST_JOINERS: [&str; 4] = ["and", "e", "&", "+"];
/// Words that only announce a reference: `memory 4197`, `memórias 4197 e 4198`.
const REFERENCE_WORDS: [&str; 7] = [
    "mem",
    "memory",
    "memories",
    "memoria",
    "memorias",
    "memória",
    "memórias",
];

/// Exact memory references named by a query.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct MemoryReferences {
    /// Referenced memory IDs in query order, deduplicated.
    pub ids: Vec<i64>,
    /// Lowercased query terms that spelled those IDs; ranked lanes drop them so
    /// a resolved reference never reappears under `missing_terms`.
    pub terms: BTreeSet<String>,
}

impl MemoryReferences {
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Ranked-lane terms with the reference tokens removed.
    pub fn strip_terms(&self, terms: Vec<String>) -> Vec<String> {
        if self.terms.is_empty() {
            return terms;
        }
        terms
            .into_iter()
            .filter(|term| !self.terms.contains(term))
            .collect()
    }

    /// True when the query names references and nothing else: `#4197`,
    /// `memory 4197`, `memories 4520, 4516 and 999`, `#1,#2`. Every token is
    /// a reference, a reference word, a list joiner, or bare punctuation.
    /// Such a request is answered by the resolved records and their warnings
    /// alone; the words `memories` and `and` must never rank unrelated filler.
    pub fn only_references(&self, query: &str) -> bool {
        !self.ids.is_empty()
            && query
                .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';'))
                .all(|token| {
                    let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
                    if bare.is_empty() || self.terms.contains(bare) {
                        return true;
                    }
                    let lower = bare.to_lowercase();
                    LIST_JOINERS.contains(&lower.as_str())
                        || REFERENCE_WORDS.contains(&lower.as_str())
                })
    }
}

fn push_reference(references: &mut MemoryReferences, digits: &str) {
    let Ok(id) = digits.parse::<i64>() else {
        return;
    };
    if id <= 0 {
        return;
    }
    if !references.ids.contains(&id) {
        references.ids.push(id);
    }
    references.terms.insert(digits.to_owned());
}

/// Explicit references (`memory 4197`, `#4197`, `[4197]`) always count. A bare
/// integer counts only when the query is nothing but that integer, or when
/// it continues a list that started with an explicit reference
/// (`memories 4197, 4198 and 4199`). A year in prose (`memories from 2026`)
/// is never a reference. Digits that continue into a date or a path
/// (`memory 2026-08-28`) are never a reference.
pub(super) fn memory_references(query: &str) -> MemoryReferences {
    let mut references = MemoryReferences::default();
    for capture in EXPLICIT_REFERENCE_RE.captures_iter(query) {
        let digits = capture.get(1).expect("reference regex has one group");
        let continues = query[digits.end()..]
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '-' | '/' | '.' | ':'));
        if !continues {
            push_reference(&mut references, digits.as_str());
        }
    }
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    if tokens.len() == 1 {
        let bare = bare_integer(tokens[0]);
        if let Some(bare) = bare {
            push_reference(&mut references, bare);
        }
        return references;
    }
    // A list continues while each next token is a joiner or another integer
    // that follows a reference (or a joiner) directly.
    let mut in_list = false;
    for token in tokens {
        let trimmed = token.trim_matches(|c: char| matches!(c, ',' | ';' | ')' | '('));
        if let Some(bare) = bare_integer(trimmed) {
            if references.terms.contains(bare) {
                in_list = true;
                continue;
            }
            if in_list {
                push_reference(&mut references, bare);
                continue;
            }
        }
        in_list = in_list && LIST_JOINERS.contains(&trimmed.to_ascii_lowercase().as_str());
    }
    references
}

fn bare_integer(token: &str) -> Option<&str> {
    let bare = token.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | ')' | '('));
    (!bare.is_empty() && bare.len() <= 18 && bare.bytes().all(|b| b.is_ascii_digit()))
        .then_some(bare)
}

/// Room-scoped primary-key lookup. A row outside `rooms` is refused by ID in
/// `warnings` and its content never leaves the database; a missing row is
/// reported the same way. Found rows come back as retrieval candidates in
/// reference order, ready to lead the evidence. Under a manual projection each
/// candidate also carries `body`, the complete authoritative record; the auto
/// projection keeps only the bounded excerpt.
pub(super) async fn resolve_memory_references(
    pool: &PgPool,
    rooms: &[String],
    references: &MemoryReferences,
    projection: RecallProjection,
    warnings: &mut Vec<String>,
) -> Result<Vec<serde_json::Value>, AppError> {
    if references.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT id,room,source_path,coalesce(title,'') AS title,body,
                archived_at IS NOT NULL AS archived,superseded_by
         FROM memories
         WHERE id = ANY($1::bigint[])",
    )
    .bind(&references.ids)
    .fetch_all(pool)
    .await?;
    let mut candidates = Vec::new();
    for id in &references.ids {
        let Some(row) = rows
            .iter()
            .find(|row| row.try_get::<i64, _>("id").ok() == Some(*id))
        else {
            warnings.push(format!("memory {id} not found"));
            continue;
        };
        let room: String = row.try_get("room")?;
        if !rooms.contains(&room) {
            warnings.push(format!("memory {id} refused: outside room scope"));
            continue;
        }
        let source_path: String = row.try_get("source_path")?;
        let title: String = row.try_get("title")?;
        let body: String = row.try_get("body")?;
        let archived: bool = row.try_get("archived")?;
        let superseded_by: Option<i64> = row.try_get("superseded_by")?;
        let mut reasons = vec!["exact memory id".to_owned()];
        if let Some(successor) = superseded_by {
            reasons.push(format!("historical: superseded by memory {successor}"));
        }
        if archived {
            reasons.push("historical: archived".to_owned());
        }
        let mut candidate = serde_json::json!({
            "memory_id": id,
            "source_path": source_path,
            "title": title,
            "heading_path": "",
            "excerpt": bounded_excerpt(&body),
            "sources": [source_path],
            "term_coverage": 1.0,
            "matched_terms": [id.to_string()],
            "missing_terms": [],
            "score": 1.0,
            "reasons": reasons,
            "source": "exact_id",
            "chunk_index": 0,
        });
        if projection.is_manual() {
            candidate["body"] = serde_json::Value::String(body);
        }
        candidates.push(candidate);
    }
    Ok(candidates)
}

/// Manual projection count cap. Exact references lead, so a trim always drops
/// from the ranked tail first; every dropped record is named in `warnings` so
/// an operator who asked for more than the cap sees exactly what did not fit.
/// A memory holds one seat: fused lanes can select the same memory through
/// different chunks, and the first (best-ranked) occurrence keeps the seat so
/// the cap counts records, never repeats.
pub(super) fn apply_manual_record_cap(
    candidates: &mut Vec<serde_json::Value>,
    warnings: &mut Vec<String>,
) {
    let mut seated = BTreeSet::new();
    candidates.retain(|candidate| match candidate["memory_id"].as_i64() {
        Some(memory_id) => seated.insert(memory_id),
        None => true,
    });
    if candidates.len() <= MANUAL_RECORD_CAP {
        return;
    }
    let dropped = candidates
        .drain(MANUAL_RECORD_CAP..)
        .map(|candidate| {
            candidate["memory_id"]
                .as_i64()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".to_owned())
        })
        .collect::<Vec<_>>();
    warnings.push(format!(
        "manual record cap {MANUAL_RECORD_CAP}: {} selected records dropped (memories {})",
        dropped.len(),
        dropped.join(", ")
    ));
}

/// Manual projection body hydration for ranked candidates. A ranked lane
/// selects a memory through one of its chunks; the record the operator reads
/// is the whole `memories.body`, read again by primary key inside room scope.
/// Candidates that already carry a body (exact references) are left alone. A
/// selected row whose body cannot be read back is refused, not returned
/// partial: it leaves the selection and is named in `warnings`, because a
/// manual record is whole or absent, never an excerpt posing as a record.
pub(super) async fn hydrate_record_bodies(
    pool: &PgPool,
    rooms: &[String],
    candidates: &mut Vec<serde_json::Value>,
    warnings: &mut Vec<String>,
) -> Result<(), AppError> {
    let wanted = candidates
        .iter()
        .filter(|candidate| candidate.get("body").is_none())
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if wanted.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT id,body FROM memories
         WHERE id = ANY($1::bigint[]) AND room = ANY($2::text[])",
    )
    .bind(&wanted)
    .bind(rooms)
    .fetch_all(pool)
    .await?;
    let mut bodies = std::collections::BTreeMap::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let body: String = row.try_get("body")?;
        bodies.insert(id, body);
    }
    let mut refused = Vec::new();
    candidates.retain_mut(|candidate| {
        if candidate.get("body").is_some() {
            return true;
        }
        let Some(memory_id) = candidate["memory_id"].as_i64() else {
            return true;
        };
        match bodies.get(&memory_id) {
            Some(body) => {
                candidate["body"] = serde_json::Value::String(body.clone());
                true
            }
            None => {
                refused.push(memory_id.to_string());
                false
            }
        }
    });
    if !refused.is_empty() {
        warnings.push(format!(
            "memories {} refused: selected but not readable whole in room scope",
            refused.join(", ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_manual_record_cap, memory_references};
    use hearth::MANUAL_RECORD_CAP;

    #[test]
    fn explicit_forms_resolve_and_strip_their_tokens() {
        let references = memory_references("memory 4197 — analysis Sol made with Kintsu");
        assert_eq!(references.ids, vec![4197]);
        assert!(references.terms.contains("4197"));
        assert_eq!(
            memory_references("see #4197 and #4198").ids,
            vec![4197, 4198]
        );
        assert_eq!(memory_references("memory #4197").ids, vec![4197]);
        assert_eq!(memory_references("[4456] mission lock").ids, vec![4456]);
        assert_eq!(memory_references("memória 4197").ids, vec![4197]);
        assert_eq!(memory_references("4197").ids, vec![4197]);
    }

    #[test]
    fn bare_integers_only_alone_or_continuing_an_explicit_list() {
        assert!(memory_references("Sol made 5 kg progress").is_empty());
        assert_eq!(
            memory_references("memories 4197, 4198 about the monitor").ids,
            vec![4197, 4198]
        );
        assert_eq!(
            memory_references("memories 4197, 4198 and 4199").ids,
            vec![4197, 4198, 4199]
        );
        // A year in prose is not a reference, even next to the word memories.
        assert!(memory_references("memories from 2026").is_empty());
        assert!(memory_references("memories about 2026 and the monitor").is_empty());
        // A number after ordinary words does not continue the list.
        assert_eq!(
            memory_references("memory 4197 about the 34 inch monitor").ids,
            vec![4197]
        );
        assert!(memory_references("what happened on 2026-08-28").is_empty());
        assert!(memory_references("memory from 2026-08-28").is_empty());
        assert!(memory_references("#0").is_empty());
    }

    #[test]
    fn strip_terms_removes_only_reference_tokens() {
        let references = memory_references("memory 4197 analysis");
        let terms = references.strip_terms(vec!["4197".into(), "analysis".into(), "memory".into()]);
        assert_eq!(terms, vec!["analysis".to_owned(), "memory".to_owned()]);
    }

    #[test]
    fn a_query_of_nothing_but_references_is_answered_without_ranked_lanes() {
        for query in [
            "memories 4520, 4516 and 999999999999999999",
            "memory 4197",
            "memory #4197",
            "#4197",
            "[4456]",
            "4197",
            "memórias 4197 e 4198",
            "memories 4197, 4198 & 4199",
            "#4197,#4198",
        ] {
            let references = memory_references(query);
            assert!(!references.is_empty(), "{query} must resolve references");
            assert!(
                references.only_references(query),
                "{query} names nothing but references"
            );
        }
        for query in [
            "memory 4197 — analysis Sol made with Kintsu",
            "memories 4197, 4198 about the monitor",
            "memory 4197 2026-08-28",
            "memories from 2026",
            "analysis Sol made with Kintsu",
        ] {
            let references = memory_references(query);
            assert!(
                !references.only_references(query),
                "{query} keeps its ranked lanes"
            );
        }
    }

    #[test]
    fn the_manual_cap_trims_the_tail_and_names_every_dropped_record() {
        let mut candidates = (1..=(MANUAL_RECORD_CAP as i64 + 2))
            .map(|id| serde_json::json!({ "memory_id": id }))
            .collect::<Vec<_>>();
        let mut warnings = Vec::new();
        apply_manual_record_cap(&mut candidates, &mut warnings);
        assert_eq!(candidates.len(), MANUAL_RECORD_CAP);
        assert_eq!(
            candidates[0]["memory_id"], 1,
            "the head keeps reference order"
        );
        assert_eq!(
            warnings,
            vec![format!(
                "manual record cap {MANUAL_RECORD_CAP}: 2 selected records dropped (memories {}, {})",
                MANUAL_RECORD_CAP + 1,
                MANUAL_RECORD_CAP + 2
            )]
        );

        let mut within = vec![serde_json::json!({ "memory_id": 1 })];
        let mut quiet = Vec::new();
        apply_manual_record_cap(&mut within, &mut quiet);
        assert_eq!(within.len(), 1);
        assert!(quiet.is_empty(), "no warning while inside the cap");

        // The same memory selected through two chunks holds one seat, the
        // first-ranked one, and the repeat is not counted against the cap.
        let mut repeated = vec![
            serde_json::json!({ "memory_id": 7, "chunk_index": 2, "heading_path": "later" }),
            serde_json::json!({ "memory_id": 7, "chunk_index": 0, "heading_path": "first" }),
            serde_json::json!({ "memory_id": 8 }),
        ];
        let mut none = Vec::new();
        apply_manual_record_cap(&mut repeated, &mut none);
        assert_eq!(repeated.len(), 2);
        assert_eq!(
            repeated[0]["heading_path"], "later",
            "the best-ranked occurrence keeps the seat"
        );
        assert_eq!(repeated[1]["memory_id"], 8);
        assert!(none.is_empty());
    }
}
