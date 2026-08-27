use hearth::GigaEvent;
use super::bounds::{GIGA_MAX_MODEL_RATIONALE_BYTES, bounded_trimmed};
use super::failure::{WorkerFailure, domain_failure};
use super::schema::{
    ExtractionOutput, GIGA_MAX_RETRIEVAL_TERMS, GIGA_MAX_SELECTED_SOURCES, GateKind, GateOutput,
};

fn exact_unique_subset(values: &[String], allowed: &[String], allow_empty: bool) -> bool {
    (allow_empty || !values.is_empty())
        && values.len() <= GIGA_MAX_SELECTED_SOURCES
        && values
            .iter()
            .enumerate()
            .all(|(index, value)| allowed.contains(value) && !values[..index].contains(value))
}

/// Ollama's JSON-Schema-to-GBNF conversion does not enforce `uniqueItems`, so
/// the model can emit the same source ID twice (observed 2026-07-31 on
/// proof_source_ids). Duplicate IDs are navigation-aid redundancy, not an
/// authority violation: dedupe preserving order, then let exact_unique_subset
/// keep its sharp refusal for genuinely out-of-set IDs.
pub(super) fn dedupe_preserving_order(values: &mut Vec<String>) {
    let mut seen = Vec::with_capacity(values.len());
    values.retain(|value| {
        if seen.contains(value) {
            false
        } else {
            seen.push(value.clone());
            true
        }
    });
}

pub(super) fn validate_gate(gate: &GateOutput, event: &GigaEvent) -> Result<(), WorkerFailure> {
    let source_ids = event
        .source_refs()
        .iter()
        .map(|source| source.source_id().to_owned())
        .collect::<Vec<_>>();
    if !bounded_trimmed(&gate.reason, 1_024)
        || !exact_unique_subset(&gate.source_ids, &source_ids, gate.kind == GateKind::None)
        || (gate.kind == GateKind::None && !gate.source_ids.is_empty())
        || (gate.kind != GateKind::None && gate.source_ids.is_empty())
        || (gate.kind == GateKind::ProjectLesson && event.project_keys().len() != 1)
        || (gate.kind == GateKind::CodingLesson && !event.project_keys().is_empty())
    {
        return Err(domain_failure());
    }
    Ok(())
}

fn finite_score(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

pub(super) fn validate_extraction(
    extraction: &ExtractionOutput,
    gate: &GateOutput,
) -> Result<(), WorkerFailure> {
    if !exact_unique_subset(&extraction.source_ids, &gate.source_ids, false)
        || !exact_unique_subset(
            &extraction.proof_source_ids,
            &extraction.source_ids,
            !gate.kind.requires_proof(),
        )
        || (gate.kind.requires_proof() && extraction.proof_source_ids.is_empty())
        || !bounded_trimmed(&extraction.proposed_title, 160)
        || !bounded_trimmed(&extraction.gist, 1_200)
        || !bounded_trimmed(&extraction.rationale, GIGA_MAX_MODEL_RATIONALE_BYTES)
        || !finite_score(extraction.priority)
        || !finite_score(extraction.novelty)
        || !finite_score(extraction.durability)
        || !finite_score(extraction.confidence)
        || extraction.retrieval_terms.len() > GIGA_MAX_RETRIEVAL_TERMS
        || extraction
            .retrieval_terms
            .iter()
            .enumerate()
            .any(|(index, term)| {
                !bounded_trimmed(term, 80) || extraction.retrieval_terms[..index].contains(term)
            })
    {
        return Err(domain_failure());
    }
    Ok(())
}
