use crate::AppError;
use hearth::{GigaPromotionPayload, GigaPromotionRequest, GigaSourceRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn promotion_source_json(source: &GigaSourceRef) -> Value {
    let range = source
        .range()
        .map(|range| json!({"start": range.start(), "end": range.end()}));
    json!({
        "source_type": source.source_type().as_str(),
        "source_id": source.source_id(),
        "role": source.role(),
        "timestamp": source.timestamp(),
        "content_hash": source.content_hash(),
        "scope": {
            "room": source.scope().room().map(ToString::to_string),
            "project": source.scope().project(),
            "visibility": source.scope().visibility().as_str(),
            "publication_review_required": source.scope().publication_review_required(),
        },
        "range": range,
    })
}

pub(super) fn promotion_sources_json(sources: &[GigaSourceRef]) -> Vec<Value> {
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by(|left, right| {
        (left.source_type().as_str(), left.source_id())
            .cmp(&(right.source_type().as_str(), right.source_id()))
    });
    sources.into_iter().map(promotion_source_json).collect()
}

fn promotion_payload_json(payload: &GigaPromotionPayload) -> Value {
    match payload {
        GigaPromotionPayload::Memory(payload) => json!({
            "kind": "memory",
            "payload": {
                "title": payload.title(),
                "body": payload.body(),
                "threads": payload.threads(),
            },
        }),
        GigaPromotionPayload::CodingLesson(payload) => json!({
            "kind": "coding_lesson",
            "payload": {
                "title": payload.title(),
                "body": payload.body(),
                "shape": payload.shape(),
                "proof_pattern": payload.proof_pattern(),
                "trigger_context": payload.trigger_context(),
                "tags": payload.tags(),
            },
        }),
        GigaPromotionPayload::ProjectLesson { payload, .. } => json!({
            "kind": "project_lesson",
            "payload": {
                "title": payload.title(),
                "body": payload.body(),
                "project": payload.project(),
                "proof_pattern": payload.proof_pattern(),
                "trigger_context": payload.trigger_context(),
                "language_keys": payload.language_keys(),
                "technology_keys": payload.technology_keys(),
                "thread_keys": payload.thread_keys(),
                "tags": payload.tags(),
            }
        }),
    }
}

pub(super) fn publication_consent_json(payload: &GigaPromotionPayload) -> Option<Value> {
    match payload {
        GigaPromotionPayload::ProjectLesson { .. } => Some(json!({
            "operator_approved": true,
            "reviewer_approved": true,
        })),
        GigaPromotionPayload::Memory(_) | GigaPromotionPayload::CodingLesson(_) => None,
    }
}

pub(super) fn promotion_digest(request: &GigaPromotionRequest) -> Result<String, AppError> {
    let canonical = json!({
        "candidate_id": request.candidate_id(),
        "room": request.room().to_string(),
        "reviewer_id": request.reviewer_id(),
        "operator_identity": request.operator_identity(),
        "authorization_basis": request.authorization_basis(),
        "source_refs": promotion_sources_json(request.source_refs()),
        "target": promotion_payload_json(request.payload()),
        "publication_consent": publication_consent_json(request.payload()),
        "reviewed_at": request.reviewed_at(),
    });
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| AppError::Protocol(format!("GIGA promotion digest failed: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(super) fn normalize_promotion_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
