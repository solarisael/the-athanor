//! The room's live Orihon owner. One task per Host room holds the exclusive chat lock, the
//! persisted chat and compactor views, the ready-node queues and the compaction engine.
//! Turns and child views reach it through a bounded command channel; a capture only marks a
//! message boundary. Nothing here scans the tree for work: a built node readies at most its
//! parent, and leaves start from a window over the unbuilt ones.
//!
//! Only consent spends. A fold session takes part through an explicit fold mode change or a
//! verified live turn, and stops through log, off, or leaving the Host's registered
//! conversations. Without a participant no engine opens and no model job starts; a restart
//! or a saved configuration alone never resumes paid work, and a status read is not consent.

use super::storage_error;
use crate::HostConfig;
use akasha::orihon_folder::{Completion, Engine, Task, model::FolderConfig};
use futures_util::FutureExt;
use hearth::orihon::live::{LIVE_INSTRUCTIONS, LIVE_VERSION, compress_task, merge_task};
use hearth::orihon::{FolderBackend, FolderSettings, Kind, MAX_BATCH_RECORDS, Record};
use origami::orihon::Error;
use origami::orihon::live::store::{
    self, Job, Live, LiveConfig, Outcome, Prefix, SessionRecord, Snapshot, Store,
};
use origami::orihon::live::{
    self as tree, LEAF_WINDOW, LINE_BYTES, MAX_JOBS, MODEL_LINE_CEILING, Method, NodeKey, Rendered,
    View,
};
use protocol::ProtocolErrorBody;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::{JoinError, JoinSet};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

/// How often a PostgreSQL owner proves its room lock connection still answers.
const OWNER_CHECK: std::time::Duration = std::time::Duration::from_secs(30);
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(60);
/// Turn and child requests waiting for the owner; senders wait for room, nothing is dropped.
const COMMANDS: usize = 64;
/// Accepted file-room records kept for the owner's next ingest. Overflow costs one book read.
const HINT_RECORDS: usize = 4 * MAX_BATCH_RECORDS;
/// A turn boundary holds one user input and its records; more than this is stale.
const SESSION_TAIL: usize = 256;
const HARNESS: &str = "omp";
const WAITING_FOR_PREFIX: &str = "the compactor waits for the first live turn's prompt prefix";
/// Fold sessions that may take part at once; each is a registered conversation.
const PARTICIPANTS: usize = 64;

/// Reads the conversations the Host still registers. A participant outside them has left.
pub(crate) type Registered =
    Box<dyn Fn() -> futures_util::future::BoxFuture<'static, Vec<String>> + Send + Sync>;

type Reply = oneshot::Sender<Result<Value, ProtocolErrorBody>>;

/// The Host's handle on its room's live owner.
#[derive(Clone, Default)]
pub(crate) struct Room(Arc<Shared>);

#[derive(Default)]
struct Shared {
    commands: OnceLock<mpsc::Sender<Command>>,
    boundary: Notify,
    hints: Mutex<Vec<Record>>,
    status: Mutex<Value>,
}

enum Command {
    Join {
        session: String,
        folder: FolderSettings,
        explicit: bool,
    },
    Leave {
        session: String,
    },
    Turn(TurnRequest, Reply),
    Subagent(Reply),
}

/// A turn of an authenticated live session.
pub(crate) struct TurnRequest {
    pub session: String,
    pub through_source_id: Option<String>,
    pub prefix: Prefix,
    pub model: TurnModel,
    /// The session's own fold folder; it configures a room that has none.
    pub folder: FolderSettings,
    pub started: Instant,
    pub deadline: Instant,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TurnModel {
    pub provider: String,
    pub id: String,
}

fn refusal(code: &'static str, message: impl Into<String>) -> ProtocolErrorBody {
    ProtocolErrorBody::application(code, message).build()
}

fn closed() -> ProtocolErrorBody {
    refusal(
        "orihon_owner_closed",
        "this Host's Orihon live owner is not running",
    )
}

pub(super) fn not_live(message: &str) -> ProtocolErrorBody {
    refusal("orihon_not_live", message)
}

impl Room {
    /// A capture accepted new records: a message boundary. File rooms pass the accepted records
    /// so the owner need not reread the book.
    pub(crate) fn boundary(&self, records: Vec<Record>) {
        if !records.is_empty() {
            let mut hints = self.0.hints.lock().unwrap_or_else(PoisonError::into_inner);
            if hints.len() + records.len() > HINT_RECORDS {
                hints.clear();
            } else {
                hints.extend(records);
            }
        }
        self.0.boundary.notify_one();
    }

    /// An explicit fold mode change: the session takes part, so model work may run. An
    /// explicit folder becomes the room compactor configuration; an implicit one only fills
    /// a room that has none.
    pub(crate) async fn join(&self, session: String, folder: FolderSettings, explicit: bool) {
        // A Host without a running owner has nothing to take part in.
        let _ = self
            .send(Command::Join {
                session,
                folder,
                explicit,
            })
            .await;
    }

    /// An explicit log or off mode change: the session stops taking part. The last one to
    /// leave pauses the room's compaction.
    pub(crate) async fn leave(&self, session: String) {
        let _ = self.send(Command::Leave { session }).await;
    }

    pub(crate) async fn turn(&self, request: TurnRequest) -> Result<Value, ProtocolErrorBody> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Turn(request, reply)).await?;
        answer.await.map_err(|_| closed())?
    }

    pub(crate) async fn subagent(&self) -> Result<Value, ProtocolErrorBody> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Subagent(reply)).await?;
        answer.await.map_err(|_| closed())?
    }

    /// The owner's last published accounting.
    pub(crate) fn status(&self) -> Value {
        let status = self.0.status.lock().unwrap_or_else(PoisonError::into_inner);
        if status.is_null() {
            idle_status(Some("this Host's Orihon live owner is not running"))
        } else {
            status.clone()
        }
    }

    async fn send(&self, command: Command) -> Result<(), ProtocolErrorBody> {
        let sender = self.0.commands.get().ok_or_else(closed)?;
        sender.send(command).await.map_err(|_| closed())
    }

    fn take_hints(&self) -> Vec<Record> {
        std::mem::take(&mut *self.0.hints.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn publish(&self, status: Value) {
        *self.0.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }
}

fn idle_status(last_error: Option<&str>) -> Value {
    json!({"units":null,"leavesBuilt":null,"running":0,"failed":0,"queued":null,
        "chatBytes":null,"chatLines":null,"batch":null,"compactorBytes":null,
        "lastError":last_error,"prefixDigest":null,"folder":null,"participants":0})
}

/// Runs the room's live owner until the Host cancels, then cancels its compactions, stores
/// every job they return, closes the engine and releases the room lock.
pub(crate) async fn serve(
    room: Room,
    host: HostConfig,
    pool: Option<PgPool>,
    registered: Registered,
    cancellation: CancellationToken,
) {
    let store = if host.akasha_enabled() {
        let Some(pool) = pool else {
            // No command channel opens, so turns and child views refuse at once.
            room.publish(idle_status(Some(
                "Orihon PostgreSQL storage is unavailable",
            )));
            return;
        };
        Store::Postgres(pool)
    } else {
        Store::File(host.room_dir.join(hearth::orihon::DIRECTORY))
    };
    let (sender, commands) = mpsc::channel(COMMANDS);
    if room.0.commands.set(sender).is_err() {
        return;
    }
    let actor = Actor {
        room: room.clone(),
        name: host.room,
        store,
        owner: None,
        participants: BTreeSet::new(),
        jobs: JoinSet::new(),
        cancel: cancellation.child_token(),
        shutdown: cancellation.clone(),
        error: None,
    };
    serve_actor(actor, commands, registered, cancellation).await;
}

async fn serve_actor(
    mut actor: Actor,
    mut commands: mpsc::Receiver<Command>,
    registered: Registered,
    cancellation: CancellationToken,
) {
    let room = actor.room.clone();
    // The PostgreSQL room lock lives on one idle connection; it is proven, never assumed.
    // The same tick lets go of participants whose conversation the Host no longer registers.
    let mut liveness = tokio::time::interval(OWNER_CHECK);
    liveness.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        actor.publish();
        let deadline = actor.deadline();
        let retry_deadline = actor.retry_deadline();
        tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            Some(joined) = actor.jobs.join_next(), if !actor.jobs.is_empty() => {
                actor.finish(joined).await;
            }
            command = commands.recv() => match command {
                Some(command) => actor.command(command).await,
                None => break,
            },
            () = room.0.boundary.notified() => actor.boundary().await,
            () = sleep_until(deadline) => actor.expire().await,
            _ = liveness.tick() => {
                actor.check_owner().await;
                let conversations = registered().await;
                actor.reconcile(&conversations).await;
            }
            () = sleep_until(retry_deadline) => {
                actor.check_owner().await;
                let conversations = registered().await;
                actor.reconcile(&conversations).await;
                actor.retry().await;
            }
        }
        if cancellation.is_cancelled() {
            break;
        }

        actor.advance().await;
    }
    commands.close();
    while let Ok(command) = commands.try_recv() {
        match command {
            Command::Turn(_, reply) | Command::Subagent(reply) => {
                let _ = reply.send(Err(closed()));
            }
            Command::Join { .. } | Command::Leave { .. } => {}
        }
    }
    actor.close().await;
    room.publish(idle_status(Some(
        "the Host closed this room's Orihon live owner",
    )));
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

struct Finished {
    key: NodeKey,
    source_digest: String,
    /// `None` only when the engine panicked and left no receipt.
    completion: Option<Completion>,
}

struct Actor {
    room: Room,
    name: String,
    store: Store,
    owner: Option<Owner>,
    /// Fold sessions that consented in this Host's lifetime: an explicit fold change or a
    /// verified live turn adds one; log, off, or leaving the registered conversations
    /// removes it. Engines open and model jobs start only while one takes part.
    participants: BTreeSet<String>,
    jobs: JoinSet<Finished>,
    /// Cancels in-flight compactions when the last participant leaves and at shutdown.
    cancel: CancellationToken,
    shutdown: CancellationToken,
    error: Option<String>,
}

enum Opening {
    Busy,
    Failed(String),
}

impl Actor {
    async fn open(&mut self) -> Result<(), Opening> {
        if self.owner.is_some() {
            return Ok(());
        }
        match Live::open(self.store.clone(), &self.name).await {
            Ok(Some((live, snapshot))) => {
                self.owner = Some(Owner::restore(live, snapshot).map_err(Opening::Failed)?);
                Ok(())
            }
            Ok(None) => Err(Opening::Busy),
            Err(error) => Err(Opening::Failed(error.to_string())),
        }
    }

    fn note_opening(&mut self, opening: Opening) {
        self.error = Some(match opening {
            Opening::Busy => "another Host process owns this room's live chat".into(),
            Opening::Failed(message) => message,
        });
    }

    /// Opens the owner for a request. Opening restores what was persisted and starts no work.
    async fn opened(&mut self) -> Result<&mut Owner, ProtocolErrorBody> {
        match self.open().await {
            Ok(()) => Ok(self.owner.as_mut().expect("the owner is open")),
            Err(Opening::Busy) => Err(refusal(
                "orihon_owner_busy",
                "another Host process owns this room's live chat",
            )),
            Err(Opening::Failed(message)) => {
                Err(ProtocolErrorBody::application("orihon_io", message)
                    .retryable(true)
                    .build())
            }
        }
    }

    fn deadline(&self) -> Option<Instant> {
        self.owner
            .as_ref()
            .and_then(|owner| owner.waiters.iter().map(|waiter| waiter.deadline).min())
    }

    fn retry_deadline(&self) -> Option<Instant> {
        if self.participants.is_empty() {
            return None;
        }

        self.owner.as_ref().and_then(|owner| owner.retry_at)
    }

    fn publish(&self) {
        let mut status = match &self.owner {
            Some(owner) => {
                let last_error = self.error.as_deref().or(owner.note);
                owner.status(last_error)
            }
            None => idle_status(self.error.as_deref()),
        };
        status["participants"] = json!(self.participants.len());
        self.room.publish(status);
    }

    async fn command(&mut self, command: Command) {
        match command {
            Command::Join {
                session,
                folder,
                explicit,
            } => match self.join(&session, &folder, explicit).await {
                // Consent resumes queued and failed work at once, as a boundary does.
                Ok(()) => self.boundary().await,
                Err(error) => self.error = Some(error.message),
            },
            Command::Leave { session } => {
                if self.participants.remove(&session) && self.participants.is_empty() {
                    self.pause().await;
                }
            }
            Command::Turn(request, reply) => self.begin_turn(request, reply).await,
            Command::Subagent(reply) => {
                let _ = reply.send(self.subagent().await);
            }
        }
    }

    /// A session takes part. An explicit folder becomes the room compactor configuration;
    /// an implicit one only fills a room that has none.
    async fn join(
        &mut self,
        session: &str,
        folder: &FolderSettings,
        explicit: bool,
    ) -> Result<(), ProtocolErrorBody> {
        if !self.participants.contains(session) && self.participants.len() >= PARTICIPANTS {
            return Err(refusal(
                "orihon_participants_full",
                format!("{PARTICIPANTS} fold sessions already take part in this room"),
            ));
        }
        let owner = self.opened().await?;
        if (explicit || owner.config.folder.is_none())
            && owner.config.folder.as_ref() != Some(folder)
        {
            let config = LiveConfig {
                folder: Some(folder.clone()),
                prefix_digest: owner.config.prefix_digest.clone(),
            };
            owner
                .live
                .save_config(&config)
                .await
                .map_err(storage_error)?;
            owner.config = config;
        }
        if !self.participants.contains(session) {
            self.participants.insert(session.to_owned());
        }
        Ok(())
    }

    /// Participants whose conversation the Host no longer registers have left.
    async fn reconcile(&mut self, registered: &[String]) {
        if self.participants.is_empty() {
            return;
        }
        self.participants
            .retain(|session| registered.contains(session));
        if self.participants.is_empty() {
            self.pause().await;
        }
    }

    async fn begin_turn(&mut self, request: TurnRequest, reply: Reply) {
        let hints = self.room.take_hints();
        // A verified live turn is consent: its session takes part from here on.
        if let Err(error) = self.join(&request.session, &request.folder, false).await {
            let _ = reply.send(Err(error));
            return;
        }
        let owner = self.owner.as_mut().expect("a participant's owner is open");
        match owner.begin_turn(&request, hints).await {
            Ok(accepted) => owner.waiters.push(Waiter {
                end: accepted.end,
                through_seq: accepted.through_seq,
                started: request.started,
                deadline: request.deadline,
                reply,
                prefix_digest: accepted.prefix_digest,
                prefix_changed: accepted.prefix_changed,
                model: request.model,
            }),
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        }
    }

    /// A task child's view of what is built. A child is not consent: this starts no work.
    async fn subagent(&mut self) -> Result<Value, ProtocolErrorBody> {
        let configured = match &self.owner {
            Some(owner) => owner.config.folder.is_some(),
            None => store::read_config(&self.store, &self.name)
                .await
                .map_err(storage_error)?
                .folder
                .is_some(),
        };
        if !configured {
            return Err(not_live("this room has no live compactor configuration"));
        }
        let owner = self.opened().await?;
        let keys = owner.chat.built_prefix(u64::MAX, &owner.sizes);
        let complete = keys.len() == owner.chat.lines.len();
        let end = keys.last().map_or(0, |key| key.end());
        let rendered = owner.render(&keys).await.map_err(storage_error)?;
        let through_seq = owner
            .live
            .seq_through_units(end)
            .await
            .map_err(storage_error)?;
        Ok(
            json!({"ok":true,"authority":"provenance","purpose":"subagent","ready":true,
            "chat":chat_json(owner.chat.revision, end, through_seq, rendered),
            "complete":complete,"prefixDigest":owner.config.prefix_digest,"prefixChanged":false,
            "instructions":LIVE_INSTRUCTIONS,"instructionsVersion":LIVE_VERSION}),
        )
    }

    /// A message boundary: ingest, retry what failed, maintain and save the views. Records
    /// of a room without a participant wait for the next consent.
    async fn boundary(&mut self) {
        let hints = self.room.take_hints();
        if self.participants.is_empty() {
            return;
        }
        if let Err(opening) = self.open().await {
            self.note_opening(opening);
            return;
        }
        let owner = self.owner.as_mut().expect("the owner is open");
        if let Err(error) = owner.boundary(hints).await {
            self.error = Some(error.to_string());
        }
    }

    async fn finish(&mut self, joined: Result<Finished, JoinError>) {
        let finished = match joined {
            Ok(finished) => finished,
            Err(error) => {
                self.error = Some(format!(
                    "a compaction task ended without a receipt: {error}"
                ));
                if self.jobs.is_empty()
                    && let Some(owner) = self.owner.as_mut()
                {
                    let lost: Vec<NodeKey> = owner.running.drain().collect();
                    owner.failed.extend(lost);
                }
                return;
            }
        };
        let Some(owner) = self.owner.as_mut() else {
            return;
        };
        if let Err(message) = owner.finish(finished).await {
            self.error = Some(message);
        }
    }

    async fn expire(&mut self) {
        let Some(owner) = self.owner.as_mut() else {
            return;
        };
        owner.answer().await;
        owner.expire(Instant::now(), self.error.as_deref());
    }

    async fn retry(&mut self) {
        if self.participants.is_empty() || self.shutdown.is_cancelled() {
            return;
        }

        let Some(owner) = self.owner.as_mut() else {
            return;
        };

        if let Err(error) = owner.retry().await {
            self.error = Some(error.to_string());
        }
    }

    /// After every event: keep the engine current, start ready work, answer ready turns.
    /// Without a participant no engine opens and no model job starts.
    async fn advance(&mut self) {
        if self.shutdown.is_cancelled() {
            return;
        }

        let Some(owner) = self.owner.as_mut() else {
            return;
        };

        if owner.stalled {
            owner.schedule_retry(!self.participants.is_empty());
            return;
        }

        if !self.participants.is_empty() {
            if let Err(message) = owner.ensure_engine().await {
                self.error = Some(message);
            }

            if self.shutdown.is_cancelled() {
                return;
            }

            if let Err(error) = owner.dispatch(&mut self.jobs, &self.cancel).await {
                self.error = Some(error.to_string());
                owner.stalled = true;
            }
        }

        owner.answer().await;
        owner.schedule_retry(!self.participants.is_empty());
    }

    /// Fail-stop when the lock connection fails: the lock may be gone, so the owner cancels and
    /// joins its compactions, closes its engine and lets go. Only a fresh `Live::open` at a
    /// later boundary or turn takes the room again.
    async fn check_owner(&mut self) {
        let Some(owner) = self.owner.as_mut() else {
            return;
        };
        if let Err(error) = owner.live.check_owner().await {
            self.stop().await;
            self.error = Some(format!(
                "the room lock connection failed, so this owner stopped: {error}"
            ));
        }
    }

    async fn close(&mut self) {
        self.stop().await;
    }

    async fn stop(&mut self) {
        self.pause().await;
        if let Some(owner) = self.owner.as_mut() {
            for waiter in owner.waiters.drain(..) {
                let _ = waiter.reply.send(Err(closed()));
            }
        }
        self.owner = None;
    }

    /// Cancels and joins every compaction, storing what each returned, and closes the engine.
    /// Queues, views and configuration stay, so a later consent resumes the work.
    async fn pause(&mut self) {
        self.cancel.cancel();
        while let Some(joined) = self.jobs.join_next().await {
            self.finish(joined).await;
        }
        self.cancel = self.shutdown.child_token();
        if let Some(owner) = self.owner.as_mut() {
            owner.retry_at = None;
        }
        if let Some(owner) = self.owner.as_mut()
            && let Some(slot) = owner.engine.take()
            && let Err(error) = slot.engine.close().await
        {
            tracing::warn!(room = self.name, error = %error, "Orihon compaction engine did not close cleanly");
        }
    }
}

struct Slot {
    engine: Engine,
    folder: FolderSettings,
    prefix_digest: String,
}

struct Waiter {
    end: u64,
    through_seq: Option<i64>,
    started: Instant,
    deadline: Instant,
    reply: Reply,
    prefix_digest: String,
    prefix_changed: bool,
    model: TurnModel,
}

struct Accepted {
    end: u64,
    through_seq: Option<i64>,
    prefix_digest: String,
    prefix_changed: bool,
}

/// A paid result whose storage write failed: retried without another model call.
enum Unsaved {
    Node {
        key: NodeKey,
        text: String,
        source_digest: String,
        job: Job,
    },
    Job(Job),
}

struct Owner {
    live: Live,
    config: LiveConfig,
    units: u64,
    ingested_seq: i64,
    /// Byte sizes of built nodes.
    sizes: HashMap<NodeKey, usize>,
    chat: View,
    compactor: View,
    /// Views changed but not yet saved; nothing is served from them until they are.
    dirty: bool,
    /// Unbuilt leaves. The first `LEAF_WINDOW` of them may start.
    pending: BTreeSet<u64>,
    /// Ready parents whose halves fit one line together.
    joins: Vec<NodeKey>,
    /// Ready parents for the model, oldest end first.
    merges: BTreeSet<(u64, NodeKey)>,
    running: HashSet<NodeKey>,
    /// Failed nodes wait for a boundary or the idle retry deadline.
    failed: HashSet<NodeKey>,
    unsaved: Vec<Unsaved>,
    engine: Option<Slot>,
    /// The engine did not open; a boundary or idle retry unblocks it.
    engine_blocked: bool,
    /// Storage failed during dispatch; a boundary or idle retry resumes it.
    stalled: bool,
    retry_at: Option<Instant>,
    note: Option<&'static str>,
    waiters: Vec<Waiter>,
}

impl Owner {
    /// Restores exactly what was persisted. Views are never rebuilt; ready parents are found
    /// once here from the built nodes.
    fn restore(live: Live, snapshot: Snapshot) -> Result<Self, String> {
        let chat = snapshot.chat.unwrap_or_default();
        let compactor = snapshot.compactor.unwrap_or_default();
        if chat.units > snapshot.units || compactor.units > snapshot.units {
            return Err("a persisted Orihon view covers units that are not stored".into());
        }
        let sizes = snapshot.nodes;
        let pending = (0..snapshot.units)
            .filter(|unit| !sizes.contains_key(&NodeKey::leaf(*unit)))
            .collect();
        let mut owner = Self {
            live,
            config: snapshot.config,
            units: snapshot.units,
            ingested_seq: snapshot.ingested_seq,
            sizes,
            chat,
            compactor,
            dirty: false,
            pending,
            joins: Vec::new(),
            merges: BTreeSet::new(),
            running: HashSet::new(),
            failed: HashSet::new(),
            unsaved: Vec::new(),
            engine: None,
            engine_blocked: false,
            stalled: false,
            retry_at: None,
            note: None,
            waiters: Vec::new(),
        };
        let lefts: Vec<NodeKey> = owner
            .sizes
            .keys()
            .filter(|key| key.index % 2 == 0)
            .copied()
            .collect();
        for key in lefts {
            owner.ready_parent(key);
        }
        Ok(owner)
    }

    fn status(&self, last_error: Option<&str>) -> Value {
        let leaves = |set: &HashSet<NodeKey>| set.iter().filter(|key| key.level == 0).count();
        let held = self
            .unsaved
            .iter()
            .filter(|unsaved| matches!(unsaved, Unsaved::Node { key, .. } if key.level == 0))
            .count();
        let waiting = self
            .pending
            .len()
            .saturating_sub(leaves(&self.running) + leaves(&self.failed) + held);
        json!({"units":self.units,"leavesBuilt":self.units - self.pending.len() as u64,
            "running":self.running.len(),"failed":self.failed.len(),
            "queued":waiting + self.joins.len() + self.merges.len(),
            "chatBytes":self.chat.bytes(&self.sizes),"chatLines":self.chat.lines.len(),
            "batch":if self.chat.merging { "merging" } else { "growing" },
            "compactorBytes":self.compactor.bytes(&self.sizes),
            "lastError":last_error,"prefixDigest":self.config.prefix_digest,
            "folder":self.config.folder,"cacheGate":self.cache_gate()})
    }

    /// The engine's cache-write coordination, stated as the engine states it.
    fn cache_gate(&self) -> Option<&'static str> {
        self.engine
            .as_ref()
            .and_then(|slot| slot.engine.cache_gate())
    }

    fn schedule_retry(&mut self, participating: bool) {
        let needed = self.engine_blocked
            || self.stalled
            || !self.failed.is_empty()
            || !self.unsaved.is_empty();

        if !participating || !needed {
            self.retry_at = None;
        } else if self.retry_at.is_none() {
            self.retry_at = Some(Instant::now() + RETRY_DELAY);
        }
    }

    async fn retry(&mut self) -> Result<(), Error> {
        self.retry_at = None;
        self.engine_blocked = false;
        self.stalled = false;
        self.save_unsaved().await?;

        for key in std::mem::take(&mut self.failed) {
            if key.level > 0 {
                self.merges.insert((key.end(), key));
            }
        }

        Ok(())
    }

    /// One message boundary: stored results first, then failed work back in the queues, new
    /// records into units, and the views' append or batch step.
    async fn boundary(&mut self, mut hints: Vec<Record>) -> Result<(), Error> {
        self.retry().await?;

        loop {
            let take = hints.len().min(MAX_BATCH_RECORDS);
            let batch: Vec<Record> = hints.drain(..take).collect();
            let sources = self
                .live
                .ingest(self.ingested_seq, self.units, batch)
                .await?;
            let Some((seq, unit)) = sources.last().map(|last| (last.unit.seq, last.unit.unit))
            else {
                break;
            };
            self.ingested_seq = seq;
            self.units = unit + 1;
            for source in sources {
                let key = NodeKey::leaf(source.unit.unit);
                self.pending.insert(key.index);
                let input = tree::leaf_input(source.kind, &source.unit, &source.text);
                if tree::verbatim(&input).is_some() {
                    self.put(key, input, Method::Verbatim, source.unit.digest, None)
                        .await?;
                }
            }
        }
        self.maintain_views().await
    }

    /// Apply built nodes to both views and persist them before more work reads their context.
    async fn maintain_views(&mut self) -> Result<(), Error> {
        if tree::maintain(&mut self.chat, &mut self.compactor, self.units, &self.sizes).changed() {
            self.dirty = true;
        }
        if self.dirty {
            self.live.save_views(&self.chat, &self.compactor).await?;
            self.dirty = false;
        }
        Ok(())
    }

    async fn save_unsaved(&mut self) -> Result<(), Error> {
        let mut pending = std::mem::take(&mut self.unsaved).into_iter();
        while let Some(unsaved) = pending.next() {
            let result = match &unsaved {
                Unsaved::Node {
                    key,
                    text,
                    source_digest,
                    job,
                } => self
                    .live
                    .put_node(
                        *key,
                        text.clone(),
                        Method::Model,
                        source_digest.clone(),
                        Some(job),
                    )
                    .await
                    .map(|node| Some((node.key, node.text.len()))),
                Unsaved::Job(job) => self.live.put_job(job).await.map(|_| None),
            };
            match result {
                Ok(Some((key, bytes))) => self.built(key, bytes),
                Ok(None) => {}
                Err(error) => {
                    self.unsaved.push(unsaved);
                    self.unsaved.extend(pending);
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Validates the turn boundary, stores the prefix, runs the boundary, and places the
    /// frontier. The view is answered once every line through the frontier is built.
    async fn begin_turn(
        &mut self,
        request: &TurnRequest,
        hints: Vec<Record>,
    ) -> Result<Accepted, ProtocolErrorBody> {
        if self.config.folder.is_none() {
            return Err(refusal(
                "orihon_folder_unconfigured",
                "this room has no live compactor configuration",
            ));
        }
        let prefix_digest = self
            .live
            .put_prefix(&request.prefix)
            .await
            .map_err(storage_error)?;
        let prefix_changed = self.config.prefix_digest.as_deref() != Some(prefix_digest.as_str());
        if prefix_changed {
            let config = LiveConfig {
                folder: self.config.folder.clone(),
                prefix_digest: Some(prefix_digest.clone()),
            };
            self.live
                .save_config(&config)
                .await
                .map_err(storage_error)?;
            self.config = config;
        }
        self.boundary(hints).await.map_err(storage_error)?;
        let frontier = self
            .frontier(&request.session, request.through_source_id.as_deref())
            .await?;
        if frontier > self.ingested_seq {
            // Another session's capture landed after this boundary's ingest.
            self.boundary(Vec::new()).await.map_err(storage_error)?;
        }
        if frontier > self.ingested_seq {
            return Err(refusal(
                "orihon_boundary_missing",
                "records before this turn are not in the live tree yet",
            ));
        }
        let end = if frontier < 0 {
            0
        } else {
            self.live
                .units_through(frontier)
                .await
                .map_err(storage_error)?
        };
        Ok(Accepted {
            end,
            through_seq: (frontier >= 0).then_some(frontier),
            prefix_digest,
            prefix_changed,
        })
    }

    /// The last record of the room that a turn's view covers. Records of this session after
    /// it may only be one new user input and the records it parents.
    async fn frontier(
        &mut self,
        session: &str,
        through: Option<&str>,
    ) -> Result<i64, ProtocolErrorBody> {
        let frontier = match through {
            Some(source_id) => self
                .live
                .source_seq(HARNESS, session, source_id)
                .await
                .map_err(storage_error)?
                .ok_or_else(|| {
                    refusal(
                        "orihon_boundary_missing",
                        "throughSourceId is not an accepted record of this session",
                    )
                })?,
            None => match self
                .live
                .session_first(HARNESS, session)
                .await
                .map_err(storage_error)?
            {
                Some(first) => first - 1,
                None => self
                    .live
                    .last_seq()
                    .await
                    .map_err(storage_error)?
                    .unwrap_or(-1),
            },
        };
        let after = self
            .live
            .session_after(HARNESS, session, frontier, SESSION_TAIL + 1)
            .await
            .map_err(storage_error)?;
        if after.len() > SESSION_TAIL || !one_user_input(&after) {
            return Err(refusal(
                "orihon_boundary_stale",
                "this session holds records after the frontier beyond one new user input",
            ));
        }
        Ok(frontier)
    }

    /// Answers every waiting turn whose frontier cut is wholly built.
    async fn answer(&mut self) {
        if self.dirty {
            return;
        }
        let mut index = 0;
        while index < self.waiters.len() {
            let Ok(keys) = self.chat.cut(self.waiters[index].end, &self.sizes) else {
                index += 1;
                continue;
            };
            let waiter = self.waiters.swap_remove(index);
            let result = match self.render(&keys).await {
                Ok(rendered) => Ok(json!({"ok":true,"authority":"provenance","purpose":"turn",
                    "ready":true,
                    "chat":chat_json(self.chat.revision, waiter.end, waiter.through_seq, rendered),
                    "complete":true,"prefixDigest":waiter.prefix_digest,
                    "prefixChanged":waiter.prefix_changed,"instructions":LIVE_INSTRUCTIONS,
                    "instructionsVersion":LIVE_VERSION,
                    "cache":cache_report(self.config.folder.as_ref(), &waiter.model, self.cache_gate())})),
                Err(error) => Err(storage_error(error)),
            };
            let _ = waiter.reply.send(result);
        }
    }

    fn expire(&mut self, now: Instant, last_error: Option<&str>) {
        let mut index = 0;
        while index < self.waiters.len() {
            if self.waiters[index].deadline > now {
                index += 1;
                continue;
            }
            let waiter = self.waiters.swap_remove(index);
            let unbuilt = self.pending.range(..waiter.end).count();
            let failed = self
                .failed
                .iter()
                .filter(|key| key.level == 0 && key.index < waiter.end)
                .count();
            let waited = now.saturating_duration_since(waiter.started).as_millis();
            let _ = waiter.reply.send(Err(ProtocolErrorBody::application(
                "orihon_view_not_ready",
                "the chat view is not built through this turn's frontier",
            )
            .retryable(true)
            .details(json!({"units":waiter.end,"unbuilt":unbuilt,"failed":failed,
                "running":self.running.len(),"lastError":last_error.or(self.note),
                "waitedMs":u64::try_from(waited).unwrap_or(u64::MAX)}))
            .build()));
        }
    }

    /// Keeps one engine for the room's current folder and prefix. A change waits for the
    /// running compactions of the old engine before closing it.
    async fn ensure_engine(&mut self) -> Result<(), String> {
        let wanted = self
            .config
            .folder
            .clone()
            .zip(self.config.prefix_digest.clone());
        if let (Some(slot), Some((folder, prefix_digest))) = (&self.engine, &wanted)
            && slot.folder == *folder
            && slot.prefix_digest == *prefix_digest
        {
            return Ok(());
        }
        if self.engine.is_some() {
            if !self.running.is_empty() {
                return Ok(());
            }
            if let Some(slot) = self.engine.take() {
                slot.engine.close().await.map_err(|error| {
                    format!("the previous compaction engine did not close: {error}")
                })?;
            }
        }
        let Some((folder, prefix_digest)) = wanted else {
            self.note = Some(WAITING_FOR_PREFIX);
            return Ok(());
        };
        if self.engine_blocked {
            return Ok(());
        }
        self.engine_blocked = true;
        let prefix = self
            .live
            .prefix(&prefix_digest)
            .await
            .map_err(|error| error.to_string())?
            .ok_or("the room's compactor prefix is not stored")?;
        let engine = Engine::open(folder_config(&folder)?, prefix)
            .await
            .map_err(|error| format!("the compaction engine did not open: {error}"))?;
        self.engine_blocked = false;
        self.note = None;
        self.engine = Some(Slot {
            engine,
            folder,
            prefix_digest,
        });
        Ok(())
    }

    /// The engine for new work, unless it is being replaced.
    fn current_engine(&self) -> Option<Engine> {
        let slot = self.engine.as_ref()?;
        (self.config.folder.as_ref() == Some(&slot.folder)
            && self.config.prefix_digest.as_deref() == Some(slot.prefix_digest.as_str()))
        .then(|| slot.engine.clone())
    }

    fn held(&self, key: NodeKey) -> bool {
        self.unsaved
            .iter()
            .any(|unsaved| matches!(unsaved, Unsaved::Node { key: held, .. } if *held == key))
    }

    fn idle(&self, key: NodeKey) -> bool {
        !self.running.contains(&key) && !self.failed.contains(&key) && !self.held(key)
    }

    /// Prioritize merges; keep one leaf slot when its context fits the compactor budget.
    async fn dispatch(
        &mut self,
        jobs: &mut JoinSet<Finished>,
        cancel: &CancellationToken,
    ) -> Result<(), Error> {
        self.drain_joins().await?;
        self.maintain_views().await?;
        let Some(engine) = self.current_engine() else {
            return Ok(());
        };
        while self.running.len() < MAX_JOBS && !cancel.is_cancelled() {
            let next = self
                .pending
                .iter()
                .take(LEAF_WINDOW)
                .copied()
                .find(|unit| self.idle(NodeKey::leaf(*unit)));
            // One bounded leaf can progress even when new merges keep arriving.
            let leaf_slot = self.running.len() == MAX_JOBS - 1
                && !self.running.iter().any(|key| key.level == 0)
                && next.is_some_and(|unit| self.context_fits(unit));

            if !leaf_slot && let Some(&(end, key)) = self.merges.first() {
                self.merges.remove(&(end, key));
                if let Err(error) = self.start_merge(jobs, &engine, cancel, key).await {
                    self.failed.insert(key);
                    return Err(error);
                }
                continue;
            }

            let Some(unit) = next else {
                break;
            };
            let key = NodeKey::leaf(unit);
            let source = self.live.leaf_source(unit).await?;
            let input = tree::leaf_input(source.kind, &source.unit, &source.text);
            if tree::verbatim(&input).is_some() {
                self.put(key, input, Method::Verbatim, source.unit.digest, None)
                    .await?;
                self.drain_joins().await?;
                self.maintain_views().await?;
                continue;
            }
            let context = self.context(unit).await?;
            let prompt = compress_task(unit, &input);
            self.spawn(
                jobs,
                &engine,
                cancel,
                key,
                source.unit.digest,
                context,
                prompt,
            );
        }
        Ok(())
    }

    async fn start_merge(
        &mut self,
        jobs: &mut JoinSet<Finished>,
        engine: &Engine,
        cancel: &CancellationToken,
        key: NodeKey,
    ) -> Result<(), Error> {
        let (left, right, left_text, right_text, source_digest) = self.halves(key).await?;
        let prompt = merge_task(
            &label(left),
            &label(right),
            key.first(),
            key.last(),
            &input_line(left, &left_text),
            &input_line(right, &right_text),
        );
        let context = self.context(key.end()).await?;
        self.spawn(jobs, engine, cancel, key, source_digest, context, prompt);
        Ok(())
    }

    /// Both built halves of a parent with their texts and the parent's source digest.
    async fn halves(
        &mut self,
        key: NodeKey,
    ) -> Result<(NodeKey, NodeKey, String, String, String), Error> {
        let [left, right] = key
            .children()
            .ok_or_else(|| Error::Invalid("a leaf has no halves".into()))?;
        let mut texts = self.live.texts(&[left, right]).await?;
        let (Some(left_text), Some(right_text)) = (texts.remove(&left), texts.remove(&right))
        else {
            return Err(Error::Invalid(
                "a ready parent's half has no stored text".into(),
            ));
        };
        let (Some(left_digest), Some(right_digest)) = (
            self.live.node_digest(left).await?,
            self.live.node_digest(right).await?,
        ) else {
            return Err(Error::Invalid(
                "a ready parent's half has no stored digest".into(),
            ));
        };
        Ok((
            left,
            right,
            left_text,
            right_text,
            tree::branch_source_digest(&left_digest, &right_digest),
        ))
    }

    async fn drain_joins(&mut self) -> Result<(), Error> {
        while let Some(key) = self.joins.pop() {
            if let Err(error) = self.join(key).await {
                self.joins.push(key);
                return Err(error);
            }
        }
        Ok(())
    }

    async fn join(&mut self, key: NodeKey) -> Result<(), Error> {
        let (_, _, left_text, right_text, source_digest) = self.halves(key).await?;
        match tree::joined(&left_text, &right_text) {
            Some(text) => {
                self.put(key, text, Method::Joined, source_digest, None)
                    .await
            }
            None => {
                self.merges.insert((key.end(), key));
                Ok(())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &mut self,
        jobs: &mut JoinSet<Finished>,
        engine: &Engine,
        cancel: &CancellationToken,
        key: NodeKey,
        source_digest: String,
        context: Rendered,
        prompt: String,
    ) {
        let engine = engine.clone();
        let cancellation = cancel.child_token();
        jobs.spawn(async move {
            let completion = AssertUnwindSafe(engine.summarize(
                Task {
                    key,
                    context,
                    prompt,
                },
                cancellation,
            ))
            .catch_unwind()
            .await
            .ok();
            Finished {
                key,
                source_digest,
                completion,
            }
        });
        self.running.insert(key);
    }

    /// Stores what one compaction returned: its node with its job when a line was accepted,
    /// otherwise its job, and the node waits for a boundary or idle retry.
    async fn finish(&mut self, finished: Finished) -> Result<(), String> {
        let Finished {
            key,
            source_digest,
            completion,
        } = finished;
        self.running.remove(&key);
        let Some(Completion { text, job }) = completion else {
            self.failed.insert(key);
            return Err("the compaction engine panicked without a job receipt".into());
        };
        let line = text.filter(|text| {
            job.outcome == Outcome::Accepted
                && !text.trim().is_empty()
                && text.len() <= MODEL_LINE_CEILING
        });
        if let Some(text) = line {
            let bytes = text.len();
            return match self
                .live
                .put_node(
                    key,
                    text.clone(),
                    Method::Model,
                    source_digest.clone(),
                    Some(&job),
                )
                .await
            {
                Ok(_) => {
                    self.built(key, bytes);
                    Ok(())
                }
                Err(error) => {
                    self.unsaved.push(Unsaved::Node {
                        key,
                        text,
                        source_digest,
                        job,
                    });
                    Err(format!("an accepted line waits for storage: {error}"))
                }
            };
        }
        let reason = if job.outcome == Outcome::Accepted {
            "the compaction engine accepted an empty or oversized line".to_owned()
        } else {
            job.error
                .clone()
                .unwrap_or_else(|| format!("a compaction ended {}", job.outcome.name()))
        };
        self.failed.insert(key);
        match self.live.put_job(&job).await {
            Ok(_) => Err(reason),
            Err(error) => {
                self.unsaved.push(Unsaved::Job(job));
                Err(format!(
                    "{reason}; its job receipt waits for storage: {error}"
                ))
            }
        }
    }

    async fn put(
        &mut self,
        key: NodeKey,
        text: String,
        method: Method,
        source_digest: String,
        job: Option<&Job>,
    ) -> Result<(), Error> {
        let bytes = text.len();
        self.live
            .put_node(key, text, method, source_digest, job)
            .await?;
        self.built(key, bytes);
        Ok(())
    }

    fn built(&mut self, key: NodeKey, bytes: usize) {
        self.sizes.insert(key, bytes);
        self.failed.remove(&key);
        if key.level == 0 {
            self.pending.remove(&key.index);
        }
        self.ready_parent(key);
    }

    /// A node's parent is ready once both halves are built. Halves that fit one line
    /// together join without a model.
    fn ready_parent(&mut self, key: NodeKey) {
        let Some(parent) = key.parent() else {
            return;
        };
        let Some([left, right]) = parent.children() else {
            return;
        };
        if self.sizes.contains_key(&parent) {
            return;
        }
        let (Some(left), Some(right)) = (self.sizes.get(&left), self.sizes.get(&right)) else {
            return;
        };
        if left + 1 + right <= LINE_BYTES {
            self.joins.push(parent);
        } else {
            self.merges.insert((parent.end(), parent));
        }
    }

    fn context_fits(&self, end: u64) -> bool {
        let mut bytes = 0;
        for key in self
            .compactor
            .lines
            .iter()
            .take_while(|key| key.end() <= end && self.sizes.contains_key(key))
        {
            bytes += tree::line_bytes(*key, self.sizes.get(key).copied());
            if bytes > tree::COMPACTOR_HIGH {
                return false;
            }
        }
        true
    }

    /// The compactor view's built lines that end at or before `end`, rendered.
    async fn context(&mut self, end: u64) -> Result<Rendered, Error> {
        let keys = self.compactor.built_prefix(end, &self.sizes);
        self.render(&keys).await
    }

    async fn render(&mut self, keys: &[NodeKey]) -> Result<Rendered, Error> {
        let texts = self.live.texts(keys).await?;
        let lines = keys
            .iter()
            .map(|key| {
                texts
                    .get(key)
                    .map(|text| (*key, text.as_str()))
                    .ok_or_else(|| Error::Invalid("a built view line has no stored text".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tree::render(&lines))
    }
}

fn chat_json(revision: u64, units: u64, through_seq: Option<i64>, rendered: Rendered) -> Value {
    json!({"revision":revision,"units":units,"throughSeq":through_seq,"bytes":rendered.bytes,
        "lines":rendered.lines,"blocks":rendered.blocks,"tail":rendered.tail})
}

fn label(key: NodeKey) -> String {
    format!("{}+{}", key.first(), key.count())
}

/// A merge input line as the view shows it: `id+n|text`.
fn input_line(key: NodeKey, text: &str) -> String {
    let mut line = tree::render_line(key, Some(text));
    // Every rendered line ends with its newline.
    line.pop();
    line
}

/// After the frontier a session may hold one user input, its parts consecutive and before
/// any other record of the session, and only records parented by its first part.
fn one_user_input(records: &[SessionRecord]) -> bool {
    let users = records
        .iter()
        .take_while(|record| record.kind == Kind::User)
        .count();
    let (inputs, rest) = records.split_at(users);
    let Some(first) = inputs.first() else {
        return rest.is_empty();
    };
    if inputs.len() > 1 || part_of(&first.source_id).is_some() {
        let Some((base, _, count)) = part_of(&first.source_id) else {
            return false;
        };
        let numbered = inputs
            .iter()
            .enumerate()
            .all(|(index, record)| part_of(&record.source_id) == Some((base, index + 1, count)));
        if !numbered {
            return false;
        }
    }
    rest.iter()
        .all(|record| record.parent_user_id.as_deref() == Some(first.source_id.as_str()))
}

/// `<entry>#part-<k>-of-<n>`, with 1 <= k <= n.
fn part_of(source_id: &str) -> Option<(&str, usize, usize)> {
    let (base, suffix) = source_id.rsplit_once("#part-")?;
    let (part, count) = suffix.split_once("-of-")?;
    let part: usize = part.parse().ok()?;
    let count: usize = count.parse().ok()?;
    (part >= 1 && part <= count).then_some((base, part, count))
}

fn folder_config(folder: &FolderSettings) -> Result<FolderConfig, String> {
    Ok(match folder.backend {
        FolderBackend::Local => FolderConfig::local(folder.model.clone()),
        FolderBackend::Hosted => FolderConfig::hosted(
            folder.model.clone(),
            folder
                .runtime
                .clone()
                .ok_or("the hosted compactor configuration has no OMP runtime binding")?,
        ),
    })
}

/// Which models the turn and the compactor use. Caches are per model and per account; this
/// compares model identity only.
fn cache_report(folder: Option<&FolderSettings>, model: &TurnModel, gate: Option<&str>) -> Value {
    let compactor = folder.map(|folder| {
        let id = match folder.backend {
            FolderBackend::Local => folder.model.as_str(),
            FolderBackend::Hosted => folder
                .model
                .split_once('/')
                .map_or(folder.model.as_str(), |(_, id)| id),
        };
        (folder, id)
    });
    let same_model =
        compactor.is_some_and(|(folder, id)| folder.provider() == model.provider && id == model.id);
    json!({"turnModel":format!("{}/{}", model.provider, model.id),
        "compactor":compactor.map(|(folder, id)| json!({"backend":folder.backend,
            "provider":folder.provider(),"model":id})),
        "sameModel":same_model,
        "note":"Prompt caches are kept per model and per account. sameModel compares model identity only; it never shows that turns and compactions share an account or a cache entry.",
        "compactorGate":gate})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seq: i64, source_id: &str, kind: Kind, parent: Option<&str>) -> SessionRecord {
        SessionRecord {
            seq,
            source_id: source_id.into(),
            kind,
            parent_user_id: parent.map(str::to_owned),
        }
    }

    #[test]
    fn a_turn_boundary_holds_one_user_input_and_its_records() {
        assert!(one_user_input(&[]));
        assert!(one_user_input(&[
            record(5, "e:user", Kind::User, None),
            record(6, "e:tool", Kind::Tool, Some("e:user")),
        ]));
        assert!(one_user_input(&[
            record(5, "e:user#part-1-of-2", Kind::User, None),
            record(6, "e:user#part-2-of-2", Kind::User, None),
            record(7, "e:talk", Kind::Talk, Some("e:user#part-1-of-2")),
        ]));
        // A record parented elsewhere, a second input, or parts out of order are stale.
        assert!(!one_user_input(&[record(
            5,
            "x:talk",
            Kind::Talk,
            Some("old:user")
        )]));
        assert!(!one_user_input(&[
            record(5, "a:user", Kind::User, None),
            record(6, "a:talk", Kind::Talk, Some("a:user")),
            record(7, "b:user", Kind::User, None),
        ]));
        assert!(!one_user_input(&[
            record(5, "a:user", Kind::User, None),
            record(6, "b:user", Kind::User, None),
        ]));
        assert!(!one_user_input(&[record(
            5,
            "e:user#part-2-of-2",
            Kind::User,
            None
        ),]));
    }

    #[test]
    fn the_cache_report_compares_model_identity_only() {
        let folder = FolderSettings {
            model: "anthropic/claude-haiku-4-5".into(),
            backend: FolderBackend::Hosted,
            runtime: None,
        };
        let same = cache_report(
            Some(&folder),
            &TurnModel {
                provider: "anthropic".into(),
                id: "claude-haiku-4-5".into(),
            },
            None,
        );
        assert_eq!(same["sameModel"], true);
        assert_eq!(same["compactor"]["model"], "claude-haiku-4-5");
        let other = cache_report(
            Some(&folder),
            &TurnModel {
                provider: "anthropic".into(),
                id: "claude-opus-4-5".into(),
            },
            None,
        );
        assert_eq!(other["sameModel"], false);
        let none = TurnModel {
            provider: "p".into(),
            id: "m".into(),
        };
        assert_eq!(cache_report(None, &none, None)["compactor"], Value::Null);
    }
}
#[cfg(test)]
#[path = "live_regression_tests.rs"]
mod regression_tests;
