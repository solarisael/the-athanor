use crate::chat::ChatLog;
use crate::config::{HOST_RECIPIENT, HostConfig};
use crate::insula::InsulaHost;
use crate::panel::PanelHost;
use crate::policy::{RecallPolicySession, apply_requested_mode};
use crate::presence::{PresenceRuntime, host_capabilities};
use crate::receipt::{ReceiptIngest, ReceiptTracker};
use crate::routing::{
    DispatchRequest, LoadedSpellbook, SpellbookRead, familiar_status, house_dispatch, lane_status,
    load_spellbook,
};
use crate::store::{
    DurableReceipt, HostDurableStore, ProjectionCursor, RoomIdentity, RoomStateStore, body_hash,
    state_hash, timestamp,
};
use crate::viewport::{ViewportSession, apply_viewport};
use akasha::insula_writer::{EmitterSpan, defer_span, end_span, record_point, start_span};
use akasha::{
    AppError, Config as SubstrateConfig, LessonFamily, LessonQueryParams, OutcomeClass,
    TrustedBinding, hallway_inbox, hallway_knock_claim, hallway_knock_settle, lesson_query, recall,
    validate_trusted_binding,
};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use hearth::context::analyze_context;
use hearth::conversation::{
    ConversationTurn, conversation_turns, is_fresh_conversation, logged_turn,
    source_ledger_directory, source_ledger_entry, source_ledger_path, transcript_debug_path,
    transcript_entry, transcript_path, turn_key, turn_label, turn_marker,
};
use hearth::hallway::{HallwayInboxRequest, HallwayKnockClaimRequest, HallwayKnockSettleRequest};
use hearth::lineage::{QuestLifecycle, normalize_lifecycle_memory, normalize_quest_memories};
use hearth::triggers::{process_lesson_plan, process_lesson_reminder};
use origami::cranes::broker::{
    ACK_WAIT, MAX_ACK_PENDING, MAX_BATCH, MAX_DELIVER, MAX_EXPIRES, NUM_REPLICAS,
    RECEIPT_STREAM_NAME, RECEIPT_SUBJECT,
};
use protocol::{
    AKASHA_COMMAND_FAILED, AKASHA_LESSON_RESULT, AKASHA_PROJECTION_ID, AKASHA_RECALL_RESULT,
    AkashaLessonFamily, AkashaLessonQueryPayload, AkashaLessonResultEvent,
    AkashaRecallQueryPayload, AkashaRecallResultEvent, CHAT_COMMAND_ACCEPTED, CHAT_COMMAND_REFUSED,
    CHAT_DELTA, CHAT_PROJECTION_ID, CHAT_SNAPSHOT, CHAT_SUBSCRIBE, CONTEXT_ANALYZED,
    CONTEXT_PROJECTION_ID, CONTEXT_VIEWPORTED, ChatEvent, ChatMessage, ClientCommand, CommandMeta,
    CommandOutcomeEvent, ContextAnalysisEvent, ContextViewportEvent, ConversationLogRequest,
    DEFAULT_HOST_WS_PATH, DeltaEvent, EventMeta, HALLWAY_INBOX_PROJECTED, HALLWAY_KNOCK_CLAIMED,
    HALLWAY_KNOCK_COMMAND_FAILED, HALLWAY_KNOCK_COMMAND_REFUSED, HALLWAY_KNOCK_SETTLED,
    HALLWAY_PROJECTION_ID, HOST_SCHEMA_VERSION, HallwayInboxProjectionEvent,
    HallwayKnockClaimedEvent, HallwayKnockSettledEvent, LINEAGE_NORMALIZED, LINEAGE_PROJECTION_ID,
    LineageResultEvent, PAPER_BOAT_RECEIPT_PROJECTION_ID, PAPER_BOAT_RECEIPT_SNAPSHOT,
    PAPER_BOAT_RECEIPT_SUBSCRIBE, PRESENCE_CLOSED, PRESENCE_COMMAND_REFUSED, PRESENCE_COMPILED,
    PRESENCE_OPENED, PRESENCE_PROJECTION_ID, PRESENCE_SETTLED, PaperBoatReceiptEvent,
    PaperBoatReceiptState, PresenceResultEvent, RECALL_POLICY_COMMAND_ACCEPTED,
    RECALL_POLICY_COMMAND_FAILED, RECALL_POLICY_COMMAND_REFUSED, RECALL_POLICY_DELTA,
    RECALL_POLICY_PROJECTION_ID, RECALL_POLICY_SNAPSHOT, RECALL_POLICY_SUBSCRIBE,
    ROUTING_PROJECTION_ID, ROUTING_RESULT, RecallParams, RecallPolicyDecision,
    RecallPolicyMutation, RecallPolicyState, RoutingResultEvent, SHELL_PROJECTION_ID, SHELL_RESULT,
    ShellResultEvent, SnapshotEvent, parse_client_command,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use summoning::presence::{
    PresenceAuthentication, PresenceBinding, PresenceCloseRequest, PresenceOpenRequest,
    PresenceResult, PresenceSettleRequest, PresenceTurnRequest,
};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

// Idle doormen report liveness once per Host, not once per session or poll.
// There is no poll-count column: bytes/tokens/drop_count retain their units;
// Vitals event_count measures these five-minute heartbeat points instead.
const KNOCK_POLL_OBSERVATION_WINDOW: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct KnockPollObservations {
    last_summary: Option<Instant>,
    was_nonempty: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum KnockPollObservation {
    ClaimSpan,
    PollPoint,
    Quiet,
}
// A cancelled claim may already have acquired a candidate. Only a completed
// empty result may discard its deferred span; cancellation remains visible.
struct PendingKnockObservation(Option<EmitterSpan>);

impl Drop for PendingKnockObservation {
    fn drop(&mut self) {
        end_span(self.0.take(), OutcomeClass::Error, Some("claim_aborted"));
    }
}

impl KnockPollObservations {
    fn observe(&mut self, nonempty: bool, now: Instant) -> KnockPollObservation {
        let changed_to_empty = self.was_nonempty && !nonempty;
        self.was_nonempty = nonempty;
        if nonempty || changed_to_empty {
            return KnockPollObservation::ClaimSpan;
        }
        if self
            .last_summary
            .is_none_or(|last| now.saturating_duration_since(last) >= KNOCK_POLL_OBSERVATION_WINDOW)
        {
            self.last_summary = Some(now);
            KnockPollObservation::PollPoint
        } else {
            KnockPollObservation::Quiet
        }
    }
}

struct RuntimeState {
    projection: RecallPolicyState,
    sessions: HashMap<String, RecallPolicySession>,
    presence: PresenceRuntime,
    viewport_sessions: HashMap<String, ViewportSession>,
    hallway_inbox_fingerprints: HashMap<String, String>,
    knock_poll: KnockPollObservations,
    chat: ChatLog,
    cursor: ProjectionCursor,
    durable: HostDurableStore,
}

#[derive(Clone)]
struct AppState {
    config: Arc<HostConfig>,
    room_store: RoomStateStore,
    runtime: Arc<Mutex<RuntimeState>>,
    hallway_pool: Option<PgPool>,
    insula_binding: Arc<TrustedBinding>,
    insula: InsulaHost,
    panel: PanelHost,
    cancellation: CancellationToken,
    tasks: TaskTracker,
    deltas: broadcast::Sender<String>,
    receipts: broadcast::Sender<String>,
    chat_deltas: broadcast::Sender<String>,
    receipt_tracker: Arc<Mutex<ReceiptTracker>>,
}

fn host_insula_binding(config: &HostConfig) -> Result<TrustedBinding, String> {
    let binding = TrustedBinding {
        house_id: config.house_id.clone(),
        room: config.room.clone(),
        spirit: config.spirit.clone(),
        session_id: format!("host:{}", config.room),
    };
    validate_trusted_binding(&binding).map_err(|error| error.to_string())?;
    Ok(binding)
}

pub(crate) struct Host {
    state: AppState,
}

impl Host {
    // [host/pool/shared] [host/state/room] [host/lifetime/shutdown]
    pub(crate) fn new(
        config: HostConfig,
        pool: Option<PgPool>,
        cancellation: CancellationToken,
        tasks: TaskTracker,
    ) -> Result<Self, String> {
        let insula_binding = Arc::new(host_insula_binding(&config)?);
        let insula = InsulaHost::new(&config, pool.clone(), insula_binding.clone())?;
        let panel = PanelHost::new(&config, pool.clone(), insula_binding.clone());
        let room_store = RoomStateStore::new(config.room_state_path(), config.room.clone());
        let projection = room_store.load()?;
        let (durable, cursor, mut sessions) =
            HostDurableStore::open(&config.state_dir, &projection)?;
        if sessions.is_empty() {
            sessions.insert(
                config.session.clone(),
                RecallPolicySession::from_projection(&projection),
            );
        }
        let (deltas, _) = broadcast::channel(64);
        let (receipts, _) = broadcast::channel(64);
        let (chat_deltas, _) = broadcast::channel(64);
        let receipt_tracker = Arc::new(Mutex::new(ReceiptTracker::new(
            config.akasha_enabled(),
            config.nats_url.is_some(),
        )));
        Ok(Self {
            state: AppState {
                config: Arc::new(config),
                room_store,
                runtime: Arc::new(Mutex::new(RuntimeState {
                    projection,
                    sessions,
                    presence: PresenceRuntime::default(),
                    viewport_sessions: HashMap::new(),
                    hallway_inbox_fingerprints: HashMap::new(),
                    knock_poll: KnockPollObservations::default(),
                    chat: ChatLog::default(),
                    cursor,
                    durable,
                })),
                hallway_pool: pool,
                insula,
                panel,
                insula_binding,
                cancellation,
                tasks,
                deltas,
                receipts,
                chat_deltas,
                receipt_tracker,
            },
        })
    }

    pub(crate) fn spawn_receipt_bridge(&self) {
        if let Some(url) = self.state.config.nats_url.clone() {
            self.state
                .tasks
                .spawn(run_receipt_bridge(self.state.clone(), url));
        }
    }

    // [host/routing] [security/auth]
    pub(crate) fn router(&self) -> Router {
        Router::new()
            .route("/health", get(health))
            .route(DEFAULT_HOST_WS_PATH, get(upgrade))
            .with_state(self.state.clone())
            .merge(self.state.insula.router())
            .merge(self.state.panel.router())
    }
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let runtime = state.runtime.lock().await;
    let receipt_health = state.receipt_tracker.lock().await.health();
    Json(json!({
        "status": "ok",
        "schema_version": HOST_SCHEMA_VERSION,
        "websocket_path": format!("{}{DEFAULT_HOST_WS_PATH}", state.config.room_path()),
        "projection_id": RECALL_POLICY_PROJECTION_ID,
        "version": runtime.cursor.version,
        "sequence": runtime.cursor.sequence,
        "state_hash": runtime.cursor.state_hash,
        "akasha_delivery": receipt_health,
        "insula": state.insula.health(),
    }))
}

async fn upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    if !authorized(&headers, &state.config.bearer_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "unauthenticated",
                "diagnostic": "Authorization: Bearer <token> is required"
            })),
        )
            .into_response();
    }
    let tracker = state.tasks.clone();
    websocket
        .on_upgrade(move |socket| async move {
            let task = tracker.spawn(handle_socket(socket, state));
            let _ = task.await;
        })
        .into_response()
}

pub(crate) fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some(candidate) = value.strip_prefix("Bearer ") else {
        return false;
    };
    constant_time_equal(candidate.as_bytes(), expected.as_bytes())
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let longest = left.len().max(right.len());
    for index in 0..longest {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sink, mut source) = socket.split();
    let mut deltas = state.deltas.subscribe();
    let mut receipts = state.receipts.subscribe();
    let mut recall_subscribed = false;
    let mut receipts_subscribed = false;
    let mut chat_deltas = state.chat_deltas.subscribe();
    let mut chat_subscribed = false;
    loop {
        tokio::select! {
            _ = state.cancellation.cancelled() => {
                let _ = sink.send(Message::Close(None)).await;
                return;
            }
            broadcast = deltas.recv(), if recall_subscribed => {
                match broadcast {
                    Ok(text) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let text = diagnostic_refusal(&state, "projection delta stream lagged; resync required").await;
                        let _ = sink.send(Message::Text(text.into())).await;
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            broadcast = receipts.recv(), if receipts_subscribed => {
                match broadcast {
                    Ok(text) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => return,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            broadcast = chat_deltas.recv(), if chat_subscribed => {
                match broadcast {
                    Ok(text) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let text = diagnostic_refusal(&state, "chat delta stream lagged; resubscribe required").await;
                        let _ = sink.send(Message::Text(text.into())).await;
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            incoming = source.next() => {
                let Some(incoming) = incoming else { return; };
                match incoming {
                    Ok(Message::Text(text)) => {
                        let requested = serde_json::from_str::<Value>(text.as_str())
                            .ok()
                            .and_then(|value| value.get("command_or_event_type").and_then(Value::as_str).map(str::to_owned));
                        let responses = process_text(&state, text.as_str()).await;
                        if requested.as_deref() == Some(RECALL_POLICY_SUBSCRIBE)
                            && contains_event(&responses.direct, RECALL_POLICY_SNAPSHOT)
                        {
                            recall_subscribed = true;
                        }
                        if requested.as_deref() == Some(PAPER_BOAT_RECEIPT_SUBSCRIBE)
                            && contains_event(&responses.direct, PAPER_BOAT_RECEIPT_SNAPSHOT)
                        {
                            receipts_subscribed = true;
                        }
                        if requested.as_deref() == Some(CHAT_SUBSCRIBE)
                            && contains_event(&responses.direct, CHAT_SNAPSHOT)
                        {
                            chat_subscribed = true;
                        }
                        for response in responses.direct {
                            if sink.send(Message::Text(response.into())).await.is_err() {
                                return;
                            }
                        }
                        if let Some(delta) = responses.delta {
                            let _ = state.deltas.send(delta);
                        }
                    }
                    Ok(Message::Close(_)) => return,
                    Ok(Message::Ping(payload)) => {
                        if sink.send(Message::Pong(payload)).await.is_err() { return; }
                    }
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Binary(_)) => {
                        let text = diagnostic_refusal(&state, "binary WebSocket messages are not accepted").await;
                        if sink.send(Message::Text(text.into())).await.is_err() { return; }
                    }
                    Err(_) => return,
                }
            }
        }
    }
}

fn contains_event(events: &[String], event_type: &str) -> bool {
    events.iter().any(|event| {
        serde_json::from_str::<Value>(event)
            .ok()
            .and_then(|value| {
                value
                    .get("command_or_event_type")
                    .and_then(Value::as_str)
                    .map(|kind| kind == event_type)
            })
            .unwrap_or(false)
    })
}

fn semantic_command_hash(raw: &Value) -> Result<String, String> {
    let mut semantic = raw.clone();
    let object = semantic
        .as_object_mut()
        .ok_or_else(|| "command envelope must be a JSON object".to_string())?;
    for field in [
        "message_id",
        "correlation_id",
        "causation_id",
        "reply_target",
        "idempotency_key",
        "created_at",
        "expires_at",
        "max_hops",
    ] {
        object.remove(field);
    }
    body_hash(&semantic)
}

struct Responses {
    direct: Vec<String>,
    delta: Option<String>,
}

async fn process_text(state: &AppState, text: &str) -> Responses {
    let raw: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return Responses {
                direct: vec![
                    diagnostic_refusal(state, &format!("malformed JSON command: {error}")).await,
                ],
                delta: None,
            };
        }
    };
    let command_hash = match semantic_command_hash(&raw) {
        Ok(hash) => hash,
        Err(reason) => {
            return Responses {
                direct: vec![diagnostic_refusal(state, &reason).await],
                delta: None,
            };
        }
    };
    let command = match parse_client_command(raw) {
        Ok(command) => command,
        Err(error) => {
            let event = outcome_for_ids(
                state,
                &error.message_id,
                &error.idempotency_key,
                RECALL_POLICY_COMMAND_REFUSED,
                Some(error.reason),
            )
            .await;
            return Responses {
                direct: vec![serialize(&event)],
                delta: None,
            };
        }
    };
    if let Err(reason) = validate_command(state, &command) {
        let event = outcome(
            state,
            command.meta(),
            RECALL_POLICY_COMMAND_REFUSED,
            Some(reason),
        )
        .await;
        return Responses {
            direct: vec![serialize(&event)],
            delta: None,
        };
    }
    match command {
        ClientCommand::PaperBoatReceiptSubscribe { meta } => {
            let receipt_state = state.receipt_tracker.lock().await.state();
            let snapshot = receipt_snapshot(state, Some(&meta), receipt_state);
            Responses {
                direct: vec![serialize(&snapshot)],
                delta: None,
            }
        }
        ClientCommand::Subscribe { meta } | ClientCommand::Resync { meta } => {
            let mut runtime = state.runtime.lock().await;
            if let Err(reason) = refresh_from_room_state(state, &mut runtime) {
                let failed = outcome_with_runtime(
                    state,
                    &meta,
                    RECALL_POLICY_COMMAND_FAILED,
                    Some(reason),
                    &runtime,
                    None,
                );
                return Responses {
                    direct: vec![serialize(&failed)],
                    delta: None,
                };
            }
            let snapshot = snapshot(state, &meta, &runtime);
            Responses {
                direct: vec![serialize(&snapshot)],
                delta: None,
            }
        }
        ClientCommand::Acknowledge {
            meta,
            version,
            sequence,
        } => {
            let runtime = state.runtime.lock().await;
            let (kind, reason) = if version == runtime.cursor.version
                && sequence == runtime.cursor.sequence
            {
                (RECALL_POLICY_COMMAND_ACCEPTED, None)
            } else {
                (
                    RECALL_POLICY_COMMAND_REFUSED,
                    Some(format!(
                        "stale acknowledgement version/sequence {version}/{sequence}; current is {}/{}",
                        runtime.cursor.version, runtime.cursor.sequence
                    )),
                )
            };
            let event = outcome_with_runtime(state, &meta, kind, reason, &runtime, None);
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        ClientCommand::SetRequestedMode {
            meta,
            base_version,
            requested_mode,
        } => set_requested_mode(state, meta, base_version, requested_mode, command_hash).await,
        ClientCommand::Evaluate { meta, facts } => {
            let mut runtime = state.runtime.lock().await;
            if let Some(response) = idempotency_response(state, &runtime, &meta, &command_hash) {
                return response;
            }
            let requested_mode = runtime.projection.requested_mode;
            let mut session = runtime
                .sessions
                .get(&meta.sender_session)
                .cloned()
                .unwrap_or_else(|| RecallPolicySession::fresh(&runtime.projection));
            let decision = session.evaluate(requested_mode, facts);
            let next = session.projection(timestamp());
            runtime
                .sessions
                .insert(meta.sender_session.clone(), session);
            commit_change(
                state,
                &mut runtime,
                &meta,
                command_hash,
                next,
                Some(decision),
            )
        }
        ClientCommand::CompleteRefresh { meta, refresh } => {
            let mut runtime = state.runtime.lock().await;
            if let Some(response) = idempotency_response(state, &runtime, &meta, &command_hash) {
                return response;
            }
            let mut session = runtime
                .sessions
                .get(&meta.sender_session)
                .cloned()
                .unwrap_or_else(|| RecallPolicySession::fresh(&runtime.projection));
            session.complete_refresh(refresh, timestamp());
            let next = session.projection(timestamp());
            runtime
                .sessions
                .insert(meta.sender_session.clone(), session);
            commit_change(state, &mut runtime, &meta, command_hash, next, None)
        }
        ClientCommand::FailRefresh { meta, reason } => {
            let mut runtime = state.runtime.lock().await;
            if let Some(response) = idempotency_response(state, &runtime, &meta, &command_hash) {
                return response;
            }
            let mut session = runtime
                .sessions
                .get(&meta.sender_session)
                .cloned()
                .unwrap_or_else(|| RecallPolicySession::fresh(&runtime.projection));
            session.fail_refresh(&reason, timestamp());
            let next = session.projection(timestamp());
            runtime
                .sessions
                .insert(meta.sender_session.clone(), session);
            commit_change(state, &mut runtime, &meta, command_hash, next, None)
        }
        ClientCommand::InvalidateAfterCompaction { meta, summary } => {
            let mut runtime = state.runtime.lock().await;
            if let Some(response) = idempotency_response(state, &runtime, &meta, &command_hash) {
                return response;
            }
            let mut session = runtime
                .sessions
                .get(&meta.sender_session)
                .cloned()
                .unwrap_or_else(|| RecallPolicySession::fresh(&runtime.projection));
            session.invalidate_after_compaction(&summary);
            runtime.viewport_sessions.remove(&meta.sender_session);
            let next = session.projection(timestamp());
            runtime
                .sessions
                .insert(meta.sender_session.clone(), session);
            commit_change(state, &mut runtime, &meta, command_hash, next, None)
        }
        ClientCommand::AnalyzeContext { meta, request } => {
            let mut runtime = state.runtime.lock().await;
            let last_nudge_band = runtime
                .viewport_sessions
                .get(&meta.sender_session)
                .map(ViewportSession::last_nudge_band)
                .unwrap_or_default();
            let analysis = match analyze_context(&meta.sender_room, request, last_nudge_band) {
                Ok(analysis) => analysis,
                Err(error) => {
                    let failed = outcome_with_runtime(
                        state,
                        &meta,
                        RECALL_POLICY_COMMAND_FAILED,
                        Some(error.to_string()),
                        &runtime,
                        None,
                    );
                    return Responses {
                        direct: vec![serialize(&failed)],
                        delta: None,
                    };
                }
            };
            if let Some(nudge) = &analysis.nudge {
                runtime
                    .viewport_sessions
                    .entry(meta.sender_session.clone())
                    .or_default()
                    .set_last_nudge_band(nudge.band);
            }
            let event = ContextAnalysisEvent {
                meta: event_meta_for_projection(
                    state,
                    Some(&meta),
                    &meta.message_id,
                    &meta.idempotency_key,
                    CONTEXT_ANALYZED,
                    CONTEXT_PROJECTION_ID,
                    runtime.cursor.sequence,
                    body_hash(
                        &serde_json::to_value(&analysis).expect("context analysis serializes"),
                    )
                    .expect("context analysis hashes"),
                    new_id(),
                ),
                analysis,
            };
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        ClientCommand::ApplyRecallViewport { meta, result, mode } => {
            let mut runtime = state.runtime.lock().await;
            let viewport = apply_viewport(
                result,
                runtime
                    .viewport_sessions
                    .entry(meta.sender_session.clone())
                    .or_default(),
                mode,
            );
            let event = ContextViewportEvent {
                meta: event_meta_for_projection(
                    state,
                    Some(&meta),
                    &meta.message_id,
                    &meta.idempotency_key,
                    CONTEXT_VIEWPORTED,
                    CONTEXT_PROJECTION_ID,
                    runtime.cursor.sequence,
                    body_hash(
                        &serde_json::to_value(&viewport).expect("viewport result serializes"),
                    )
                    .expect("viewport result hashes"),
                    new_id(),
                ),
                result: viewport,
            };
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        ClientCommand::ProjectHallwayInbox { meta } => project_hallway_inbox(state, meta).await,
        ClientCommand::ClaimHallwayKnock { meta } => claim_hallway_knock(state, meta).await,
        ClientCommand::SettleHallwayKnock { meta, request } => {
            settle_hallway_knock(state, meta, request).await
        }
        ClientCommand::AkashaRecallQuery { meta, payload } => {
            query_akasha_recall(state, meta, payload).await
        }
        ClientCommand::AkashaLessonQuery { meta, payload } => {
            query_akasha_lessons(state, meta, payload).await
        }
        ClientCommand::PresenceOpen { meta, request } => {
            open_presence_frame(state, meta, request).await
        }
        ClientCommand::PresenceCompile { meta, request } => {
            compile_presence_turn(state, meta, request).await
        }
        ClientCommand::PresenceSettle { meta, request } => {
            settle_presence_turn(state, meta, request).await
        }
        ClientCommand::PresenceClose { meta, request } => {
            close_presence_frame(state, meta, request).await
        }
        ClientCommand::RoutingStatus { meta } => {
            routing_response(
                state,
                meta,
                serde_json::to_value(lane_status()).expect("lane status serializes"),
            )
            .await
        }
        ClientCommand::RoutingDispatch {
            meta,
            room_dir,
            request,
        } => {
            let result = match resolve_room_dir(room_dir.as_deref(), &state.config) {
                Ok(room_dir) => match serde_json::from_value::<DispatchRequest>(request) {
                    Ok(request) => serde_json::to_value(house_dispatch(request, || {
                        read_room_spellbook(room_dir.as_ref())
                    }))
                    .expect("dispatch receipt serializes"),
                    Err(error) => invalid_routing_request("routing", error),
                },
                Err(rejection) => rejection,
            };
            routing_response(state, meta, result).await
        }
        ClientCommand::FamiliarStatus { meta, room_dir } => {
            let result = match resolve_room_dir(room_dir.as_deref(), &state.config) {
                Ok(room_dir) => {
                    serde_json::to_value(familiar_status(read_room_spellbook(room_dir.as_ref())))
                        .expect("familiar status serializes")
                }
                Err(rejection) => rejection,
            };
            routing_response(state, meta, result).await
        }
        ClientCommand::NormalizeLineage { meta, request } => {
            let memories = normalize_quest_memories(request);
            lineage_response(state, meta, true, memories).await
        }
        ClientCommand::SettleLineage { meta, lifecycle } => {
            let settled = lifecycle.is_terminal();
            let report = read_quest_report(&lifecycle);
            let memories = normalize_lifecycle_memory(lifecycle, &report)
                .into_iter()
                .collect();
            lineage_response(state, meta, settled, memories).await
        }
        ClientCommand::LogConversation { meta, request } => {
            let result = log_conversation(&meta, request);
            shell_response(state, meta, result).await
        }
        ClientCommand::PlanTriggerLessons { meta, request } => {
            let plan = process_lesson_plan(request.trigger.as_deref());
            shell_response(state, meta, json!({ "plan": plan })).await
        }
        ClientCommand::BraidTriggerLessons { meta, request } => {
            let reminder = process_lesson_reminder(request.trigger.as_deref(), &request.lessons);
            shell_response(state, meta, json!({ "reminder": reminder })).await
        }
        ClientCommand::ChatSubscribe { meta } => {
            let runtime = state.runtime.lock().await;
            let event = chat_event(
                state,
                &meta,
                CHAT_SNAPSHOT,
                runtime.chat.snapshot(),
                runtime.cursor.sequence,
            );
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        ClientCommand::ChatSay { meta, payload } => {
            let identity = match chat_identity(state, &meta, &payload.room).await {
                Ok(identity) => identity,
                Err(responses) => return *responses,
            };
            let mut runtime = state.runtime.lock().await;
            let appended = runtime.chat.say(
                &identity.operator,
                &payload.text,
                &payload.say_id,
                now_rfc3339(),
            );
            let sequence = runtime.cursor.sequence;
            drop(runtime);
            chat_appended(state, &meta, appended, sequence).await
        }
        ClientCommand::ChatTurn { meta, payload } => {
            let identity = match chat_identity(state, &meta, &payload.room).await {
                Ok(identity) => identity,
                Err(responses) => return *responses,
            };
            let mut runtime = state.runtime.lock().await;
            let appended = runtime.chat.turn(
                &identity.spirit,
                &payload.text,
                &payload.turn_id,
                now_rfc3339(),
            );
            let sequence = runtime.cursor.sequence;
            drop(runtime);
            chat_appended(state, &meta, appended, sequence).await
        }
    }
}

/// The room's authenticated identity, with the claimed room checked against
/// this Host's own room; a foreign room refuses loudly.
async fn chat_identity(
    state: &AppState,
    meta: &CommandMeta,
    claimed_room: &str,
) -> Result<RoomIdentity, Box<Responses>> {
    if claimed_room != state.config.room {
        return Err(Box::new(
            chat_refusal(state, meta, "chat command names a foreign room").await,
        ));
    }
    match state.room_store.identity() {
        Ok(identity) => Ok(identity),
        Err(reason) => Err(Box::new(chat_refusal(state, meta, &reason).await)),
    }
}

/// Answer an append: accepted either way (a duplicate id is a retry), and a
/// fresh line additionally broadcasts one delta to chat subscribers.
async fn chat_appended(
    state: &AppState,
    meta: &CommandMeta,
    appended: Option<ChatMessage>,
    sequence: u64,
) -> Responses {
    if let Some(message) = appended {
        let delta = chat_event(state, meta, CHAT_DELTA, vec![message], sequence);
        let _ = state.chat_deltas.send(serialize(&delta));
    }
    let outcome_hash = body_hash(&json!({ "ok": true })).expect("chat outcome hashes");
    let event = CommandOutcomeEvent {
        meta: event_meta_for_projection(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            CHAT_COMMAND_ACCEPTED,
            CHAT_PROJECTION_ID,
            sequence,
            outcome_hash,
            new_id(),
        ),
        reason: None,
        version: 0,
        state: None,
        decision: None,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

async fn chat_refusal(state: &AppState, meta: &CommandMeta, reason: &str) -> Responses {
    let runtime = state.runtime.lock().await;
    let reason_hash = body_hash(&json!({ "reason": reason })).expect("chat refusal hashes");
    let event = CommandOutcomeEvent {
        meta: event_meta_for_projection(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            CHAT_COMMAND_REFUSED,
            CHAT_PROJECTION_ID,
            runtime.cursor.sequence,
            reason_hash,
            new_id(),
        ),
        reason: Some(reason.to_owned()),
        version: 0,
        state: None,
        decision: None,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

fn chat_event(
    state: &AppState,
    meta: &CommandMeta,
    kind: &str,
    messages: Vec<ChatMessage>,
    sequence: u64,
) -> ChatEvent {
    let body_hash = body_hash(&json!({ "messages": messages.len() })).expect("chat frame hashes");
    ChatEvent {
        meta: event_meta_for_projection(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            kind,
            CHAT_PROJECTION_ID,
            sequence,
            body_hash,
            new_id(),
        ),
        room: state.config.room.clone(),
        messages,
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn open_presence_frame(
    state: &AppState,
    meta: CommandMeta,
    request: PresenceOpenRequest,
) -> Responses {
    let authentication = match authenticate_presence(state, &meta) {
        Ok(authentication) => authentication,
        Err(reason) => return presence_refusal(state, &meta, &reason).await,
    };
    let mut runtime = state.runtime.lock().await;
    let result = runtime
        .presence
        .open(&authentication, &meta.idempotency_key, request)
        .map(PresenceResult::Open);
    presence_response(
        state,
        &meta,
        PRESENCE_OPENED,
        result,
        runtime.cursor.sequence,
    )
}

/// Derive who is present from the authenticated command and the room's own
/// state, and project the capabilities the Host can actually stand behind.
///
/// enough: the open request still carries a binding, but it is a claim. The
/// session comes from the authenticated envelope, the spirit and operator come
/// from room state, and a sender whose spirit disagrees with the room is
/// refused here rather than opening a frame under a name the House does not
/// recognise. Capabilities are never read from the wire.
fn authenticate_presence(
    state: &AppState,
    meta: &CommandMeta,
) -> Result<PresenceAuthentication, String> {
    let identity: RoomIdentity = state.room_store.identity()?;
    if identity.room != meta.sender_room {
        return Err(format!(
            "Presence sender room {} is not this room",
            meta.sender_room
        ));
    }
    if identity.spirit != meta.sender_spirit {
        return Err(format!(
            "Presence sender spirit {} is not the embodied spirit {}",
            meta.sender_spirit, identity.spirit
        ));
    }
    Ok(PresenceAuthentication {
        binding: PresenceBinding {
            room: identity.room,
            spirit: identity.spirit,
            operator: identity.operator,
            session: meta.sender_session.clone(),
        },
        capabilities: host_capabilities(
            state.config.database_url.is_some(),
            state.config.nats_url.is_some(),
        ),
    })
}

async fn compile_presence_turn(
    state: &AppState,
    meta: CommandMeta,
    request: PresenceTurnRequest,
) -> Responses {
    let mut runtime = state.runtime.lock().await;
    let result = runtime
        .presence
        .compile(&meta.sender_session, &meta.idempotency_key, request)
        .map(PresenceResult::Compile);
    presence_response(
        state,
        &meta,
        PRESENCE_COMPILED,
        result,
        runtime.cursor.sequence,
    )
}

async fn settle_presence_turn(
    state: &AppState,
    meta: CommandMeta,
    request: PresenceSettleRequest,
) -> Responses {
    let mut runtime = state.runtime.lock().await;
    let result = runtime
        .presence
        .settle(&meta.sender_session, &meta.idempotency_key, request)
        .map(PresenceResult::Settle);
    presence_response(
        state,
        &meta,
        PRESENCE_SETTLED,
        result,
        runtime.cursor.sequence,
    )
}

async fn close_presence_frame(
    state: &AppState,
    meta: CommandMeta,
    request: PresenceCloseRequest,
) -> Responses {
    let mut runtime = state.runtime.lock().await;
    let result = runtime
        .presence
        .close(&meta.sender_session, &meta.idempotency_key, request)
        .map(PresenceResult::Close);
    presence_response(
        state,
        &meta,
        PRESENCE_CLOSED,
        result,
        runtime.cursor.sequence,
    )
}

fn presence_response(
    state: &AppState,
    meta: &CommandMeta,
    event_type: &str,
    result: Result<PresenceResult, impl ToString>,
    sequence: u64,
) -> Responses {
    match result {
        Ok(result) => {
            let value = serde_json::to_value(&result).expect("Presence result serializes");
            let event = PresenceResultEvent {
                meta: event_meta_for_projection(
                    state,
                    Some(meta),
                    &meta.message_id,
                    &meta.idempotency_key,
                    event_type,
                    PRESENCE_PROJECTION_ID,
                    sequence,
                    body_hash(&value).expect("Presence result hashes"),
                    new_id(),
                ),
                result,
            };
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        Err(error) => presence_refusal_sync(state, meta, error.to_string(), sequence),
    }
}

async fn presence_refusal(state: &AppState, meta: &CommandMeta, reason: &str) -> Responses {
    let runtime = state.runtime.lock().await;
    presence_refusal_sync(state, meta, reason.to_owned(), runtime.cursor.sequence)
}

fn presence_refusal_sync(
    state: &AppState,
    meta: &CommandMeta,
    reason: String,
    sequence: u64,
) -> Responses {
    let reason_hash = body_hash(&json!({ "reason": reason })).expect("Presence refusal hashes");
    let event = CommandOutcomeEvent {
        meta: event_meta_for_projection(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            PRESENCE_COMMAND_REFUSED,
            PRESENCE_PROJECTION_ID,
            sequence,
            reason_hash,
            new_id(),
        ),
        reason: Some(reason),
        version: 0,
        state: None,
        decision: None,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

fn hallway_projection_changed(previous: Option<&str>, current: &str, ringing: bool) -> bool {
    previous != Some(current) && (ringing || previous.is_some())
}

async fn project_hallway_inbox(state: &AppState, meta: CommandMeta) -> Responses {
    let Some(pool) = state.hallway_pool.as_ref() else {
        record_point(
            state.insula_binding.as_ref(),
            "host",
            "host",
            "hallway_projection",
            OutcomeClass::Degraded,
            None,
            None,
        );
        let failed = outcome(
            state,
            &meta,
            RECALL_POLICY_COMMAND_FAILED,
            Some("Hallway projection requires DATABASE_URL".into()),
        )
        .await;
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    };
    let inbox = match hallway_inbox(
        pool,
        HallwayInboxRequest {
            room: meta.sender_room.clone(),
            spirit: meta.sender_spirit.clone(),
            session: meta.sender_session.clone(),
        },
    )
    .await
    {
        Ok(inbox) => inbox,
        Err(error) => {
            record_point(
                state.insula_binding.as_ref(),
                "host",
                "host",
                "hallway_projection",
                app_error_outcome(&error),
                Some("app_error"),
                None,
            );
            let failed = outcome(
                state,
                &meta,
                RECALL_POLICY_COMMAND_FAILED,
                Some(format!("Hallway projection failed: {error}")),
            )
            .await;
            return Responses {
                direct: vec![serialize(&failed)],
                delta: None,
            };
        }
    };
    let inbox_value = serde_json::to_value(&inbox).expect("Hallway inbox serializes");
    let fingerprint = body_hash(&inbox_value).expect("Hallway inbox hashes");
    let ringing = inbox
        .hallways
        .iter()
        .any(|entry| entry.unread > 0 || entry.mentions > 0);
    let mut runtime = state.runtime.lock().await;
    let previous = runtime
        .hallway_inbox_fingerprints
        .insert(meta.sender_session.clone(), fingerprint.clone());
    let changed = hallway_projection_changed(previous.as_deref(), &fingerprint, ringing);
    let event = HallwayInboxProjectionEvent {
        meta: event_meta_for_projection(
            state,
            Some(&meta),
            &meta.message_id,
            &meta.idempotency_key,
            HALLWAY_INBOX_PROJECTED,
            HALLWAY_PROJECTION_ID,
            runtime.cursor.sequence,
            fingerprint,
            new_id(),
        ),
        changed,
        inbox,
    };
    record_point(
        state.insula_binding.as_ref(),
        "host",
        "host",
        "hallway_projection",
        OutcomeClass::Ok,
        None,
        Some((HALLWAY_INBOX_PROJECTED, event.meta.event_id.as_str())),
    );
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

/// Host-owned authority gate for Hallway Knock coordination.
///
/// Runs before any `hallway_pool` access so a disabled Host or a foreign
/// caller never reaches the database. Authority is read from [`HostConfig`],
/// never from the caller envelope: a bearer token proves reach to this Host,
/// not the right to act as another room or spirit.
fn knock_authority(config: &HostConfig, meta: &CommandMeta) -> Result<(), AppError> {
    if !config.knock_autonomy.claims_enabled() {
        return Err(AppError::Refusal {
            code: "knock_autonomy_disabled",
            message: "Host Knock autonomy is off; set ATHANOR_HOST_KNOCK_AUTONOMY=claim to enable",
        });
    }
    if meta.sender_room != config.room || meta.sender_spirit != config.spirit {
        return Err(AppError::Refusal {
            code: "foreign_knock_authority",
            message: "Knock commands must carry this Host's own room and spirit",
        });
    }
    Ok(())
}

fn app_error_outcome(error: &AppError) -> OutcomeClass {
    match error {
        AppError::Invalid(_) | AppError::Refusal { .. } => OutcomeClass::Refused,
        _ => OutcomeClass::Error,
    }
}

/// One honest AKASHA read failure: the reason travels, the ledger does not move.
async fn akasha_failed(state: &AppState, meta: &CommandMeta, reason: String) -> Responses {
    let failed = outcome(state, meta, AKASHA_COMMAND_FAILED, Some(reason)).await;
    Responses {
        direct: vec![serialize(&failed)],
        delta: None,
    }
}

/// One read-only AKASHA recall. The room is the bound sender's room, never a
/// payload field, and every retrieval tuning field stays at its substrate
/// default so the GUI reads exactly what the organs read.
async fn query_akasha_recall(
    state: &AppState,
    meta: CommandMeta,
    payload: AkashaRecallQueryPayload,
) -> Responses {
    let Some(pool) = state.hallway_pool.as_ref() else {
        return akasha_failed(state, &meta, "Akasha recall requires DATABASE_URL".into()).await;
    };
    let config = match SubstrateConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            return akasha_failed(
                state,
                &meta,
                format!("Akasha recall requires substrate configuration: {error}"),
            )
            .await;
        }
    };
    let params: RecallParams = serde_json::from_value(json!({
        "room": meta.sender_room,
        "query": payload.query
    }))
    .expect("recall params carry only the bound room and query, every tuning field defaulted");
    let request = match hearth::RecallRequest::try_from(params) {
        Ok(request) => request,
        Err(error) => {
            return akasha_failed(state, &meta, format!("Akasha recall refused: {error}")).await;
        }
    };
    let result = match recall(pool, &config, request, None).await {
        Ok(result) => serde_json::to_value(&result).expect("Akasha recall result serializes"),
        Err(error) => {
            return akasha_failed(state, &meta, format!("Akasha recall failed: {error}")).await;
        }
    };
    let runtime = state.runtime.lock().await;
    let event = AkashaRecallResultEvent {
        meta: event_meta_for_projection(
            state,
            Some(&meta),
            &meta.message_id,
            &meta.idempotency_key,
            AKASHA_RECALL_RESULT,
            AKASHA_PROJECTION_ID,
            runtime.cursor.sequence,
            body_hash(&result).expect("Akasha recall result hashes"),
            new_id(),
        ),
        result,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

fn lesson_family(family: AkashaLessonFamily) -> LessonFamily {
    match family {
        AkashaLessonFamily::Coding => LessonFamily::Coding,
        AkashaLessonFamily::Project => LessonFamily::Project,
        AkashaLessonFamily::Writing => LessonFamily::Writing,
        AkashaLessonFamily::Design => LessonFamily::Design,
        AkashaLessonFamily::Audio => LessonFamily::Audio,
    }
}

/// One read-only AKASHA lesson query. Filters arrive bounded from the protocol;
/// the room is binding, so a client can only ever read its own room's scope.
async fn query_akasha_lessons(
    state: &AppState,
    meta: CommandMeta,
    payload: AkashaLessonQueryPayload,
) -> Responses {
    let Some(pool) = state.hallway_pool.as_ref() else {
        return akasha_failed(
            state,
            &meta,
            "Akasha lesson query requires DATABASE_URL".into(),
        )
        .await;
    };
    let params = LessonQueryParams {
        room: meta.sender_room.clone(),
        family: lesson_family(payload.family),
        shape: payload.shape,
        project: payload.project,
        register: payload.register,
        stage: payload.stage,
        language_keys: payload.language_keys,
        technology_keys: payload.technology_keys,
        query: payload.query,
        always_on: false,
        limit: payload.limit,
    };
    let result = match lesson_query(pool, params).await {
        Ok(result) => serde_json::to_value(&result).expect("Akasha lesson result serializes"),
        Err(error) => {
            return akasha_failed(state, &meta, format!("Akasha lesson query failed: {error}"))
                .await;
        }
    };
    let runtime = state.runtime.lock().await;
    let event = AkashaLessonResultEvent {
        meta: event_meta_for_projection(
            state,
            Some(&meta),
            &meta.message_id,
            &meta.idempotency_key,
            AKASHA_LESSON_RESULT,
            AKASHA_PROJECTION_ID,
            runtime.cursor.sequence,
            body_hash(&result).expect("Akasha lesson result hashes"),
            new_id(),
        ),
        result,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

async fn claim_hallway_knock(state: &AppState, meta: CommandMeta) -> Responses {
    let mut span = PendingKnockObservation(defer_span(
        state.insula_binding.as_ref(),
        "host",
        "host",
        "knock_claim",
    ));
    if let Err(error) = knock_authority(&state.config, &meta) {
        end_span(span.0.take(), app_error_outcome(&error), Some("app_error"));
        return hallway_knock_error(state, &meta, "claim", error).await;
    }
    let Some(pool) = state.hallway_pool.as_ref() else {
        end_span(span.0.take(), OutcomeClass::Degraded, None);
        let failed = outcome(
            state,
            &meta,
            HALLWAY_KNOCK_COMMAND_FAILED,
            Some("Hallway Knock claim requires DATABASE_URL".into()),
        )
        .await;
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    };
    match hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: state.config.room.clone(),
            spirit: state.config.spirit.clone(),
            session: meta.sender_session.clone(),
        },
    )
    .await
    {
        Ok(result) => {
            let result_value =
                serde_json::to_value(&result).expect("Hallway Knock claim serializes");
            let mut runtime = state.runtime.lock().await;
            let observation = runtime
                .knock_poll
                .observe(result.knock.is_some(), Instant::now());
            let span = span.0.take();
            match observation {
                KnockPollObservation::ClaimSpan => end_span(span, OutcomeClass::Ok, None),
                KnockPollObservation::PollPoint => record_point(
                    state.insula_binding.as_ref(),
                    "host",
                    "host",
                    "knock_poll",
                    OutcomeClass::Ok,
                    None,
                    None,
                ),
                KnockPollObservation::Quiet => {}
            }
            let event = HallwayKnockClaimedEvent {
                meta: event_meta_for_projection(
                    state,
                    Some(&meta),
                    &meta.message_id,
                    &meta.idempotency_key,
                    HALLWAY_KNOCK_CLAIMED,
                    HALLWAY_PROJECTION_ID,
                    runtime.cursor.sequence,
                    body_hash(&result_value).expect("Hallway Knock claim hashes"),
                    new_id(),
                ),
                result,
            };
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        Err(error) => {
            end_span(span.0.take(), app_error_outcome(&error), Some("app_error"));
            hallway_knock_error(state, &meta, "claim", error).await
        }
    }
}

async fn settle_hallway_knock(
    state: &AppState,
    meta: CommandMeta,
    request: protocol::HallwayKnockSettlePayload,
) -> Responses {
    let span = start_span(
        state.insula_binding.as_ref(),
        "host",
        "host",
        "knock_settle",
    );
    if let Err(error) = knock_authority(&state.config, &meta) {
        end_span(span, app_error_outcome(&error), Some("app_error"));
        return hallway_knock_error(state, &meta, "settlement", error).await;
    }
    let Some(pool) = state.hallway_pool.as_ref() else {
        end_span(span, OutcomeClass::Degraded, None);
        let failed = outcome(
            state,
            &meta,
            HALLWAY_KNOCK_COMMAND_FAILED,
            Some("Hallway Knock settlement requires DATABASE_URL".into()),
        )
        .await;
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    };
    match hallway_knock_settle(
        pool,
        HallwayKnockSettleRequest {
            room: state.config.room.clone(),
            spirit: state.config.spirit.clone(),
            session: meta.sender_session.clone(),
            knock_id: request.knock_id,
            outcome: request.outcome,
            reason: request.reason,
        },
    )
    .await
    {
        Ok(result) => {
            let result_value =
                serde_json::to_value(&result).expect("Hallway Knock settlement serializes");
            let runtime = state.runtime.lock().await;
            let event = HallwayKnockSettledEvent {
                meta: event_meta_for_projection(
                    state,
                    Some(&meta),
                    &meta.message_id,
                    &meta.idempotency_key,
                    HALLWAY_KNOCK_SETTLED,
                    HALLWAY_PROJECTION_ID,
                    runtime.cursor.sequence,
                    body_hash(&result_value).expect("Hallway Knock settlement hashes"),
                    new_id(),
                ),
                result,
            };
            end_span(span, OutcomeClass::Ok, None);
            Responses {
                direct: vec![serialize(&event)],
                delta: None,
            }
        }
        Err(error) => {
            end_span(span, app_error_outcome(&error), Some("app_error"));
            hallway_knock_error(state, &meta, "settlement", error).await
        }
    }
}

async fn hallway_knock_error(
    state: &AppState,
    meta: &CommandMeta,
    operation: &str,
    error: AppError,
) -> Responses {
    let (kind, reason) = match error {
        AppError::Refusal { code, message } => (
            HALLWAY_KNOCK_COMMAND_REFUSED,
            format!("Hallway Knock {operation} refused ({code}): {message}"),
        ),
        other => (
            HALLWAY_KNOCK_COMMAND_FAILED,
            format!("Hallway Knock {operation} failed: {other}"),
        ),
    };
    let event = outcome(state, meta, kind, Some(reason)).await;
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

async fn routing_response(state: &AppState, meta: CommandMeta, result: Value) -> Responses {
    let runtime = state.runtime.lock().await;
    let event_id = new_id();
    let event = RoutingResultEvent {
        meta: event_meta_for_projection(
            state,
            Some(&meta),
            &meta.message_id,
            &meta.idempotency_key,
            ROUTING_RESULT,
            ROUTING_PROJECTION_ID,
            runtime.cursor.sequence,
            body_hash(&result).expect("routing result hashes"),
            event_id,
        ),
        result,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

async fn lineage_response(
    state: &AppState,
    meta: CommandMeta,
    settled: bool,
    memories: Vec<hearth::lineage::QuestMemory>,
) -> Responses {
    let runtime = state.runtime.lock().await;
    let result = serde_json::to_value(&memories).expect("lineage memories serialize");
    let event = LineageResultEvent {
        meta: event_meta_for_projection(
            state,
            Some(&meta),
            &meta.message_id,
            &meta.idempotency_key,
            LINEAGE_NORMALIZED,
            LINEAGE_PROJECTION_ID,
            runtime.cursor.sequence,
            body_hash(&result).expect("lineage memories hash"),
            new_id(),
        ),
        settled,
        memories,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

async fn shell_response(state: &AppState, meta: CommandMeta, result: Value) -> Responses {
    let runtime = state.runtime.lock().await;
    let event = ShellResultEvent {
        meta: event_meta_for_projection(
            state,
            Some(&meta),
            &meta.message_id,
            &meta.idempotency_key,
            SHELL_RESULT,
            SHELL_PROJECTION_ID,
            runtime.cursor.sequence,
            body_hash(&result).expect("shell result hashes"),
            new_id(),
        ),
        result,
    };
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}

/// Caller-selected filesystem authority is refused: a supplied `room_dir`
/// must name this Host's configured room directory (canonicalized equal) or
/// stay empty. Anything else is rejected loudly, never silently served.
fn resolve_room_dir<'a>(
    requested: Option<&'a str>,
    config: &'a HostConfig,
) -> Result<Cow<'a, str>, Value> {
    let Some(requested) = requested.filter(|room_dir| !room_dir.trim().is_empty()) else {
        return Ok(config.room_dir.to_string_lossy());
    };

    let supplied = std::path::Path::new(requested.trim());
    let matches_configured = match (supplied.canonicalize(), config.room_dir.canonicalize()) {
        (Ok(supplied), Ok(configured)) => supplied == configured,
        _ => false,
    };
    if matches_configured {
        return Ok(config.room_dir.to_string_lossy());
    }

    Err(json!({
        "ok": false,
        "status": "rejected",
        "errors": [format!(
            "room_dir must name this Host's configured room directory; refused: {requested}"
        )],
        "warnings": [],
        "spawnPacket": Value::Null,
    }))
}

/// The Host owns the filesystem for room-local House files; `core` owns
/// which files those are and what they must contain.
fn read_room_spellbook(room_dir: &str) -> LoadedSpellbook {
    load_spellbook(room_dir, |candidate| {
        match std::fs::read_to_string(candidate) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(spellbook) => SpellbookRead::Parsed(spellbook),
                Err(error) => SpellbookRead::Malformed(error.to_string()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => SpellbookRead::Missing,
            Err(error) => SpellbookRead::Unreadable(error.to_string()),
        }
    })
}

fn read_quest_report(lifecycle: &QuestLifecycle) -> String {
    lifecycle
        .report_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default()
}

fn append_line(path: &str, content: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut file, content.as_bytes()).map_err(|error| error.to_string())
}

/// Capture the visible conversation: identity, freshness, dedupe marker, and
/// transcript shape all come from `hearth::conversation`.
fn log_conversation(meta: &CommandMeta, request: ConversationLogRequest) -> Value {
    let now = chrono::Local::now();
    let captured_at = now
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let date_stamp = now.format("%Y-%m-%d").to_string();
    let clock = now.format("%H:%M").to_string();
    let turns: Vec<ConversationTurn> = conversation_turns(&request.messages, &captured_at);
    let fresh = is_fresh_conversation(&turns);
    let ledger_directory = source_ledger_directory(&request.room_dir);

    let mut appended = 0_u64;
    let mut skipped = 0_u64;
    let mut errors = Vec::new();
    let mut logged = Vec::new();

    if request.persist {
        let transcript = transcript_path(&request.room_dir, &date_stamp);
        let ledger = source_ledger_path(&request.room_dir, &date_stamp);
        let mut existing_transcript = std::fs::read_to_string(&transcript).unwrap_or_default();
        let mut existing_ledger = std::fs::read_to_string(&ledger).unwrap_or_default();
        for turn in &turns {
            let key = turn_key(&request.session_id, turn);
            let marker = turn_marker(&key);
            let label = turn_label(&turn.role, &request.operator, &request.spirit);
            match transcript_entry(
                &existing_transcript,
                &marker,
                &date_stamp,
                &clock,
                label,
                &turn.text,
            ) {
                Some(entry) => match append_line(&transcript, &entry) {
                    Ok(()) => {
                        existing_transcript.push_str(&entry);
                        appended += 1;
                    }
                    Err(error) => errors.push(json!({
                        "key": key,
                        "surface": "transcript",
                        "error": error,
                    })),
                },
                None => skipped += 1,
            }

            let source = logged_turn(turn, &request.session_id);
            let ledger_ready = match source_ledger_entry(&existing_ledger, &source) {
                Ok(Some(entry)) => match append_line(&ledger, &entry) {
                    Ok(()) => {
                        existing_ledger.push_str(&entry);
                        true
                    }
                    Err(error) => {
                        errors.push(json!({
                            "key": key,
                            "surface": "giga_source_ledger",
                            "error": error,
                        }));
                        false
                    }
                },
                Ok(None) => true,
                Err(error) => {
                    errors.push(json!({
                        "key": key,
                        "surface": "giga_source_ledger",
                        "error": error,
                    }));
                    false
                }
            };
            if ledger_ready {
                logged.push(source);
            }
        }

        let debug = json!({
            "timestamp": captured_at,
            "room": &meta.sender_room,
            "source": &request.source,
            "turns": turns.len(),
            "appended": appended,
            "skipped": skipped,
            "errors": errors.clone(),
            "sourceLedgerDirectory": &ledger_directory,
        });
        // Debug provenance must never block capture.
        let _ = append_line(
            &transcript_debug_path(&request.room_dir),
            &format!("{debug}\n"),
        );
    }

    json!({
        "turns": turns.len(),
        "fresh": fresh,
        "appended": appended,
        "skipped": skipped,
        "errors": errors,
        "loggedTurns": logged,
        "sourceLedgerDirectory": ledger_directory,
    })
}

fn invalid_routing_request(kind: &str, error: serde_json::Error) -> Value {
    json!({
        "ok": false,
        "status": "rejected",
        "errors": [format!("Invalid {kind} request: {error}")],
        "warnings": [],
        "spawnPacket": Value::Null,
    })
}

async fn set_requested_mode(
    state: &AppState,
    meta: CommandMeta,
    base_version: u64,
    requested_mode: protocol::RecallRequestedMode,
    command_hash: String,
) -> Responses {
    let mut runtime = state.runtime.lock().await;
    if let Some(response) = idempotency_response(state, &runtime, &meta, &command_hash) {
        return response;
    }
    if let Err(reason) = refresh_from_room_state(state, &mut runtime) {
        let failed = outcome_with_runtime(
            state,
            &meta,
            RECALL_POLICY_COMMAND_FAILED,
            Some(reason),
            &runtime,
            None,
        );
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    }
    if base_version != runtime.cursor.version {
        let refusal = outcome_with_runtime(
            state,
            &meta,
            RECALL_POLICY_COMMAND_REFUSED,
            Some(format!(
                "stale base_version {base_version}; current version is {}",
                runtime.cursor.version
            )),
            &runtime,
            None,
        );
        let _ = runtime.durable.save_receipt(DurableReceipt {
            idempotency_key: meta.idempotency_key.clone(),
            body_hash: command_hash,
            outcome: refusal.clone(),
            stored_at: timestamp(),
        });
        return Responses {
            direct: vec![serialize(&refusal)],
            delta: None,
        };
    }
    let next = apply_requested_mode(&runtime.projection, requested_mode, timestamp());
    commit_change(state, &mut runtime, &meta, command_hash, next, None)
}

fn refresh_from_room_state(state: &AppState, runtime: &mut RuntimeState) -> Result<(), String> {
    let projection = state.room_store.load()?;
    let hash = state_hash(&projection)?;
    if hash == runtime.cursor.state_hash {
        return Ok(());
    }
    let version = runtime
        .cursor
        .version
        .checked_add(1)
        .ok_or_else(|| "projection version overflow while reloading room state".to_string())?;
    let sequence = runtime
        .cursor
        .sequence
        .checked_add(1)
        .ok_or_else(|| "projection sequence overflow while reloading room state".to_string())?;
    runtime.projection = projection;
    runtime.cursor = ProjectionCursor {
        projection_id: RECALL_POLICY_PROJECTION_ID.into(),
        version,
        sequence,
        state_hash: hash,
    };
    runtime.sessions.clear();
    runtime.sessions.insert(
        state.config.session.clone(),
        RecallPolicySession::fresh(&runtime.projection),
    );
    let sessions = runtime.sessions.clone();
    let cursor = runtime.cursor.clone();
    runtime.durable.save_sessions(&sessions)?;
    runtime.durable.save_cursor(&cursor)
}

fn idempotency_response(
    state: &AppState,
    runtime: &RuntimeState,
    meta: &CommandMeta,
    command_hash: &str,
) -> Option<Responses> {
    let receipt = runtime.durable.receipt(&meta.idempotency_key)?;
    if receipt.body_hash == command_hash {
        return Some(Responses {
            direct: vec![serialize(&receipt.outcome)],
            delta: None,
        });
    }
    let refusal = outcome_with_runtime(
        state,
        meta,
        RECALL_POLICY_COMMAND_REFUSED,
        Some("idempotency_key was already used for a different command body".into()),
        runtime,
        None,
    );
    Some(Responses {
        direct: vec![serialize(&refusal)],
        delta: None,
    })
}

fn commit_change(
    state: &AppState,
    runtime: &mut RuntimeState,
    meta: &CommandMeta,
    command_hash: String,
    next: RecallPolicyState,
    decision: Option<RecallPolicyDecision>,
) -> Responses {
    let has_decision = decision.is_some();
    let mutations = RecallPolicyMutation::between(&runtime.projection, &next);
    let base_version = runtime.cursor.version;
    let next_version = match base_version.checked_add(1) {
        Some(version) => version,
        None => {
            let failed = outcome_with_runtime(
                state,
                meta,
                RECALL_POLICY_COMMAND_FAILED,
                Some("projection version overflow".into()),
                runtime,
                None,
            );
            return Responses {
                direct: vec![serialize(&failed)],
                delta: None,
            };
        }
    };
    let next_sequence = match runtime.cursor.sequence.checked_add(1) {
        Some(sequence) => sequence,
        None => {
            let failed = outcome_with_runtime(
                state,
                meta,
                RECALL_POLICY_COMMAND_FAILED,
                Some("projection sequence overflow".into()),
                runtime,
                None,
            );
            return Responses {
                direct: vec![serialize(&failed)],
                delta: None,
            };
        }
    };
    let next_hash = match state_hash(&next) {
        Ok(hash) => hash,
        Err(reason) => {
            let failed = outcome_with_runtime(
                state,
                meta,
                RECALL_POLICY_COMMAND_FAILED,
                Some(reason),
                runtime,
                None,
            );
            return Responses {
                direct: vec![serialize(&failed)],
                delta: None,
            };
        }
    };
    if let Err(reason) = state.room_store.write_policy(&next) {
        let failed = outcome_with_runtime(
            state,
            meta,
            RECALL_POLICY_COMMAND_FAILED,
            Some(reason),
            runtime,
            None,
        );
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    }
    runtime.projection = next;
    runtime.cursor = ProjectionCursor {
        projection_id: RECALL_POLICY_PROJECTION_ID.into(),
        version: next_version,
        sequence: next_sequence,
        state_hash: next_hash.clone(),
    };
    if let Err(reason) = runtime.durable.save_sessions(&runtime.sessions) {
        let failed = outcome_with_runtime(
            state,
            meta,
            RECALL_POLICY_COMMAND_FAILED,
            Some(reason),
            runtime,
            None,
        );
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    }
    if let Err(reason) = runtime.durable.save_cursor(&runtime.cursor) {
        let failed = outcome_with_runtime(
            state,
            meta,
            RECALL_POLICY_COMMAND_FAILED,
            Some(reason),
            runtime,
            None,
        );
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    }
    let accepted = outcome_with_runtime(
        state,
        meta,
        RECALL_POLICY_COMMAND_ACCEPTED,
        None,
        runtime,
        decision,
    );
    if let Err(reason) = runtime.durable.save_receipt(DurableReceipt {
        idempotency_key: meta.idempotency_key.clone(),
        body_hash: command_hash,
        outcome: accepted.clone(),
        stored_at: timestamp(),
    }) {
        let failed = outcome_with_runtime(
            state,
            meta,
            RECALL_POLICY_COMMAND_FAILED,
            Some(reason),
            runtime,
            None,
        );
        return Responses {
            direct: vec![serialize(&failed)],
            delta: None,
        };
    }
    if has_decision {
        record_point(
            state.insula_binding.as_ref(),
            "host",
            "host",
            "recall_policy_decide",
            OutcomeClass::Ok,
            None,
            Some((
                RECALL_POLICY_COMMAND_ACCEPTED,
                accepted.meta.event_id.as_str(),
            )),
        );
    }
    let delta_event_id = new_id();
    let delta = DeltaEvent {
        meta: event_meta(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            RECALL_POLICY_DELTA,
            next_sequence,
            next_hash,
            delta_event_id.clone(),
        ),
        delta_id: delta_event_id,
        base_version,
        next_version,
        source_event_ids: vec![accepted.meta.event_id.clone()],
        mutations,
        coalesce_key: "recall_policy".into(),
    };
    Responses {
        direct: vec![serialize(&accepted)],
        delta: Some(serialize(&delta)),
    }
}

fn validate_command(state: &AppState, command: &ClientCommand) -> Result<(), String> {
    let meta = command.meta();
    if meta.max_hops != 1 {
        return Err("max_hops must be exactly 1".into());
    }
    if meta.correlation_id != meta.message_id {
        return Err("correlation_id must equal message_id for a client command".into());
    }
    let expires = chrono::DateTime::parse_from_rfc3339(&meta.expires_at)
        .map_err(|_| "expires_at must be an RFC 3339 timestamp")?;
    if expires <= chrono::Utc::now() {
        return Err("command has expired".into());
    }
    let expected = &state.config;
    let blank_binding = meta.house_id.is_empty()
        && meta.sender_room.is_empty()
        && meta.sender_spirit.is_empty()
        && meta.sender_session.is_empty()
        && meta.recipient.is_empty()
        && meta.reply_target.is_empty()
        && meta.scope.is_empty()
        && meta.visibility.is_empty()
        && meta.authority_class.is_empty();
    if matches!(
        command,
        ClientCommand::Subscribe { .. } | ClientCommand::PaperBoatReceiptSubscribe { .. }
    ) && blank_binding
    {
        return Ok(());
    }
    if meta.house_id != expected.house_id
        || meta.sender_room != expected.room
        || meta.sender_spirit != expected.spirit
        || meta.sender_session.trim().is_empty()
        || meta.sender_session.len() > 256
        || meta.recipient != HOST_RECIPIENT
        || meta.reply_target != meta.sender_session
        || meta.scope != expected.scope()
        || meta.visibility != "operator"
        || meta.authority_class != "room_state"
    {
        return Err(
            "command binding is foreign or does not match the authenticated Host binding".into(),
        );
    }
    Ok(())
}

fn snapshot(state: &AppState, meta: &CommandMeta, runtime: &RuntimeState) -> SnapshotEvent {
    let snapshot_id = new_id();
    SnapshotEvent {
        meta: event_meta(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            RECALL_POLICY_SNAPSHOT,
            runtime.cursor.sequence,
            runtime.cursor.state_hash.clone(),
            new_id(),
        ),
        snapshot_id,
        version: runtime.cursor.version,
        state: runtime.projection.clone(),
    }
}

async fn outcome(
    state: &AppState,
    meta: &CommandMeta,
    kind: &str,
    reason: Option<String>,
) -> CommandOutcomeEvent {
    let runtime = state.runtime.lock().await;
    outcome_with_runtime(state, meta, kind, reason, &runtime, None)
}

async fn outcome_for_ids(
    state: &AppState,
    message_id: &str,
    idempotency_key: &str,
    kind: &str,
    reason: Option<String>,
) -> CommandOutcomeEvent {
    let fallback_id = new_id();
    let message_id = if message_id.trim().is_empty() {
        fallback_id.as_str()
    } else {
        message_id
    };
    let idempotency_key = if idempotency_key.trim().is_empty() {
        message_id
    } else {
        idempotency_key
    };
    let runtime = state.runtime.lock().await;
    CommandOutcomeEvent {
        meta: event_meta(
            state,
            None,
            message_id,
            idempotency_key,
            kind,
            runtime.cursor.sequence,
            runtime.cursor.state_hash.clone(),
            new_id(),
        ),
        reason,
        version: runtime.cursor.version,
        state: Some(runtime.projection.clone()),
        decision: None,
    }
}

fn outcome_with_runtime(
    state: &AppState,
    meta: &CommandMeta,
    kind: &str,
    reason: Option<String>,
    runtime: &RuntimeState,
    decision: Option<RecallPolicyDecision>,
) -> CommandOutcomeEvent {
    CommandOutcomeEvent {
        meta: event_meta(
            state,
            Some(meta),
            &meta.message_id,
            &meta.idempotency_key,
            kind,
            runtime.cursor.sequence,
            runtime.cursor.state_hash.clone(),
            new_id(),
        ),
        reason,
        version: runtime.cursor.version,
        state: Some(runtime.projection.clone()),
        decision,
    }
}

async fn diagnostic_refusal(state: &AppState, reason: &str) -> String {
    let event = outcome_for_ids(
        state,
        "unparsed-command",
        "unparsed-command",
        RECALL_POLICY_COMMAND_REFUSED,
        Some(reason.into()),
    )
    .await;
    serialize(&event)
}

fn event_meta(
    state: &AppState,
    command: Option<&CommandMeta>,
    correlation_id: &str,
    idempotency_key: &str,
    kind: &str,
    sequence: u64,
    state_hash: String,
    event_id: String,
) -> EventMeta {
    event_meta_for_projection(
        state,
        command,
        correlation_id,
        idempotency_key,
        kind,
        RECALL_POLICY_PROJECTION_ID,
        sequence,
        state_hash,
        event_id,
    )
}

fn event_meta_for_projection(
    state: &AppState,
    command: Option<&CommandMeta>,
    correlation_id: &str,
    idempotency_key: &str,
    kind: &str,
    projection_id: &str,
    sequence: u64,
    state_hash: String,
    event_id: String,
) -> EventMeta {
    let sender_session = command
        .map(|meta| meta.sender_session.trim())
        .filter(|session| !session.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| state.config.session.clone());
    EventMeta {
        schema_version: HOST_SCHEMA_VERSION,
        event_id,
        house_id: state.config.house_id.clone(),
        sender_room: state.config.room.clone(),
        sender_spirit: state.config.spirit.clone(),
        sender_session: sender_session.clone(),
        recipient: sender_session,
        command_or_event_type: kind.into(),
        correlation_id: correlation_id.into(),
        causation_id: correlation_id.into(),
        reply_target: HOST_RECIPIENT.to_owned(),
        idempotency_key: idempotency_key.into(),
        source_record_refs: Vec::new(),
        scope: state.config.scope(),
        visibility: "operator".into(),
        authority_class: "room_state".into(),
        created_at: timestamp(),
        projection_id: projection_id.into(),
        sequence,
        state_hash,
    }
}

fn receipt_snapshot(
    state: &AppState,
    command: Option<&CommandMeta>,
    receipt_state: PaperBoatReceiptState,
) -> PaperBoatReceiptEvent {
    let receipt = receipt_state.receipt.as_ref();
    let sequence = receipt
        .map(|value| value.original_stream_sequence)
        .unwrap_or(0);
    let event_id = receipt
        .map(|value| value.event_id.clone())
        .unwrap_or_else(new_id);
    let correlation_id = command
        .map(|meta| meta.message_id.clone())
        .unwrap_or_else(|| event_id.clone());
    let idempotency_key = receipt
        .map(|value| value.event_id.clone())
        .or_else(|| command.map(|meta| meta.idempotency_key.clone()))
        .unwrap_or_else(|| event_id.clone());
    let state_hash = hex::encode(Sha256::digest(
        serde_json::to_vec(&receipt_state)
            .expect("typed receipt state contains no fallible JSON values"),
    ));
    PaperBoatReceiptEvent {
        meta: EventMeta {
            schema_version: HOST_SCHEMA_VERSION,
            event_id,
            house_id: state.config.house_id.clone(),
            sender_room: state.config.room.clone(),
            sender_spirit: state.config.spirit.clone(),
            sender_session: state.config.session.clone(),
            recipient: state.config.session.clone(),
            command_or_event_type: PAPER_BOAT_RECEIPT_SNAPSHOT.into(),
            correlation_id: correlation_id.clone(),
            causation_id: correlation_id,
            reply_target: HOST_RECIPIENT.to_owned(),
            idempotency_key,
            source_record_refs: Vec::new(),
            scope: format!("room:{}:paper_boat_receipt", state.config.room),
            visibility: "operator".into(),
            authority_class: "delivery_receipt".into(),
            created_at: receipt
                .map(|value| value.processed_at.clone())
                .unwrap_or_else(timestamp),
            projection_id: PAPER_BOAT_RECEIPT_PROJECTION_ID.into(),
            sequence,
            state_hash,
        },
        snapshot_id: new_id(),
        state: receipt_state,
    }
}

/// How long an ephemeral receipt replay consumer may sit idle before JetStream
/// reaps it. Its own knob, not the ack window it currently equals: this one is
/// "how long after the socket goes quiet do we stop paying for the cursor",
/// which is a client-liveness question, and it is free to move without touching
/// how long a delivered receipt has to be acknowledged.
const RECEIPT_CONSUMER_IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

async fn run_receipt_bridge(state: AppState, nats_url: String) {
    loop {
        if state.cancellation.is_cancelled() {
            return;
        }
        state.receipt_tracker.lock().await.connecting();
        let callback_state = state.clone();
        let client = match async_nats::ConnectOptions::new()
            .event_callback(move |event| {
                let callback_state = callback_state.clone();
                async move {
                    match event {
                        async_nats::Event::Disconnected | async_nats::Event::Closed => {
                            publish_receipt_degradation(
                                &callback_state,
                                "AKASHA delivery broker connection was lost",
                            )
                            .await;
                        }
                        async_nats::Event::Connected => {
                            let receipt_state = {
                                let mut tracker = callback_state.receipt_tracker.lock().await;
                                tracker.connected();
                                tracker.state()
                            };
                            let event = receipt_snapshot(&callback_state, None, receipt_state);
                            let _ = callback_state.receipts.send(serialize(&event));
                        }
                        _ => {}
                    }
                }
            })
            .connect(&nats_url)
            .await
        {
            Ok(client) => client,
            Err(_) => {
                publish_receipt_degradation(&state, "AKASHA delivery broker is unavailable").await;
                if wait_receipt_retry(&state).await {
                    return;
                }
                continue;
            }
        };
        let context = async_nats::jetstream::new(client);
        let stream = match context.get_stream(RECEIPT_STREAM_NAME).await {
            Ok(stream) => stream,
            Err(_) => {
                publish_receipt_degradation(
                    &state,
                    "AKASHA delivery receipt stream is not configured yet",
                )
                .await;
                if wait_receipt_retry(&state).await {
                    return;
                }
                continue;
            }
        };
        let mut consumer = match stream
            .create_consumer(async_nats::jetstream::consumer::pull::Config {
                name: Some(format!("athanor-host-receipts-{}", Uuid::new_v4().simple())),
                description: Some(
                    "Bounded ephemeral Host replay of sanitized Paper Boat receipts".into(),
                ),
                deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::All,
                ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
                ack_wait: ACK_WAIT,
                max_deliver: MAX_DELIVER,
                filter_subject: RECEIPT_SUBJECT.into(),
                max_ack_pending: MAX_ACK_PENDING,
                max_batch: MAX_BATCH,
                max_expires: MAX_EXPIRES,
                // The seam: this is a genuinely different consumer on the same
                // stream as origami's cranes broker, not a drifted copy of it.
                // The broker's lane consumers are durable ledger writers whose
                // cursor must outlive a restart; this one is ephemeral -- named
                // per connection, discarded after inactive_threshold, and read
                // only to refill one live socket. A cursor nobody will resume
                // has no reason to touch disk, so storage is memory and the
                // consumer is allowed to disappear with the client. That is why
                // memory_storage disagrees with broker's `false`, and why this
                // config carries no durable_name and no CONSUMER_BACKOFF: a
                // ten-minute backoff would outlive the socket it serves.
                inactive_threshold: RECEIPT_CONSUMER_IDLE_TTL,
                num_replicas: NUM_REPLICAS,
                memory_storage: true,
                ..Default::default()
            })
            .await
        {
            Ok(consumer) => consumer,
            Err(_) => {
                publish_receipt_degradation(
                    &state,
                    "AKASHA delivery receipt replay consumer is unavailable",
                )
                .await;
                if wait_receipt_retry(&state).await {
                    return;
                }
                continue;
            }
        };
        state.receipt_tracker.lock().await.connected();
        'replay: loop {
            let mut messages = match consumer
                .fetch()
                .max_messages(64)
                .expires(std::time::Duration::from_secs(5))
                .messages()
                .await
            {
                Ok(messages) => messages,
                Err(_) => {
                    publish_receipt_degradation(
                        &state,
                        "AKASHA delivery receipt replay cannot start",
                    )
                    .await;
                    break 'replay;
                }
            };
            loop {
                tokio::select! {
                    _ = state.cancellation.cancelled() => return,
                    incoming = messages.next() => {
                        let Some(incoming) = incoming else {
                            break;
                        };
                        let message = match incoming {
                            Ok(message) => message,
                            Err(_) => {
                                publish_receipt_degradation(
                                    &state,
                                    "AKASHA delivery receipt replay failed",
                                )
                                .await;
                                break 'replay;
                            }
                        };
                        let outcome = {
                            let mut tracker = state.receipt_tracker.lock().await;
                            match tracker.ingest(&state.config.room, message.payload.as_ref()) {
                                Ok(outcome) => Ok(outcome),
                                Err(_) => {
                                    tracker.refuse_malformed();
                                    Err(tracker.state())
                                }
                            }
                        };
                        match outcome {
                            Ok(ReceiptIngest::Accepted(receipt)) => {
                                record_point(
                                    state.insula_binding.as_ref(),
                                    "host",
                                    "host",
                                    "receipt_projection",
                                    OutcomeClass::Ok,
                                    None,
                                    Some(("paper_boat_receipt", receipt.event_id.as_str())),
                                );
                                let receipt_state = state.receipt_tracker.lock().await.state();
                                let event = receipt_snapshot(&state, None, receipt_state);
                                let _ = state.receipts.send(serialize(&event));
                            }
                            Ok(ReceiptIngest::Duplicate) => {
                                record_point(
                                    state.insula_binding.as_ref(),
                                    "host",
                                    "host",
                                    "receipt_projection",
                                    OutcomeClass::Ok,
                                    None,
                                    None,
                                );
                            }
                            Ok(ReceiptIngest::Stale | ReceiptIngest::ForeignRoom) => {
                                record_point(
                                    state.insula_binding.as_ref(),
                                    "host",
                                    "host",
                                    "receipt_projection",
                                    OutcomeClass::Refused,
                                    None,
                                    None,
                                );
                            }
                            Err(receipt_state) => {
                                record_point(
                                    state.insula_binding.as_ref(),
                                    "host",
                                    "host",
                                    "receipt_projection",
                                    OutcomeClass::Refused,
                                    None,
                                    None,
                                );
                                let event = receipt_snapshot(&state, None, receipt_state);
                                let _ = state.receipts.send(serialize(&event));
                            }
                        }
                        if message.ack().await.is_err() {
                            publish_receipt_degradation(
                                &state,
                                "AKASHA delivery receipt acknowledgement failed",
                            )
                            .await;
                            break 'replay;
                        }
                    }
                }
            }
            // A broker restart forgets this memory-only consumer while the
            // client reconnects underneath. The server answers a pull for a
            // missing consumer with an end-of-batch status, never an error,
            // so an empty batch is the only tell. Ask before pulling again;
            // a lost consumer rebuilds through the outer loop, which also
            // clears the degraded state the disconnect left behind.
            if consumer.info().await.is_err() {
                publish_receipt_degradation(
                    &state,
                    "AKASHA delivery receipt replay consumer was lost",
                )
                .await;
                break 'replay;
            }
        }
        if wait_receipt_retry(&state).await {
            return;
        }
    }
}

async fn publish_receipt_degradation(state: &AppState, reason: &str) {
    let receipt_state = {
        let mut tracker = state.receipt_tracker.lock().await;
        tracker.degraded(reason);
        tracker.state()
    };
    let event = receipt_snapshot(state, None, receipt_state);
    let _ = state.receipts.send(serialize(&event));
}

async fn wait_receipt_retry(state: &AppState) -> bool {
    tokio::select! {
        _ = state.cancellation.cancelled() => true,
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => false,
    }
}

fn serialize(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("typed Host events contain no fallible JSON values")
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        KNOCK_POLL_OBSERVATION_WINDOW, KnockPollObservation, KnockPollObservations,
        app_error_outcome, hallway_projection_changed, host_insula_binding, knock_authority,
        resolve_room_dir,
    };
    use crate::config::{HostConfig, KnockAutonomy};
    use akasha::{AppError, OutcomeClass};
    use protocol::CommandMeta;
    use std::time::{Duration, Instant};

    fn config(knock_autonomy: KnockAutonomy) -> HostConfig {
        HostConfig {
            bind: "127.0.0.1:0".parse().expect("test bind"),
            bearer_token: "test-token".into(),
            room_dir: std::path::PathBuf::from("configured-room"),
            state_dir: std::path::PathBuf::from("host-state"),
            house_id: "solarisael".into(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "test-session".into(),
            database_url: None,
            nats_url: None,
            knock_autonomy,
        }
    }

    fn meta(sender_room: &str, sender_spirit: &str) -> CommandMeta {
        CommandMeta {
            schema_version: 1,
            message_id: "message-1".into(),
            house_id: "solarisael".into(),
            sender_room: sender_room.into(),
            sender_spirit: sender_spirit.into(),
            sender_session: "caller-session".into(),
            recipient: "house-host".into(),
            correlation_id: "message-1".into(),
            causation_id: String::new(),
            reply_target: "caller-session".into(),
            idempotency_key: "message-1".into(),
            source_record_refs: Vec::new(),
            scope: "room:kodo:recall_policy".into(),
            visibility: "operator".into(),
            authority_class: "room_state".into(),
            created_at: "2026-08-20T00:00:00.000Z".into(),
            expires_at: "2099-08-20T00:00:00.000Z".into(),
            max_hops: 1,
            projection_id: "hallway".into(),
        }
    }

    fn refusal_code(error: AppError) -> &'static str {
        match error {
            AppError::Refusal { code, .. } => code,
            other => panic!("expected a typed refusal, got {other:?}"),
        }
    }

    #[test]
    fn empty_knock_polls_are_bounded_but_claims_and_transitions_keep_spans() {
        let start = Instant::now();
        let mut observations = KnockPollObservations::default();
        // Many sessions share this one Host state; polling faster must not
        // increase the heartbeat rate.
        for window in 0..3 {
            let base = start + KNOCK_POLL_OBSERVATION_WINDOW * window;
            assert_eq!(
                observations.observe(false, base),
                KnockPollObservation::PollPoint
            );
            for millis in [0, 1, 2_000, 50_000, 299_999] {
                assert_eq!(
                    observations.observe(false, base + Duration::from_millis(millis)),
                    KnockPollObservation::Quiet
                );
            }
        }
        let claimed_at = start + KNOCK_POLL_OBSERVATION_WINDOW * 3;
        for _ in 0..2 {
            assert_eq!(
                observations.observe(true, claimed_at),
                KnockPollObservation::ClaimSpan
            );
        }
        assert_eq!(
            observations.observe(false, claimed_at),
            KnockPollObservation::ClaimSpan,
            "the first empty poll after a claim is an observed transition"
        );
        assert_eq!(
            observations.observe(false, claimed_at),
            KnockPollObservation::PollPoint
        );
        assert_eq!(
            observations.observe(false, claimed_at),
            KnockPollObservation::Quiet
        );
        assert_eq!(
            observations.observe(false, claimed_at + KNOCK_POLL_OBSERVATION_WINDOW),
            KnockPollObservation::PollPoint
        );
    }

    #[test]
    fn host_insula_binding_uses_configured_identity_and_host_session() {
        let binding =
            host_insula_binding(&config(KnockAutonomy::Off)).expect("Host binding is valid");

        assert_eq!(binding.house_id, "solarisael");
        assert_eq!(binding.room, "kodo");
        assert_eq!(binding.spirit, "Kodo");
        assert_eq!(binding.session_id, "host:kodo");
    }

    #[test]
    fn host_observation_outcomes_distinguish_refusals_from_errors() {
        assert_eq!(
            app_error_outcome(&AppError::Invalid("invalid".into())),
            OutcomeClass::Refused
        );
        assert_eq!(
            app_error_outcome(&AppError::Refusal {
                code: "refused",
                message: "refused",
            }),
            OutcomeClass::Refused
        );
        assert_eq!(
            app_error_outcome(&AppError::Config("failed".into())),
            OutcomeClass::Error
        );
    }

    #[test]
    fn knock_authority_refuses_every_caller_while_autonomy_is_off() {
        let config = config(KnockAutonomy::Off);
        let own = knock_authority(&config, &meta("kodo", "Kodo"))
            .expect_err("a disabled Host must refuse even its own room");
        assert_eq!(refusal_code(own), "knock_autonomy_disabled");
        let foreign = knock_authority(&config, &meta("kintsu", "Kintsu"))
            .expect_err("a disabled Host must refuse a foreign room");
        assert_eq!(refusal_code(foreign), "knock_autonomy_disabled");
    }

    #[test]
    fn enabled_knock_authority_admits_only_this_hosts_own_room_and_spirit() {
        let config = config(KnockAutonomy::Claim);
        knock_authority(&config, &meta("kodo", "Kodo")).expect("the Host's own binding is allowed");

        for (room, spirit) in [("kintsu", "Kodo"), ("kodo", "Kintsu"), ("kintsu", "Kintsu")] {
            let error = knock_authority(&config, &meta(room, spirit))
                .expect_err("a foreign room or spirit must be refused");
            assert_eq!(refusal_code(error), "foreign_knock_authority");
        }
    }

    #[test]
    fn familiar_status_defaults_to_configured_room_dir() {
        let config = config(KnockAutonomy::Off);

        let resolved =
            resolve_room_dir(None, &config).expect("empty room_dir uses the configured one");
        assert_eq!(resolved.as_ref(), "configured-room");

        let blank =
            resolve_room_dir(Some("   "), &config).expect("blank room_dir uses the configured one");
        assert_eq!(blank.as_ref(), "configured-room");
    }

    #[test]
    fn caller_selected_room_dir_is_refused() {
        let config = config(KnockAutonomy::Off);

        let rejection = resolve_room_dir(Some("C:/somewhere/else"), &config)
            .expect_err("a foreign room_dir must be rejected");
        assert_eq!(rejection["status"], "rejected");
        assert_eq!(rejection["ok"], false);
    }

    #[test]
    fn canonical_equal_room_dir_is_admitted() {
        let dir = std::env::temp_dir().join(format!("athanor-room-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");
        let mut config = config(KnockAutonomy::Off);
        config.room_dir = dir.clone();

        let supplied = dir.to_string_lossy().to_string();
        let resolved =
            resolve_room_dir(Some(&supplied), &config).expect("the configured dir is admitted");
        assert_eq!(resolved.as_ref(), dir.to_string_lossy().as_ref());

        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn hallway_projection_rings_once_per_host_observed_revision() {
        assert!(!hallway_projection_changed(None, "quiet", false));
        assert!(hallway_projection_changed(None, "ringing", true));
        assert!(!hallway_projection_changed(
            Some("ringing"),
            "ringing",
            true
        ));
        assert!(hallway_projection_changed(Some("ringing"), "quiet", false));
    }
}
