mod assembly;
mod cache;
mod identity;
mod lessons;
mod materials;
mod observations;
mod recall;
mod telemetry;

use super::*;
use crate::judgment::{JudgmentRequest, RecallGrantKind};
use cache::{Block, BlockKind, LegacyMemo, SavedSession};
pub(super) use cache::{ContextSession, ContextSessions};
use protocol::organ::{OrganOperation, OrganRequest, OrganTargetScope};
use serde::{Deserialize, Serialize};

const CONTEXT_BUDGET_MS: u64 = if cfg!(windows) { 8_000 } else { 2_000 };
const COMMIT_RESERVE_MS: u64 = 500;

struct AssemblyLifetime {
    cancellation: CancellationToken,
    span: Option<EmitterSpan>,
}

impl Drop for AssemblyLifetime {
    fn drop(&mut self) {
        self.cancellation.cancel();
        end_span(
            self.span.take(),
            OutcomeClass::Cancelled,
            Some("automatic_context_cancelled"),
        );
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ContextPrepareRequest {
    turn_id: String,
    #[serde(skip)]
    operation_id: String,
    prompt: String,
    native_user: bool,
    visible_turn_ids: Vec<String>,
    history: Vec<HistoryTurn>,
    existing_block_kinds: Vec<BlockKind>,
    context_characters: u64,
    user_turn_ordinal: u64,
    fresh_conversation: bool,
    active_project: Option<String>,
    tool_evidence_revision: u64,
    tool_evidence_epoch: String,
    capabilities: Capabilities,
    plan_token: String,
    credential: Option<String>,
    legacy_memo: Option<LegacyMemo>,
    legacy_rejection: Option<LegacyRejection>,
    recall_telemetry_override: Option<String>,
    prior_presence: Option<PriorPresence>,
    previous_request: Option<observations::PreviousRequest>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum LegacyRejection {
    UnpairedSurrogate,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Capabilities {
    automatic_recall: bool,
    top_level: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PriorPresence {
    frame_id: String,
    frame_rendered: String,
    #[serde(default)]
    frame_version: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HistoryTurn {
    kind: HistoryKind,
    text: String,
    turn_id: Option<String>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum HistoryKind {
    User,
    Assistant,
    Generated,
}

pub(super) async fn session(
    state: &AppState,
    session: &str,
) -> tokio::sync::OwnedMutexGuard<ContextSession> {
    let handle = state.context_sessions.lock().await.get(session);
    handle.lock_owned().await
}

pub(super) fn invalidate(
    state: &AppState,
    session: &str,
    context: &mut ContextSession,
) -> Result<(), String> {
    if context.saved.is_none() && !cache::file(state, session).exists() {
        // An empty native file here would prevent later legacy adoption.
        return Ok(());
    }
    let mut saved = match context.saved.take() {
        Some(saved) => saved,
        None => cache::load(state, session, None, &[])?,
    };
    remove_recall(&mut saved);
    let result = cache::commit(state, session, &saved);
    context.saved = Some(saved);
    result
}

fn remove_recall(saved: &mut SavedSession) {
    for turn in &mut saved.turns {
        turn.blocks
            .retain(|block| block.kind != BlockKind::RecallContext);
    }
}

pub(super) async fn lesson_plan(state: &AppState, meta: CommandMeta, raw: Value) -> Responses {
    let result = async {
        let request: lessons::LessonPlanRequest =
            serde_json::from_value(raw).map_err(|error| error.to_string())?;
        if request.native_user && !request.capabilities.top_level {
            return Err("nativeUser requires a top-level context".into());
        }
        if request.turn_id.trim().is_empty() {
            return Err("lesson plan requires a turn identity".into());
        }
        let deadline = request.deadline.min(now_ms() + CONTEXT_BUDGET_MS);
        let mut plan = bounded(deadline, lessons::select(state, &meta, request)).await?;
        let mut context = session(state, &meta.sender_session).await;
        if context.saved.is_none() {
            if let Ok(saved) = cache::load(state, &meta.sender_session, None, &[]) {
                if saved.identity_epoch.is_some() {
                    context.saved = Some(saved);
                }
            }
        }
        if let Some(saved) = context.saved.as_ref() {
            if identity::replay_eligible(state, &meta, saved, &plan.turn_id)
                .await
                .unwrap_or(false)
            {
                plan.requires_credential = false;
            }
        }
        let result = serde_json::to_value(&plan).map_err(|error| error.to_string())?;
        context.plans.retain(|_, plan| plan.deadline > now_ms());
        context.plans.insert(plan.token.clone(), plan);
        Ok(result)
    }
    .await;
    response(state, &meta, "athanor.context.lesson_planned", result).await
}

pub(super) async fn prepare(state: &AppState, meta: CommandMeta, raw: Value) -> Responses {
    let result = prepare_inner(state, &meta, raw).await;
    response(state, &meta, "athanor.context.prepared", result).await
}

async fn prepare_inner(state: &AppState, meta: &CommandMeta, raw: Value) -> Result<Value, String> {
    let mut request: ContextPrepareRequest =
        serde_json::from_value(raw).map_err(|error| error.to_string())?;
    if request.native_user && !request.capabilities.top_level {
        return Err("nativeUser requires a top-level context".into());
    }
    if request.legacy_memo.is_some() && request.legacy_rejection.is_some() {
        return Err("a legacy proposal cannot also be a rejection".into());
    }
    if request.turn_id.trim().is_empty() || !request.visible_turn_ids.contains(&request.turn_id) {
        return Err("context turn identity is absent from visible turns".into());
    }
    let mut context = session(state, &meta.sender_session).await;
    context.plans.retain(|token, plan| {
        token == &request.plan_token
            || (request.visible_turn_ids.contains(&plan.turn_id) && plan.deadline > now_ms())
    });
    let plan = context
        .plans
        .get(&request.plan_token)
        .filter(|plan| plan.turn_id == request.turn_id)
        .cloned()
        .ok_or("context lesson plan is absent or belongs to another turn")?;
    let compacted_before_adoption = if context.saved.is_none()
        && request.legacy_memo.is_some()
        && !cache::file(state, &meta.sender_session).exists()
    {
        state
            .runtime
            .lock()
            .await
            .sessions
            .get(&meta.sender_session)
            .is_some_and(RecallPolicySession::has_pending_recovery)
    } else {
        false
    };
    let saved = match context.saved.as_ref() {
        Some(saved) => saved.clone(),
        None => cache::load(
            state,
            &meta.sender_session,
            request.legacy_memo.take(),
            &request.visible_turn_ids,
        )?,
    };
    let mut saved = saved;
    if compacted_before_adoption {
        remove_recall(&mut saved);
    }
    saved
        .turns
        .retain(|turn| request.visible_turn_ids.contains(&turn.turn_id));
    let identity = bounded(
        plan.deadline,
        identity::synchronize(state, meta, &mut context, &mut saved, &mut request),
    )
    .await?;
    request.operation_id = format!("{}:{}", saved.generation, request.turn_id);
    if saved
        .turns
        .iter()
        .any(|turn| turn.turn_id == request.turn_id)
    {
        let report_coverage = coverage(state).await;
        let identity_fence = bounded(plan.deadline, identity::fence(state, &saved)).await?;
        let (pending, settled) =
            settlement_fields(&identity_fence, meta, &saved, &request.turn_id)?;
        let result = json!({"turns":saved.turns,"replayed":true,"activities":[],"warnings":[],"nativeOwned":cache::file(state,&meta.sender_session).exists(),"adoption":identity.adoption,"adoptionRejection":request.legacy_rejection,"invalidationReason":identity.invalidation,"coverage":report_coverage,"presenceSettlement":pending,"presenceSettledContractId":settled});
        context.saved = Some(saved);
        return Ok(result);
    }
    let mut lifetime = AssemblyLifetime {
        cancellation: CancellationToken::new(),
        span: start_span(
            state.insula_binding.as_ref(),
            "host",
            "host",
            "context_assembly",
        ),
    };
    let assembled = bounded(
        plan.deadline,
        assembly::assemble(
            state,
            meta,
            &request,
            &plan,
            &mut saved,
            &mut context,
            lifetime.cancellation.clone(),
        ),
    )
    .await;
    match assembled {
        Ok(mut result) => {
            let report_coverage = coverage(state).await;
            let identity_fence = bounded(plan.deadline, identity::fence(state, &saved)).await?;
            if now_ms() >= plan.deadline {
                end_span(
                    lifetime.span.take(),
                    OutcomeClass::Timeout,
                    Some("automatic_context_timeout"),
                );
                return Err("automatic context deadline elapsed before commit".into());
            }
            cache::merge(
                &mut saved,
                &request.turn_id,
                std::mem::take(&mut result.blocks),
            );
            let (pending, settled) =
                settlement_fields(&identity_fence, meta, &saved, &request.turn_id)?;
            let durable = cache::commit(state, &meta.sender_session, &saved).is_ok();
            if !durable {
                result
                    .warnings
                    .push("native context memo durability degraded".into());
            }
            let outcome = if result.warnings.is_empty() {
                OutcomeClass::Ok
            } else {
                OutcomeClass::Degraded
            };
            let output = json!({"turns":saved.turns,"replayed":false,"activities":result.activities,"warnings":result.warnings,"presenceSettlement":pending,"presenceSettledContractId":settled,"nativeOwned":durable,"adoption":identity.adoption,"adoptionRejection":request.legacy_rejection,"invalidationReason":identity.invalidation,"coverage":report_coverage});
            context.saved = Some(saved);
            if now_ms() >= plan.deadline {
                end_span(
                    lifetime.span.take(),
                    OutcomeClass::Timeout,
                    Some("automatic_context_timeout"),
                );
                return Err(
                    "automatic context deadline elapsed; complete native memo retained for replay"
                        .into(),
                );
            }
            end_span(lifetime.span.take(), outcome, None);
            Ok(output)
        }
        Err(error) => {
            end_span(
                lifetime.span.take(),
                OutcomeClass::Timeout,
                Some("automatic_context_timeout"),
            );
            Err(error)
        }
    }
}

fn settlement_fields(
    runtime: &super::RuntimeState,
    meta: &CommandMeta,
    saved: &SavedSession,
    turn_id: &str,
) -> Result<(Option<Value>, Option<String>), String> {
    let Some(block) = saved
        .turns
        .iter()
        .find(|turn| turn.turn_id == turn_id)
        .and_then(|turn| {
            turn.blocks
                .iter()
                .find(|block| block.kind == BlockKind::PresenceContext)
        })
    else {
        return Ok((None, None));
    };
    let contract_id = text(&block.details["contractId"]);
    match runtime
        .presence
        .settlement_state(&meta.sender_session, contract_id)
    {
        Some(crate::presence::SettlementState::Settled(id)) => Ok((None, Some(id.to_owned()))),
        Some(crate::presence::SettlementState::Pending(contract)) => {
            let mut directives = Vec::new();
            for directive in contract
                .must_enact
                .iter()
                .chain(&contract.must_avoid)
                .chain(&contract.guards)
            {
                if directive.severity == summoning::presence::PresenceSeverity::Hard
                    && !directives.contains(&directive.id.as_str())
                {
                    directives.push(directive.id.as_str());
                }
            }
            let nonempty = contract.guards.iter().any(|guard| {
                guard.id == "presence:nonempty-response"
                    && guard.severity == summoning::presence::PresenceSeverity::Hard
            });
            Ok((
                Some(json!({
                    "contractId": contract.contract_id,
                    "directiveIds": directives,
                    "nonemptyGuardId": nonempty.then_some("presence:nonempty-response"),
                })),
                None,
            ))
        }
        None => Err("cached Presence contract is no longer active".into()),
    }
}

async fn response(
    state: &AppState,
    meta: &CommandMeta,
    kind: &str,
    result: Result<Value, String>,
) -> Responses {
    let result = match result {
        Ok(result) => result,
        Err(reason) => {
            return Responses {
                direct: vec![serialize(
                    &outcome(state, meta, RECALL_POLICY_COMMAND_FAILED, Some(reason)).await,
                )],
                delta: None,
            };
        }
    };
    let sequence = state.runtime.lock().await.cursor.sequence;
    let metadata = event_meta_for_projection(
        state,
        Some(meta),
        &meta.message_id,
        &meta.idempotency_key,
        kind,
        CONTEXT_PROJECTION_ID,
        sequence,
        body_hash(&result).expect("context result hashes"),
        new_id(),
    );
    let mut event = serde_json::to_value(metadata).expect("context event metadata serializes");
    event["result"] = result;
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

async fn organ(
    state: &AppState,
    meta: &CommandMeta,
    operation: OrganOperation,
    params: Value,
) -> Result<Value, protocol::ProtocolErrorBody> {
    let params = params.as_object().cloned().ok_or_else(|| {
        protocol::ProtocolErrorBody::application(
            "invalid_params",
            "organ parameters must be an object",
        )
        .build()
    })?;
    match crate::organ::call(
        &state.config,
        state.hallway_pool.as_ref(),
        meta,
        OrganRequest {
            operation,
            params,
            target_scope: OrganTargetScope::Room,
        },
        &state.giga,
    )
    .await
    {
        protocol::ResponsePayload::Result { result } => Ok(result),
        protocol::ResponsePayload::Error { error } => Err(error),
    }
}

fn child_meta(meta: &CommandMeta, turn: &str, stage: &str, projection: &str) -> CommandMeta {
    let mut child = meta.clone();
    child.idempotency_key = if stage.is_empty() {
        turn.into()
    } else {
        format!("{turn}:{stage}")
    };
    child.message_id = new_id();
    child.correlation_id = child.message_id.clone();
    child.causation_id = meta.message_id.clone();
    child.projection_id = projection.into();
    child
}

async fn command(state: &AppState, command: ClientCommand, hash: Value) -> Result<Value, String> {
    super::validate_command(state, &command)?;
    let response = super::execute_service_command(state, command, body_hash(&hash)?).await;
    if let Some(delta) = response.delta {
        let _ = state.deltas.send(delta);
    }
    let mut answer = None;
    for raw in response.direct {
        let value: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let kind = text(&value["command_or_event_type"]);
        if kind.ends_with("command_failed") || kind.ends_with("command_refused") {
            return Err(text(&value["reason"]).to_owned());
        }
        answer = Some(value);
    }
    answer.ok_or_else(|| "native context operation returned no result".into())
}

async fn judge(state: &AppState, request: JudgmentRequest) -> Result<Value, String> {
    let grant = match &request {
        JudgmentRequest::RecallRerank {
            grant: RecallGrantKind::Recall,
            ..
        } => Some("recall"),
        JudgmentRequest::RecallRerank {
            grant: RecallGrantKind::Lessons,
            ..
        } => Some("lessons"),
        _ => None,
    };
    let mut result =
        crate::judgment::execute(&state.config.room_dir, &state.config.room, request).await;
    if let Some(grant) = grant {
        if let Ok(result) = &mut result {
            result["receipt"] = observations::receipt(&result["receipt"]);
        }
        let mut sessions = state.context_sessions.lock().await;
        let counts = sessions
            .coverage
            .entry(grant.into())
            .or_insert_with(empty_coverage);
        counts["turns"] = json!(counts["turns"].as_u64().unwrap_or_default() + 1);
        if let Ok(result) = &result {
            let receipt = &result["receipt"];
            let status = text(&receipt["status"]);
            if counts.get(status).and_then(Value::as_u64).is_some() {
                counts[status] = json!(counts[status].as_u64().unwrap_or_default() + 1);
            }
            if status != "disabled" {
                counts["approved"] = json!(counts["approved"].as_u64().unwrap_or_default() + 1);
            }
            counts["selected"] = json!(
                counts["selected"].as_u64().unwrap_or_default()
                    + receipt["selected"].as_u64().unwrap_or_default()
            );
            counts["last"] = receipt.clone();
        } else {
            counts["failed"] = json!(counts["failed"].as_u64().unwrap_or_default() + 1);
        }
    }
    result
}

fn empty_coverage() -> Value {
    json!({"turns":0,"approved":0,"disabled":0,"refused":0,"unavailable":0,"failed":0,"shadow":0,"active":0,"selected":0,"last":null})
}

async fn coverage(state: &AppState) -> Value {
    let sessions = state.context_sessions.lock().await;
    json!({
        "recall":sessions.coverage.get("recall").cloned().unwrap_or_else(empty_coverage),
        "lessons":sessions.coverage.get("lessons").cloned().unwrap_or_else(empty_coverage)
    })
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn judge_deadline(deadline: u64) -> Option<u64> {
    let now = now_ms();
    let available = deadline.saturating_sub(now + COMMIT_RESERVE_MS + 100);
    (available > 0).then_some(now + available.min(1_500))
}

async fn bounded<T>(
    deadline: u64,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    if now_ms() >= deadline {
        return Err("automatic context deadline elapsed".into());
    }
    tokio::time::timeout(
        Duration::from_millis(deadline.saturating_sub(now_ms())),
        future,
    )
    .await
    .map_err(|_| "automatic context deadline elapsed".to_owned())?
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}
fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
fn clip(text: &str, limit: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= limit
        })
        .collect()
}

#[cfg(test)]
mod tests;
