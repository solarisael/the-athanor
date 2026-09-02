use super::bounds::{
    GIGA_MAX_STORED_RATIONALE_BYTES, GIGA_RATIONALE_TRUNCATION_MARKER, truncate_with_marker,
};
use super::failure::{WorkerFailure, domain_failure};
use super::identity::{
    GIGA_MODEL_MANIFEST_DIGEST, GIGA_MODEL_TAG, GIGA_PROMPT_VERSION, candidate_id,
    configuration_digest,
};
use super::ledger::ResolvedSource;
use super::ollama::{ollama_config, request_ollama_structured, verify_ollama_model};
use super::prompts::{GIGA_EXTRACTION_PROMPT, GIGA_GATE_PROMPT};
use super::schema::{
    ExtractionOutput, GateKind, GateOutput, extraction_schema, gate_schema, schema_json,
};
use super::validation::{dedupe_preserving_order, validate_extraction, validate_gate};
use chrono::Utc;
use hearth::{
    GigaAuthority, GigaCandidate, GigaClassifierIdentity, GigaEvent, GigaReviewState, GigaScope,
    GigaScores, GigaVisibility,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct ModelSource<'a> {
    source_id: &'a str,
    role: &'a str,
    timestamp: &'a str,
    text: &'a str,
}

pub(super) async fn classify_event(
    event: &GigaEvent,
    sources: &[ResolvedSource],
) -> Result<Option<GigaCandidate>, WorkerFailure> {
    let config = ollama_config()?;
    verify_ollama_model(&config).await?;
    let model_sources = sources
        .iter()
        .map(|resolved| ModelSource {
            source_id: resolved.source.source_id(),
            role: resolved.source.role(),
            timestamp: resolved.source.timestamp(),
            text: &resolved.text,
        })
        .collect::<Vec<_>>();
    let all_source_ids = model_sources
        .iter()
        .map(|source| source.source_id.to_owned())
        .collect::<Vec<_>>();
    let mut gate: GateOutput = request_ollama_structured(
        &config,
        GIGA_GATE_PROMPT,
        json!({
            "project_keys": event.project_keys(),
            "sources": model_sources
        }),
        schema_json(gate_schema(&all_source_ids))?,
        256,
    )
    .await?;
    dedupe_preserving_order(&mut gate.source_ids);
    validate_gate(&gate, event)?;
    if gate.kind == GateKind::None {
        return Ok(None);
    }
    let mut extraction: ExtractionOutput = request_ollama_structured(
        &config,
        GIGA_EXTRACTION_PROMPT,
        json!({
            "fixed_kind": gate.kind.as_str(),
            "gate_source_ids": gate.source_ids,
            "project_keys": event.project_keys(),
            "sources": model_sources
        }),
        extraction_schema(&gate.source_ids, gate.kind.requires_proof())?,
        768,
    )
    .await?;
    dedupe_preserving_order(&mut extraction.source_ids);
    dedupe_preserving_order(&mut extraction.proof_source_ids);
    validate_extraction(&extraction, &gate)?;
    let rationale = truncate_with_marker(
        &extraction.rationale,
        GIGA_MAX_STORED_RATIONALE_BYTES,
        GIGA_RATIONALE_TRUNCATION_MARKER,
    );
    let selected = sources
        .iter()
        .filter(|resolved| {
            extraction
                .source_ids
                .iter()
                .any(|source_id| source_id == resolved.source.source_id())
        })
        .map(|resolved| resolved.source.clone())
        .collect::<Vec<_>>();
    if selected.len() != extraction.source_ids.len() {
        return Err(domain_failure());
    }
    let completed_at = Utc::now().to_rfc3339();
    let classifier = GigaClassifierIdentity::new(
        GIGA_MODEL_TAG.into(),
        "ollama".into(),
        GIGA_MODEL_MANIFEST_DIGEST.into(),
        GIGA_PROMPT_VERSION.into(),
        configuration_digest(&config)?,
        Uuid::new_v4().to_string(),
        completed_at.clone(),
    )
    .map_err(|_| domain_failure())?;
    let scores = GigaScores::new(
        extraction.priority,
        extraction.novelty,
        extraction.durability,
        extraction.confidence,
    )
    .map_err(|_| domain_failure())?;
    let project = event.project_keys().first().cloned();
    let scope = GigaScope::new(
        Some(event.room().to_string()),
        project,
        GigaVisibility::Private,
        true,
    )
    .map_err(|_| domain_failure())?;
    GigaCandidate::new(
        candidate_id(event, gate.kind, &extraction.source_ids)?,
        event.event_id().into(),
        event.room().clone(),
        event.session_id().into(),
        gate.kind.candidate_kind()?,
        selected,
        extraction.proof_source_ids,
        scores,
        event.project_keys().to_vec(),
        Vec::new(),
        Vec::new(),
        extraction.retrieval_terms,
        extraction.proposed_title,
        extraction.gist,
        rationale,
        scope,
        GigaAuthority::PointerOnly,
        GigaReviewState::Unreviewed,
        classifier,
        completed_at,
        None,
        Vec::new(),
    )
    .map(Some)
    .map_err(|_| domain_failure())
}
