use super::bounds::truncate_with_marker;
use super::context::{MemoryContext, MemoryStatus, TargetJudgment, judge_target};
use super::failure::{WorkerFailure, domain_failure};
use super::identity::{GIGA_PROMPT_VERSION, candidate_id, configuration_digest};
use super::ledger::ResolvedSource;
use super::typesafe::{GigaClassifierConfig, JevJudgment};
use crate::Config;
use chrono::Utc;
use hearth::{
    GigaAuthority, GigaCandidate, GigaCandidateKind, GigaClassifierIdentity, GigaEvent,
    GigaReviewState, GigaScope, GigaScores, GigaVisibility,
};
use sqlx::PgPool;
use uuid::Uuid;

fn source_label(source: &ResolvedSource) -> String {
    format!("{} @ {}", source.source.role(), source.source.timestamp())
}

fn bounded_excerpt(text: &str, maximum: usize) -> String {
    let trimmed = text.trim();
    truncate_with_marker(trimmed, maximum, "\n[excerpt truncated]")
}

fn joined_ids(ids: &[i64]) -> String {
    if ids.is_empty() {
        return "none".into();
    }
    ids.iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// How the judgment met memory. Recalled entries are named by ID only:
/// they are context, never the candidate's source or proof.
fn novelty_line(novelty: Option<f64>, memory: &MemoryContext) -> String {
    match novelty {
        Some(score) => format!(
            "Novelty {score:.2} was measured against {} Recall lookup(s) in {} scope \
             (canon: {}; memories: {}). Recalled entries are context, not proof.",
            memory.lookups(),
            memory.scope(),
            joined_ids(&memory.canon_ids()),
            joined_ids(&memory.memory_ids()),
        ),
        None => format!(
            "Novelty was not measured: memory context was {} ({}). \
             Missing memory is not evidence of novelty or insignificance.",
            memory.status().as_str(),
            memory.reason().unwrap_or("no completed lookup"),
        ),
    }
}

fn deterministic_fields(
    kind: GigaCandidateKind,
    source: &ResolvedSource,
    novelty: Option<f64>,
    memory: &MemoryContext,
) -> (String, String, String) {
    let label = source_label(source);
    let excerpt = bounded_excerpt(&source.text, 1_200);
    let title = format!("{} · {}", kind.as_str(), label);
    let rationale = format!(
        "{}\nExact source {}: {}",
        novelty_line(novelty, memory),
        source.source.source_id(),
        excerpt
    );
    (
        title.chars().take(160).collect(),
        excerpt,
        truncate_with_marker(&rationale, 4 * 1024, "\n[excerpt truncated]"),
    )
}

/// A pointer-only candidate whose one source and proof is the judged target.
pub(super) fn target_candidate(
    event: &GigaEvent,
    target: &ResolvedSource,
    kind: GigaCandidateKind,
    judgment: JevJudgment,
    memory: &MemoryContext,
    configuration_digest: &str,
) -> Result<GigaCandidate, WorkerFailure> {
    // Novelty stands only on a completed lookup; otherwise it stays unmeasured.
    let novelty = judgment
        .novelty
        .filter(|_| memory.status() == MemoryStatus::Complete);
    let source_id = target.source.source_id().to_owned();
    let proof_source_ids = if kind.requires_proof() {
        vec![source_id.clone()]
    } else {
        Vec::new()
    };
    let (title, gist, rationale) = deterministic_fields(kind, target, novelty, memory);
    let completed_at = Utc::now().to_rfc3339();
    let classifier_identity = GigaClassifierIdentity::new(
        judgment.model.clone(),
        "typesafe".into(),
        judgment.model,
        GIGA_PROMPT_VERSION.into(),
        configuration_digest.into(),
        Uuid::new_v4().to_string(),
        completed_at.clone(),
    )
    .map_err(|_| domain_failure())?;
    let scores = GigaScores::new(
        judgment.priority,
        novelty.unwrap_or(0.0),
        judgment.durability,
        judgment.confidence,
    )
    .map_err(|_| domain_failure())?;
    let scope = GigaScope::new(
        Some(event.room().to_string()),
        event.project_keys().first().cloned(),
        GigaVisibility::Private,
        true,
    )
    .map_err(|_| domain_failure())?;
    GigaCandidate::new(
        candidate_id(event, kind, std::slice::from_ref(&source_id))?,
        event.event_id().into(),
        event.room().clone(),
        event.session_id().into(),
        kind,
        vec![target.source.clone()],
        proof_source_ids,
        scores,
        event.project_keys().to_vec(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        title,
        gist,
        rationale,
        scope,
        GigaAuthority::PointerOnly,
        GigaReviewState::Unreviewed,
        classifier_identity,
        completed_at,
        None,
        Vec::new(),
    )
    .map_err(|_| domain_failure())
}

/// Judge every event source as its own target. A canonical event has one.
pub(super) async fn classify_event(
    pool: &PgPool,
    config: &Config,
    event: &GigaEvent,
    sources: &[ResolvedSource],
    classifier: &GigaClassifierConfig,
) -> Result<Vec<GigaCandidate>, WorkerFailure> {
    let configuration_digest = configuration_digest(classifier)?;
    let mut candidates = Vec::new();
    for target in sources {
        let TargetJudgment { judgment, memory } =
            judge_target(pool, config, event, target, classifier).await?;
        if judgment.kind == "none" {
            continue;
        }
        let kind = GigaCandidateKind::parse(&judgment.kind).map_err(|_| domain_failure())?;
        if kind == GigaCandidateKind::ProjectLesson && event.project_keys().len() != 1
            || kind == GigaCandidateKind::CodingLesson && !event.project_keys().is_empty()
        {
            continue;
        }
        candidates.push(target_candidate(
            event,
            target,
            kind,
            judgment,
            &memory,
            &configuration_digest,
        )?);
    }
    Ok(candidates)
}
