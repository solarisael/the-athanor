use super::bounded_excerpt;
use crate::config::AppError;
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
/// reference order, ready to lead the evidence.
pub(super) async fn resolve_memory_references(
    pool: &PgPool,
    rooms: &[String],
    references: &MemoryReferences,
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
        candidates.push(serde_json::json!({
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
        }));
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::memory_references;

    #[test]
    fn explicit_forms_resolve_and_strip_their_tokens() {
        let references = memory_references("memory 4197 — analysis Sol made with Kintsu");
        assert_eq!(references.ids, vec![4197]);
        assert!(references.terms.contains("4197"));
        assert_eq!(memory_references("see #4197 and #4198").ids, vec![4197, 4198]);
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
        let terms = references.strip_terms(vec![
            "4197".into(),
            "analysis".into(),
            "memory".into(),
        ]);
        assert_eq!(terms, vec!["analysis".to_owned(), "memory".to_owned()]);
    }
}
