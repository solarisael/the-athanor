//! Portable sessions: one authenticated door per room for exact live instances.
//!
//! Wire: local portable-session-wire v1 (POST /room/<room>/session). The Host
//! serving the room is the only authority for spirit, operator and home; a
//! caller names an instance, never a room, home path, or authority. Workspace
//! and cwd never select a target: every control addresses one instanceId.

use super::{AppState, authorized};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use interactive_process::ProcessWatch;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

#[path = "portable/attach.rs"]
mod attach;
#[path = "portable/control.rs"]
mod control;
#[path = "portable/home.rs"]
mod home;
#[path = "portable/launch.rs"]
mod launch;
#[path = "portable/ledger.rs"]
mod ledger;
#[path = "portable/restart.rs"]
mod restart;
#[path = "portable/status.rs"]
mod status;

pub(crate) const SESSION_PATH: &str = "/session";
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const LEASE_SECS: i64 = 30;
const HEARTBEAT_SECS: i64 = 10;
const COMMAND_ACK_SECS: i64 = 120;
const RETIRED_BOUND: usize = 4096;
const COMMAND_BOUND: usize = 4096;
const CONTROLS: &[&str] = &["deliver", "interrupt", "compact", "close", "restart"];
const KEEPER_EXIT_CODE: i32 = 87;

// ---------- refusals ----------

#[derive(Debug)]
pub(super) struct Refusal {
    status: StatusCode,
    code: &'static str,
    message: String,
    extra: Map<String, Value>,
}

impl Refusal {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            extra: Map::new(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }
    fn with(mut self, key: &str, value: Value) -> Self {
        self.extra.insert(key.to_owned(), value);
        self
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let mut error = self.extra;
        error.insert("code".into(), json!(self.code));
        error.insert("message".into(), json!(self.message));
        (self.status, Json(json!({"ok": false, "error": error}))).into_response()
    }
}

// ---------- registry ----------

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Terminal {
    kind: TerminalKind,
    session: String,
    pane: String,
    socket: String,
    executable: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TerminalKind {
    Tern,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum LauncherRequest {
    /// `environment` carries only the OMP profile selection (launch.rs PROFILE_ENV).
    Tern {
        program: String,
        args: Vec<String>,
        environment: BTreeMap<String, String>,
    },
    Keeper {
        keeper_instance: Uuid,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LauncherKind {
    Tern,
    Keeper,
}

impl LauncherKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tern => "tern",
            Self::Keeper => "keeper",
        }
    }
}

struct Instance {
    conversation_id: String,
    workspace: String,
    session_file: String,
    process_id: u32,
    watch: Option<Arc<ProcessWatch>>,
    controls: Vec<String>,
    terminal: Option<Terminal>,
    launcher: Option<LauncherKind>,
    launch: Option<launch::Launch>,
    lease_expires: DateTime<Utc>,
    attached_at: DateTime<Utc>,
}

impl Instance {
    /// An observation error is unknown, never proof of exit.
    fn exited(&self) -> bool {
        self.watch
            .as_ref()
            .is_some_and(|watch| matches!(watch.has_exited(), Ok(true)))
    }
    fn connected(&self, now: DateTime<Utc>) -> bool {
        self.lease_expires > now && !self.exited()
    }
    /// Restart support is a launcher property, separate from authorization.
    fn restart_support(&self) -> Result<LauncherKind, &'static str> {
        if !self.controls.iter().any(|control| control == "restart") {
            return Err("the adapter did not offer restart");
        }
        if self.watch.is_none() {
            return Err("the Host cannot observe this process exit");
        }
        match self.launcher {
            Some(LauncherKind::Tern) if self.terminal.is_some() && self.launch.is_some() => {
                Ok(LauncherKind::Tern)
            }
            Some(LauncherKind::Keeper) => Ok(LauncherKind::Keeper),
            _ => Err("no registered launcher can relaunch this session"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandState {
    /// Journaled with its identity before any effect; never delivered. A Host
    /// that restarts on this state answers outcomeUnknown and replays nothing.
    Pending,
    Queued,
    Delivered,
    Applied,
    Refused,
    Failed,
    Expired,
    /// The Host restarted before this control settled; it is never replayed.
    OutcomeUnknown,
}

impl CommandState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Delivered => "delivered",
            Self::Applied => "applied",
            Self::Refused => "refused",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::OutcomeUnknown => "outcomeUnknown",
        }
    }
    fn terminal(self) -> bool {
        !matches!(self, Self::Pending | Self::Queued | Self::Delivered)
    }
    /// What poll may hand to the target.
    fn deliverable(self) -> bool {
        matches!(self, Self::Queued | Self::Delivered)
    }
}

struct Command {
    /// sha256 of the issuing request body; durable across Host restarts.
    fingerprint: String,
    kind: String,
    payload: Value,
    target: Uuid,
    issuer: Option<Uuid>,
    issued_at: DateTime<Utc>,
    deadline_at: DateTime<Utc>,
    state: CommandState,
    detail: Option<String>,
    restart_id: Option<Uuid>,
    /// Frozen at issue: a command never lands on a later conversation.
    conversation_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RestartStage {
    Requested,
    AwaitingExit,
    Relaunching,
    HandedToKeeper,
    Verified,
    Failed,
}

impl RestartStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::AwaitingExit => "awaitingExit",
            Self::Relaunching => "relaunching",
            Self::HandedToKeeper => "handedToKeeper",
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone)]
pub(super) struct RestartRecord {
    intent_id: String,
    target: Uuid,
    conversation_id: String,
    workspace: String,
    session_file: String,
    reason: String,
    launcher: LauncherKind,
    stage: RestartStage,
    detail: Option<String>,
    terminal: Option<Terminal>,
    launch: Option<launch::Launch>,
    claim_token: Option<String>,
    successor_proof: Option<String>,
    expected_pane: Option<String>,
    relaunch_by: Option<DateTime<Utc>>,
    successor: Option<Uuid>,
    fresh: bool,
    /// The successor has been told it is one (attach or heartbeat).
    reported: bool,
    updated_at: DateTime<Utc>,
}

#[derive(Default)]
pub(crate) struct Registry {
    instances: HashMap<Uuid, Instance>,
    retired: HashSet<Uuid>,
    retired_order: VecDeque<Uuid>,
    commands: HashMap<Uuid, Command>,
    command_order: VecDeque<Uuid>,
    chat_owner: Option<Uuid>,
    restarts: HashMap<Uuid, RestartRecord>,
}

impl Registry {
    fn live(&self, now: DateTime<Utc>) -> impl Iterator<Item = (&Uuid, &Instance)> {
        self.instances
            .iter()
            .filter(move |(_, instance)| instance.connected(now))
    }

    fn chat_owner_live(&self, now: DateTime<Utc>) -> Option<Uuid> {
        self.chat_owner.filter(|owner| {
            self.instances
                .get(owner)
                .is_some_and(|instance| instance.connected(now))
        })
    }

    /// `Some(exited)` while the exit can be observed; `None` when unknown.
    fn process_exited(&self, id: Uuid) -> Option<bool> {
        let instance = self.instances.get(&id)?;
        instance.watch.as_ref()?.has_exited().ok()
    }

    fn retire(&mut self, id: Uuid, why: &str) {
        self.instances.remove(&id);
        if self.chat_owner == Some(id) {
            self.chat_owner = None;
        }
        for command in self.commands.values_mut() {
            if command.target == id && !command.state.terminal() {
                command.state = CommandState::Expired;
                command.detail = Some(format!("target {why}"));
            }
        }
        if self.retired.insert(id) {
            self.retired_order.push_back(id);
            while self.retired_order.len() > RETIRED_BOUND {
                if let Some(old) = self.retired_order.pop_front() {
                    self.retired.remove(&old);
                }
            }
        }
    }

    fn expire(&mut self, now: DateTime<Utc>) {
        for command in self.commands.values_mut() {
            if !command.state.terminal() && command.deadline_at <= now {
                command.state = CommandState::Expired;
            }
        }
    }

    /// The exact live registration, or the reason there is none.
    fn bound(
        &mut self,
        id: Uuid,
        conversation_id: &str,
        now: DateTime<Utc>,
    ) -> Result<&mut Instance, Refusal> {
        if self.retired.contains(&id) {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "instance_stale",
                "this instance was retired; a successor or detach ended it",
            ));
        }
        let instance = self.instances.get_mut(&id).ok_or_else(|| {
            Refusal::new(StatusCode::NOT_FOUND, "instance_unknown", "attach first")
        })?;
        if instance.conversation_id != conversation_id {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "conversation_mismatch",
                "conversationId does not match the registration",
            ));
        }
        if instance.lease_expires <= now || instance.exited() {
            return Err(Refusal::new(
                StatusCode::GONE,
                "lease_expired",
                "the lease lapsed; attach again",
            ));
        }
        instance.lease_expires = now + chrono::Duration::seconds(LEASE_SECS);
        Ok(instance)
    }

    fn pending_for(&self, id: Uuid) -> usize {
        self.commands
            .values()
            .filter(|command| command.target == id && !command.state.terminal())
            .count()
    }

    /// The WS chatNext gate: with live portable sessions only the explicit owner receives turns.
    pub(crate) fn chat_recipient(&self, instance_id: Option<&str>) -> Result<(), Option<Uuid>> {
        let now = Utc::now();
        if self.live(now).next().is_none() {
            return Ok(());
        }
        let owner = self.chat_owner_live(now);
        match (owner, instance_id.and_then(|id| Uuid::parse_str(id).ok())) {
            (Some(owner), Some(asker)) if owner == asker => Ok(()),
            _ => Err(owner),
        }
    }
}

// ---------- requests ----------

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Request {
    Attach {
        conversation_id: String,
        instance_id: Uuid,
        workspace: String,
        harness: String,
        session_file: String,
        process_id: u32,
        controls: Vec<String>,
        #[serde(default)]
        chat_delivery: bool,
        #[serde(default)]
        terminal: Option<Terminal>,
        #[serde(default)]
        launcher: Option<LauncherRequest>,
    },
    Context {
        instance_id: Uuid,
        conversation_id: String,
    },
    Heartbeat {
        instance_id: Uuid,
        conversation_id: String,
    },
    Detach {
        instance_id: Uuid,
        conversation_id: String,
        #[serde(default)]
        reason: Option<String>,
    },
    ChatDelivery {
        instance_id: Uuid,
        conversation_id: String,
        claim: bool,
    },
    Status,
    Command {
        command_id: Uuid,
        target_instance_id: Uuid,
        target_conversation_id: String,
        #[serde(default)]
        issuer_instance_id: Option<Uuid>,
        kind: String,
        payload: Value,
    },
    CommandStatus {
        command_id: Uuid,
    },
    Poll {
        instance_id: Uuid,
        conversation_id: String,
    },
    Acknowledge {
        instance_id: Uuid,
        conversation_id: String,
        command_id: Uuid,
        outcome: String,
        #[serde(default)]
        detail: Option<String>,
    },
    ResourceList {
        instance_id: Uuid,
        conversation_id: String,
        #[serde(default)]
        prefix: Option<String>,
    },
    ResourceRead {
        instance_id: Uuid,
        conversation_id: String,
        path: String,
    },
    ResourceWrite {
        instance_id: Uuid,
        conversation_id: String,
        path: String,
        text: String,
        expected_revision: Option<String>,
    },
}

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .route(SESSION_PATH, post(session))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<Value>, JsonRejection>,
) -> Response {
    if !authorized(&headers, &state.config.bearer_token) {
        return Refusal::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Authorization: Bearer <token> is required",
        )
        .into_response();
    }
    let Ok(Json(body)) = body else {
        return Refusal::invalid("body must be one JSON object").into_response();
    };
    let request: Request = match serde_json::from_value(body.clone()) {
        Ok(request) => request,
        Err(error) => return Refusal::invalid(error.to_string()).into_response(),
    };
    let read_only = matches!(
        body["action"].as_str(),
        Some(
            "status"
                | "commandStatus"
                | "context"
                | "resourceList"
                | "resourceRead"
                | "resourceWrite"
        )
    );
    let answer = handle(&state, request, body).await;
    if !read_only {
        // Persist before answering: an answered control must survive a Host restart.
        let registry = state.portable.lock().await;
        if let Err(error) = ledger::save(&state.config.state_dir, &registry) {
            tracing::error!(%error, "portable session ledger write failed");
            return Refusal::internal(error).into_response();
        }
    }
    match answer {
        Ok(Value::Object(mut answer)) => {
            answer.insert("ok".into(), json!(true));
            Json(Value::Object(answer)).into_response()
        }
        Ok(_) => Refusal::internal("answer was not an object").into_response(),
        Err(refusal) => refusal.into_response(),
    }
}

fn bounded(value: &str, field: &str, max: usize) -> Result<(), Refusal> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(Refusal::invalid(format!(
            "{field} must be 1..{max} printable bytes"
        )));
    }
    Ok(())
}

fn absolute(value: &str, field: &str) -> Result<(), Refusal> {
    bounded(value, field, 4096)?;
    if !Path::new(value).is_absolute() {
        return Err(Refusal::invalid(format!(
            "{field} must be an absolute path"
        )));
    }
    Ok(())
}

fn context_json(context: &home::Context) -> Value {
    json!({"revision": context.revision, "text": context.text, "sources": context.sources})
}

fn now_text(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

async fn handle(state: &AppState, request: Request, body: Value) -> Result<Value, Refusal> {
    let now = Utc::now();
    match request {
        Request::Attach {
            conversation_id,
            instance_id,
            workspace,
            harness,
            session_file,
            process_id,
            controls,
            chat_delivery,
            terminal,
            launcher,
        } => {
            let request = attach::Attach {
                conversation_id,
                instance_id,
                workspace,
                harness,
                session_file,
                process_id,
                controls,
                chat_delivery,
                terminal,
                launcher,
            };
            attach::attach(state, now, request).await
        }
        Request::Context {
            instance_id,
            conversation_id,
        } => attach::context(state, now, instance_id, conversation_id).await,
        Request::Heartbeat {
            instance_id,
            conversation_id,
        } => attach::heartbeat(state, now, instance_id, conversation_id).await,
        Request::Detach {
            instance_id,
            conversation_id,
            reason,
        } => attach::detach(state, instance_id, conversation_id, reason).await,
        Request::ChatDelivery {
            instance_id,
            conversation_id,
            claim,
        } => attach::chat_delivery(state, now, instance_id, conversation_id, claim).await,
        Request::Status => status::status(state, now).await,
        Request::Command {
            command_id,
            target_instance_id,
            target_conversation_id,
            issuer_instance_id,
            kind,
            payload,
        } => {
            let target = (target_instance_id, target_conversation_id);
            control::command(
                state,
                now,
                command_id,
                target,
                issuer_instance_id,
                kind,
                payload,
                body,
            )
            .await
        }
        Request::CommandStatus { command_id } => {
            control::command_status(state, now, command_id).await
        }
        Request::Poll {
            instance_id,
            conversation_id,
        } => control::poll(state, now, instance_id, conversation_id).await,
        Request::Acknowledge {
            instance_id,
            conversation_id,
            command_id,
            outcome,
            detail,
        } => {
            control::acknowledge(
                state,
                now,
                instance_id,
                conversation_id,
                command_id,
                outcome,
                detail,
            )
            .await
        }
        Request::ResourceList {
            instance_id,
            conversation_id,
            prefix,
        } => {
            state
                .portable
                .lock()
                .await
                .bound(instance_id, &conversation_id, now)?;
            home::list(&state.config.room_dir, prefix.as_deref())
        }
        Request::ResourceRead {
            instance_id,
            conversation_id,
            path,
        } => {
            state
                .portable
                .lock()
                .await
                .bound(instance_id, &conversation_id, now)?;
            home::read(&state.config.room_dir, &path)
        }
        Request::ResourceWrite {
            instance_id,
            conversation_id,
            path,
            text,
            expected_revision,
        } => {
            // The registry lock serializes compare-and-write for this home.
            let mut registry = state.portable.lock().await;
            registry.bound(instance_id, &conversation_id, now)?;
            home::write(
                &state.config.room_dir,
                &path,
                &text,
                expected_revision.as_deref(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(conversation: &str, lease: i64) -> Instance {
        Instance {
            conversation_id: conversation.into(),
            workspace: "C:/w".into(),
            session_file: "C:/s.jsonl".into(),
            process_id: 1,
            watch: None,
            controls: vec!["deliver".into(), "restart".into()],
            terminal: None,
            launcher: None,
            launch: None,
            lease_expires: Utc::now() + chrono::Duration::seconds(lease),
            attached_at: Utc::now(),
        }
    }

    #[test]
    fn exact_instance_binding_refuses_stale_mismatched_and_lapsed() {
        let mut registry = Registry::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        registry.instances.insert(a, instance("c1", 30));
        registry.instances.insert(b, instance("c1", -1));
        let now = Utc::now();
        assert!(registry.bound(a, "c1", now).is_ok());
        assert_eq!(
            registry.bound(a, "c2", now).err().unwrap().code,
            "conversation_mismatch"
        );
        assert_eq!(
            registry.bound(b, "c1", now).err().unwrap().code,
            "lease_expired"
        );
        registry.retire(a, "detached");
        assert_eq!(
            registry.bound(a, "c1", now).err().unwrap().code,
            "instance_stale"
        );
        assert_eq!(
            registry
                .bound(Uuid::new_v4(), "c1", now)
                .err()
                .unwrap()
                .code,
            "instance_unknown"
        );
    }

    #[test]
    fn chat_goes_only_to_the_explicit_owner_once_portable_sessions_live() {
        let mut registry = Registry::default();
        assert!(
            registry.chat_recipient(None).is_ok(),
            "standalone path keeps legacy delivery"
        );
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        registry.instances.insert(a, instance("c1", 30));
        registry.instances.insert(b, instance("c2", 30));
        assert_eq!(registry.chat_recipient(Some(&a.to_string())), Err(None));
        registry.chat_owner = Some(a);
        assert!(registry.chat_recipient(Some(&a.to_string())).is_ok());
        assert_eq!(registry.chat_recipient(Some(&b.to_string())), Err(Some(a)));
        assert_eq!(registry.chat_recipient(None), Err(Some(a)));
    }

    #[test]
    fn restart_support_is_separate_from_offered_controls() {
        let mut offered = instance("c1", 30);
        assert!(offered.restart_support().is_err(), "no watch, no launcher");
        offered.controls.retain(|control| control != "restart");
        assert_eq!(
            offered.restart_support().err(),
            Some("the adapter did not offer restart")
        );
    }

    #[test]
    fn tern_launch_refuses_session_selectors_and_uncarried_environment() {
        let program = std::env::current_exe().unwrap().display().to_string();
        let none = BTreeMap::new;
        assert!(launch::Launch::validated(&program, &["--resume".into()], none()).is_err());
        assert!(
            launch::Launch::validated(&program, &["--model".into(), "x".into()], none()).is_ok()
        );
        assert!(launch::Launch::validated("relative.exe", &[], none()).is_err());
        let profile = BTreeMap::from([("OMP_PROFILE".to_owned(), "work".to_owned())]);
        assert!(launch::Launch::validated(&program, &[], profile).is_ok());
        let secret = BTreeMap::from([("OPENAI_API_KEY".to_owned(), "sk".to_owned())]);
        assert!(launch::Launch::validated(&program, &[], secret).is_err());
    }

    #[test]
    fn tern_launch_binds_only_to_the_observed_process_image() {
        let own = interactive_process::ProcessWatch::open(std::process::id()).unwrap();
        let other = if cfg!(windows) {
            "C:\\Windows\\System32\\cmd.exe"
        } else {
            "/bin/sh"
        };
        let launch = launch::Launch::validated(other, &[], BTreeMap::new()).unwrap();
        assert!(launch.bind(Some(&own), None).is_err());
    }

    #[test]
    fn tern_ids_read_the_same_as_numbers_or_strings() {
        assert_eq!(
            restart::id_text(&json!(137438953474u64)).as_deref(),
            Some("137438953474")
        );
        assert_eq!(
            restart::id_text(&json!("137438953474")).as_deref(),
            Some("137438953474")
        );
        assert_eq!(restart::id_text(&json!(null)), None);
    }

    // ---------- restart authority against a real substrate ----------

    const RESTART_MIGRATIONS: [&str; 3] = [
        include_str!("../../../substrate/migrations/0026_restart.sql"),
        include_str!("../../../substrate/migrations/0027_restart_successor_proof.sql"),
        include_str!("../../../substrate/migrations/0036_restart_requester_instance.sql"),
    ];
    const CLAIM_FILE: &str = "restart-capability";
    const VERIFY_FILE: &str = "restart-verify-capability";
    const CLAIM_SECRET: &str = "keeper-claim-secret";
    const VERIFY_SECRET: &str = "room-verify-secret";
    const CONVERSATION: &str = "3f1c9a6e-70b4-4a2f-9d13-5c8e2b7a4d61";

    fn capability_hash(secret: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(secret.as_bytes()))
    }

    /// One room whose Host holds real restart authority: the four holder files
    /// provisioning writes, their hashes in a scratch substrate, and a live
    /// keeper-launched target, which is this test process.
    struct RestartRoom {
        state: AppState,
        pool: sqlx::PgPool,
        directory: std::path::PathBuf,
        target: Uuid,
    }

    impl RestartRoom {
        async fn open() -> Self {
            let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
                .expect("the restart authority proof requires a dedicated PostgreSQL URL");
            let lower = url.to_ascii_lowercase();
            assert!(
                !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
                "refusing a live or production-looking database"
            );
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(2)
                .connect(&url)
                .await
                .unwrap();
            // Like the native lifecycle proof, this owns only the restart schema
            // of the dedicated database and starts from an empty plane.
            sqlx::query("DROP SCHEMA IF EXISTS restart CASCADE")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
            )
            .execute(&pool)
            .await
            .unwrap();
            for migration in RESTART_MIGRATIONS {
                sqlx::raw_sql(migration).execute(&pool).await.unwrap();
            }
            let directory =
                std::env::temp_dir().join(format!("athanor-restart-{}", Uuid::new_v4()));
            let config = crate::config::HostConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                bearer_token: "restart-test".into(),
                room_dir: directory.clone(),
                state_dir: directory.join("host"),
                house_id: "solarisael".into(),
                room: "kodo".into(),
                spirit: "Kodo".into(),
                session: "restart-session".into(),
                database_url: None,
                nats_url: None,
                nats_auth: None,
                knock_autonomy: crate::config::KnockAutonomy::Off,
                operator: None,
            };
            std::fs::create_dir_all(directory.join(".omp").join("runtime")).unwrap();
            std::fs::create_dir_all(&config.state_dir).unwrap();
            let room_state = json!({
                "room": "kodo", "operator": "Sol", "embodiedSpirit": "Kodo",
                "recallPolicy": {
                    "requestedMode": "auto", "resolvedMode": "conversation",
                    "workingSetEntries": 0, "recoveryPending": false, "recoveryTerms": [],
                    "activeProject": null, "resolutionReason": "test",
                    "lastRefreshReason": null, "lastRefreshAt": null,
                    "degraded": null, "updatedAt": null
                }
            });
            std::fs::write(config.room_state_path(), room_state.to_string()).unwrap();
            let session_file = directory.join("session.jsonl");
            std::fs::write(&session_file, "{\"type\":\"session\"}\n").unwrap();
            let state = super::super::Host::new(
                config,
                Some(pool.clone()),
                tokio_util::sync::CancellationToken::new(),
                tokio_util::task::TaskTracker::new(),
            )
            .unwrap()
            .state;
            let target = Uuid::new_v4();
            state.portable.lock().await.instances.insert(
                target,
                Instance {
                    conversation_id: CONVERSATION.into(),
                    workspace: "D:/athanor-wt/host-restart".into(),
                    session_file: session_file.display().to_string(),
                    process_id: std::process::id(),
                    watch: Some(Arc::new(ProcessWatch::open(std::process::id()).unwrap())),
                    controls: vec!["restart".into()],
                    terminal: None,
                    launcher: Some(LauncherKind::Keeper),
                    launch: None,
                    lease_expires: Utc::now() + chrono::Duration::seconds(300),
                    attached_at: Utc::now(),
                },
            );
            let room = Self {
                state,
                pool,
                directory,
                target,
            };
            for (principal, class, file, secret) in [
                ("omp-keeper", "restart_claim", CLAIM_FILE, CLAIM_SECRET),
                ("kodo", "restart_request", "restart-request-capability", "room-request-secret"),
                ("kodo", "restart_exit", "restart-exit-capability", "room-exit-secret"),
                ("kodo", "restart_verify", VERIFY_FILE, VERIFY_SECRET),
            ] {
                room.provision(principal, class, secret).await;
                room.hold(file, secret);
            }
            room
        }

        /// The substrate half of provisioning: the sha256 only, like the ritual.
        async fn provision(&self, principal: &str, class: &str, secret: &str) {
            sqlx::query(
                "INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash) VALUES ($1, $2, $3) ON CONFLICT (principal, operation_class) DO UPDATE SET capability_hash = EXCLUDED.capability_hash",
            )
            .bind(principal)
            .bind(class)
            .bind(capability_hash(secret))
            .execute(&self.pool)
            .await
            .unwrap();
        }

        /// The holder half: the room's runtime file the Host reads.
        fn hold(&self, file: &str, secret: &str) {
            std::fs::write(self.holder(file), secret).unwrap();
        }

        fn lose(&self, file: &str) {
            std::fs::remove_file(self.holder(file)).unwrap();
        }

        fn holder(&self, file: &str) -> std::path::PathBuf {
            self.directory.join(".omp").join("runtime").join(file)
        }

        async fn ask(&self, command_id: Uuid) -> Result<Value, Refusal> {
            let payload =
                json!({"reason": "the loader installed a newer release", "mode": "resume"});
            control::command(
                &self.state,
                Utc::now(),
                command_id,
                (self.target, CONVERSATION.to_owned()),
                None,
                "restart".into(),
                payload.clone(),
                json!({"kind": "restart", "payload": payload}),
            )
            .await
        }

        async fn acknowledge(&self, command_id: Uuid) -> Result<Value, Refusal> {
            control::acknowledge(
                &self.state,
                Utc::now(),
                self.target,
                CONVERSATION.to_owned(),
                command_id,
                "applied".into(),
                None,
            )
            .await
        }

        async fn intents(&self) -> Vec<(String, String)> {
            sqlx::query_as("SELECT intent_id::text, state FROM restart.intents ORDER BY created_at")
                .fetch_all(&self.pool)
                .await
                .unwrap()
        }
    }

    impl Drop for RestartRoom {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    // Kintsu, 2026-10-13: provisioning another room rotated the shared
    // omp-keeper secret and rewrote only that room's holder, so Kintsu's holder
    // predated it. The Host requested and armed, the predecessor exited, and
    // only then did the claim refuse; the intent stayed exiting with nobody
    // able to claim it. The claim and verify secrets now meet their fence before
    // any intent exists and again before the answer that lets the adapter exit.
    // red-proof: drop require_relaunch_authority from control::command (the
    // stale holder mints a requested intent) or from restart::arm (the rotated
    // claim reaches exiting).
    #[tokio::test]
    #[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated restart schema"]
    async fn a_failing_post_exit_secret_refuses_the_restart_while_the_predecessor_runs() {
        let room = RestartRoom::open().await;
        let ask = Uuid::new_v4();

        // Request time. Each refusal journals and mints nothing, so the same
        // command id is free again once the holder is repaired.
        let rotated_away = "a-keeper-secret-from-before-the-rotation";
        room.hold(CLAIM_FILE, rotated_away);
        let stale = room.ask(ask).await.unwrap_err();
        assert_eq!(
            (stale.status, stale.code),
            (StatusCode::CONFLICT, "restart_capability")
        );
        assert!(stale.message.contains(CLAIM_FILE), "{}", stale.message);
        assert!(
            !stale.message.contains(rotated_away),
            "a refusal never carries the secret"
        );
        room.lose(CLAIM_FILE);
        let missing = room.ask(ask).await.unwrap_err();
        assert_eq!(
            (missing.status, missing.code),
            (StatusCode::SERVICE_UNAVAILABLE, "restart_authority_unavailable")
        );
        room.hold(CLAIM_FILE, CLAIM_SECRET);
        room.lose(VERIFY_FILE);
        let unverifiable = room.ask(ask).await.unwrap_err();
        assert_eq!(unverifiable.code, "restart_authority_unavailable");
        assert!(unverifiable.message.contains(VERIFY_FILE), "{}", unverifiable.message);
        room.hold(VERIFY_FILE, "not-this-room-verify-secret");
        let stale_verify = room.ask(ask).await.unwrap_err();
        assert_eq!(stale_verify.code, "restart_capability");
        assert!(stale_verify.message.contains(VERIFY_FILE), "{}", stale_verify.message);
        assert!(
            room.intents().await.is_empty(),
            "no refused request minted an intent"
        );
        {
            let registry = room.state.portable.lock().await;
            assert!(
                registry.commands.is_empty() && registry.restarts.is_empty(),
                "no refused request took a journal entry"
            );
        }

        // Every holder current: the same command id queues one requested intent.
        room.hold(VERIFY_FILE, VERIFY_SECRET);
        assert_eq!(room.ask(ask).await.unwrap()["state"], "queued");
        let minted = room.intents().await;
        assert_eq!(minted.len(), 1);
        assert_eq!(minted[0].1, "requested");

        // Acknowledge time. The keeper principal rotates after the request and
        // this holder keeps the old secret: the acknowledgement is refused, so
        // the adapter never receives the answer that lets it exit, and the
        // intent never arms.
        let rotated = "the-keeper-secret-after-the-rotation";
        room.provision("omp-keeper", "restart_claim", rotated).await;
        let unarmed = room.acknowledge(ask).await.unwrap_err();
        assert_eq!(
            (unarmed.status, unarmed.code),
            (StatusCode::CONFLICT, "restart_capability")
        );
        assert!(unarmed.message.contains(CLAIM_FILE), "{}", unarmed.message);
        assert_eq!(
            room.intents().await,
            minted,
            "the refused acknowledgement armed nothing"
        );

        // The holder catches up and the restart path is whole again. Nothing
        // could claim the leftover request to fail it, so it holds the
        // workspace until its TTL passes and the next request sweeps it.
        room.hold(CLAIM_FILE, rotated);
        sqlx::query(
            "UPDATE restart.intents SET expires_at=clock_timestamp()-INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
        )
        .bind(&minted[0].0)
        .execute(&room.pool)
        .await
        .unwrap();
        let again = Uuid::new_v4();
        assert_eq!(room.ask(again).await.unwrap()["state"], "queued");
        assert_eq!(room.acknowledge(again).await.unwrap()["state"], "applied");
        let states: Vec<String> = room
            .intents()
            .await
            .into_iter()
            .map(|(_, state)| state)
            .collect();
        assert_eq!(states, ["expired", "exiting"]);
        // The applied acknowledgement started the exit watch; stop it first.
        room.state.cancellation.cancel();
        room.state.tasks.close();
        room.state.tasks.wait().await;
    }
}
