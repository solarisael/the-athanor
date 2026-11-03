use super::*;
use origami::orihon::live::COMPACTOR_HIGH;
use origami::orihon::live::store::Usage;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

struct TestBook(PathBuf);

impl TestBook {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("orihon-live-regression-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create test book");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn append(&self, count: usize, bytes: usize) {
        for start in (0..count).step_by(MAX_BATCH_RECORDS) {
            let records = (start..count.min(start + MAX_BATCH_RECORDS))
                .map(|index| hearth::orihon::CaptureRecord {
                    source_id: format!("user-{index}"),
                    parent_user_id: None,
                    kind: Kind::User,
                    speaker: "Operator".into(),
                    text: format!("source-{index}-{}", "x".repeat(bytes)),
                    said_at: "2030-01-01T00:00:00Z".into(),
                    timestamp_observed: false,
                    redacted: false,
                    fold_text: None,
                    children: None,
                })
                .collect();
            origami::orihon::append_file(
                &self.0,
                "room",
                "writer",
                hearth::orihon::CaptureBatch {
                    harness: "test".into(),
                    source_session: "session".into(),
                    records,
                },
            )
            .expect("append test records");
        }
    }
}

impl Drop for TestBook {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned test book");
    }
}

async fn open_owner(book: &TestBook) -> Owner {
    let (live, snapshot) = Live::open(Store::File(book.path().to_path_buf()), "room")
        .await
        .expect("open test room")
        .expect("test room lock");
    Owner::restore(live, snapshot).expect("restore test owner")
}

fn folder() -> FolderSettings {
    FolderSettings {
        model: "test-model".into(),
        backend: FolderBackend::Local,
        runtime: None,
    }
}

async fn model_server() -> (String, Arc<AtomicUsize>, CancellationToken, JoinHandle<()>) {
    controlled_model_server(Arc::new(AtomicBool::new(false))).await
}

async fn controlled_model_server(
    failing: Arc<AtomicBool>,
) -> (String, Arc<AtomicUsize>, CancellationToken, JoinHandle<()>) {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind controlled model");
    let endpoint = format!("http://{}", listener.local_addr().expect("model address"));
    let requests = Arc::new(AtomicUsize::new(0));
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task_requests = requests.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = task_cancel.cancelled() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    let requests = task_requests.clone();
                    let failing = failing.clone();
                    tokio::spawn(async move { answer_model(stream, requests, failing).await });
                }
            }
        }
    });
    (endpoint, requests, cancel, task)
}

async fn answer_model(mut stream: TcpStream, requests: Arc<AtomicUsize>, failing: Arc<AtomicBool>) {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let body_start;
    let content_length;
    loop {
        let count = stream.read(&mut chunk).await.expect("read model request");
        if count == 0 {
            return;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            body_start = end + 4;
            let headers = String::from_utf8_lossy(&bytes[..end]);
            content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .expect("model content length");
            break;
        }
    }
    while bytes.len() < body_start + content_length {
        let count = stream.read(&mut chunk).await.expect("read model body");
        if count == 0 {
            return;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    requests.fetch_add(1, Ordering::Relaxed);
    if failing.load(Ordering::SeqCst) {
        stream
            .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .expect("write controlled model failure");
        return;
    }
    let response = serde_json::json!({
        "model": "test-model",
        "done": true,
        "done_reason": "stop",
        "message": {"role": "assistant", "content": "s".repeat(480)},
        "prompt_eval_count": 1,
        "eval_count": 1
    });
    let body = serde_json::to_vec(&response).expect("serialize model response");
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .await
        .expect("write model headers");
    stream.write_all(&body).await.expect("write model response");
}

async fn close_model(cancel: CancellationToken, task: JoinHandle<()>) {
    cancel.cancel();
    task.await.expect("stop controlled model");
}

fn accepted_job(key: NodeKey) -> Job {
    Job {
        id: None,
        key,
        outcome: Outcome::Accepted,
        backend: "local".into(),
        provider: "ollama".into(),
        model: "test-model".into(),
        cache_scope: "test-scope".into(),
        prefix_digest: "a".repeat(64),
        context_digest: "b".repeat(64),
        context_lines: 0,
        context_bytes: 0,
        input_bytes: 1,
        attempts: Vec::new(),
        usage: Usage::default(),
        cost_usd: None,
        cost_reason: None,
        error: None,
        started_at: "2030-01-01T00:00:00Z".into(),
        finished_at: "2030-01-01T00:00:01Z".into(),
    }
}

#[tokio::test]
async fn ready_model_merges_precede_leaf_backlog_even_after_verbatim_joins() {
    for built in [4, 32] {
        let book = TestBook::new();
        book.append(built + 9, 700);
        let mut owner = open_owner(&book).await;
        owner.boundary(Vec::new()).await.expect("ingest records");

        for unit in 0..built as u64 {
            let source = owner
                .live
                .leaf_source(unit)
                .await
                .expect("read leaf source");
            owner
                .put(
                    NodeKey::leaf(unit),
                    "v".repeat(if built == 4 { 200 } else { 400 }),
                    Method::Verbatim,
                    source.unit.digest,
                    None,
                )
                .await
                .expect("store short leaf");
        }
        owner.joins.sort();
        owner.joins.dedup();
        assert_eq!(
            owner.joins.len(),
            if built == 4 { 2 } else { 0 },
            "only sibling pairs within one line join without a model"
        );

        let (endpoint, _requests, model_cancel, model_task) = model_server().await;
        let prefix = Prefix {
            system: Vec::new(),
            tools: Vec::new(),
        };
        let mut config = FolderConfig::local("test-model");
        config.endpoint = endpoint.parse().expect("controlled model endpoint");
        let engine = Engine::open(config, prefix.clone())
            .await
            .expect("open controlled engine");
        let settings = folder();
        owner.config = LiveConfig {
            folder: Some(settings.clone()),
            prefix_digest: Some(prefix.digest()),
        };
        owner.engine = Some(Slot {
            engine,
            folder: settings,
            prefix_digest: prefix.digest(),
        });

        let mut actor = Actor {
            room: Room::default(),
            name: "room".into(),
            store: Store::File(book.path().to_path_buf()),
            owner: Some(owner),
            participants: BTreeSet::from(["session".to_owned()]),
            jobs: tokio::task::JoinSet::new(),
            cancel: CancellationToken::new(),
            shutdown: CancellationToken::new(),
            error: None,
        };
        actor.advance().await;

        let owner = actor.owner.as_ref().expect("actor keeps the room owner");
        let ready_merge = NodeKey {
            level: if built == 4 { 2 } else { 1 },
            index: 0,
        };
        assert!(
            owner.running.contains(&ready_merge),
            "a model-ready parent must start before remaining leaves drain"
        );
        assert!(owner.pending.iter().any(|unit| *unit >= 4));
        if built == 32 {
            assert!(!owner.merges.is_empty(), "merge backlog remains");
            assert!(
                owner.running.iter().any(|key| key.level == 0),
                "bounded leaves must progress while merges remain ready"
            );
        }

        actor.jobs.abort_all();
        while actor.jobs.join_next().await.is_some() {}
        actor.pause().await;
        close_model(model_cancel, model_task).await;
    }
}

#[tokio::test]
async fn completed_merge_updates_saved_views_and_restart_keeps_exact_source() {
    let book = TestBook::new();
    book.append(256, 700);
    let mut owner = open_owner(&book).await;
    owner.boundary(Vec::new()).await.expect("ingest records");

    for unit in 0..owner.units {
        let source = owner
            .live
            .leaf_source(unit)
            .await
            .expect("read leaf source");
        let text = if unit < 2 {
            "x".repeat(MODEL_LINE_CEILING)
        } else {
            format!("line-{unit}-{}", "x".repeat(490))
        };
        owner
            .live
            .put_node(
                NodeKey::leaf(unit),
                text.clone(),
                Method::Model,
                source.unit.digest,
                Some(&accepted_job(NodeKey::leaf(unit))),
            )
            .await
            .expect("store leaf");
        owner.sizes.insert(NodeKey::leaf(unit), text.len());
        owner.pending.remove(&unit);
    }

    let missing = NodeKey { level: 1, index: 0 };
    for level in 1..=8 {
        for index in 0..(256_u64 >> level) {
            let key = NodeKey { level, index };
            if key == missing || (key.first() <= missing.first() && key.end() >= missing.end()) {
                continue;
            }
            let text = format!("node-{level}-{index}-{}", "m".repeat(490));
            let (_, _, _, _, digest) = owner.halves(key).await.expect("read built halves");
            owner
                .live
                .put_node(key, text.clone(), Method::Joined, digest, None)
                .await
                .expect("store built parent");
            owner.sizes.insert(key, text.len());
        }
    }
    owner.maintain_views().await.expect("save pre-merge views");
    let before = owner.compactor.clone();
    let context = owner
        .context(owner.units)
        .await
        .expect("render compactor context");
    assert!(
        context.bytes > COMPACTOR_HIGH,
        "the missing merge must hold the compactor above its budget: {}",
        context.bytes
    );
    assert_ne!(before.lines.first(), Some(&missing));

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    owner.waiters.push(Waiter {
        end: owner.units,
        through_seq: Some(255),
        started: Instant::now(),
        deadline: Instant::now() + Duration::from_secs(30),
        reply: ready_tx,
        prefix_digest: "a".repeat(64),
        prefix_changed: false,
        model: TurnModel {
            provider: "ollama".into(),
            id: "test-model".into(),
        },
    });
    let (_, _, _, _, source_digest) = owner.halves(missing).await.expect("read completed halves");
    owner
        .finish(Finished {
            key: missing,
            source_digest,
            completion: Some(Completion {
                text: Some("merged summary".into()),
                job: accepted_job(missing),
            }),
        })
        .await
        .expect("store completed merge");
    owner
        .dispatch(&mut tokio::task::JoinSet::new(), &CancellationToken::new())
        .await
        .expect("refresh views after completed work without another boundary");
    owner.answer().await;
    let answer = ready_rx
        .await
        .expect("ready turn response")
        .expect("ready turn");
    assert_eq!(answer["ready"], true);
    assert_eq!(answer["chat"]["throughSeq"], 255);
    assert_eq!(owner.compactor.lines.first(), Some(&missing));
    let context = owner
        .context(owner.units)
        .await
        .expect("updated compactor context");
    assert!(
        context.bytes <= COMPACTOR_HIGH,
        "updated context: {}",
        context.bytes
    );

    let expected_chat = owner.chat.clone();
    let expected_compactor = owner.compactor.clone();
    drop(owner);

    let (reopened, snapshot) = Live::open(Store::File(book.path().to_path_buf()), "room")
        .await
        .expect("reopen test room")
        .expect("reopen room lock");
    assert_eq!(snapshot.chat, Some(expected_chat));
    assert_eq!(snapshot.compactor, Some(expected_compactor));
    let (_, original) = store::read_unit(&Store::File(book.path().to_path_buf()), "room", 0)
        .await
        .expect("read original source after restart")
        .expect("original source exists after restart");
    assert_eq!(original.text, format!("source-0-{}", "x".repeat(700)));
    drop(reopened);
}

struct IdleRoom {
    book: TestBook,
    room: Room,
    failing: Arc<AtomicBool>,
    registered: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
    model_cancel: CancellationToken,
    model_task: JoinHandle<()>,
}

impl IdleRoom {
    async fn failed() -> Self {
        let book = TestBook::new();
        book.append(1, 700);
        let mut owner = open_owner(&book).await;
        owner
            .boundary(Vec::new())
            .await
            .expect("ingest original unit");

        let failing = Arc::new(AtomicBool::new(true));
        let (endpoint, requests, model_cancel, model_task) =
            controlled_model_server(failing.clone()).await;
        let prefix = Prefix {
            system: Vec::new(),
            tools: Vec::new(),
        };
        let digest = owner.live.put_prefix(&prefix).await.expect("save prefix");
        let settings = folder();
        owner.config = LiveConfig {
            folder: Some(settings.clone()),
            prefix_digest: Some(digest.clone()),
        };
        owner
            .live
            .save_config(&owner.config)
            .await
            .expect("save config");

        let mut config = FolderConfig::local("test-model");
        config.endpoint = endpoint.parse().expect("controlled endpoint");
        owner.engine = Some(Slot {
            engine: Engine::open(config, prefix)
                .await
                .expect("open controlled engine"),
            folder: settings,
            prefix_digest: digest,
        });

        let room = Room::default();
        let (sender, commands) = mpsc::channel(COMMANDS);
        assert!(room.0.commands.set(sender).is_ok());
        let registered = Arc::new(AtomicBool::new(true));
        let live_registration = registered.clone();
        let cancel = CancellationToken::new();
        let actor = Actor {
            room: room.clone(),
            name: "room".into(),
            store: Store::File(book.path().to_path_buf()),
            owner: Some(owner),
            participants: BTreeSet::from(["session".to_owned()]),
            jobs: JoinSet::new(),
            cancel: cancel.child_token(),
            shutdown: cancel.clone(),
            error: None,
        };
        let task = tokio::spawn(serve_actor(
            actor,
            commands,
            Box::new(move || {
                let sessions = if live_registration.load(Ordering::SeqCst) {
                    vec!["session".to_owned()]
                } else {
                    Vec::new()
                };
                Box::pin(async move { sessions })
            }),
            cancel.clone(),
        ));
        let fixture = Self {
            book,
            room,
            failing,
            registered,
            requests,
            cancel,
            task,
            model_cancel,
            model_task,
        };
        fixture.status_when(|status| status["failed"] == 1).await;
        assert!(fixture.room.status()["lastError"].is_string());
        fixture
    }

    async fn status_when(&self, ready: impl Fn(&Value) -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if ready(&self.room.status()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("actor publishes the expected state");
    }

    async fn close(self) {
        self.cancel.cancel();
        self.task.await.expect("stop live actor");
        close_model(self.model_cancel, self.model_task).await;
    }
}

async fn elapse_idle(seconds: u64) {
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(seconds)).await;
    tokio::time::resume();
    tokio::time::sleep(Duration::from_millis(20)).await;
}

#[tokio::test]
async fn idle_retry_recovers_original_node_without_new_boundary() {
    let fixture = IdleRoom::failed().await;
    let failures = fixture.requests.load(Ordering::SeqCst);
    let before = fixture.room.subagent().await.expect("read failed view");
    assert_eq!(before["complete"], false);
    fixture.failing.store(false, Ordering::SeqCst);

    elapse_idle(59).await;
    assert_eq!(fixture.requests.load(Ordering::SeqCst), failures);
    assert_eq!(
        fixture.room.subagent().await.expect("unrelated read")["complete"],
        false
    );
    elapse_idle(2).await;
    fixture
        .status_when(|status| status["leavesBuilt"] == 1)
        .await;

    let ready = fixture.room.subagent().await.expect("read recovered view");
    assert_eq!(ready["complete"], true);
    assert_eq!(ready["chat"]["units"], 1);
    assert_eq!(fixture.requests.load(Ordering::SeqCst), failures + 1);

    let store = Store::File(fixture.book.path().to_path_buf());
    let node = store::read_node(&store, "room", NodeKey::leaf(0))
        .await
        .expect("read persisted recovered node")
        .expect("recovered node exists");
    assert_eq!(node.method, Method::Model);
    assert_eq!(node.text, "s".repeat(480));
    let (chat, compactor) = store::read_views(&store, "room")
        .await
        .expect("read saved views");
    assert_eq!(chat.expect("saved chat").lines, vec![NodeKey::leaf(0)]);
    assert_eq!(
        compactor.expect("saved compactor").lines,
        vec![NodeKey::leaf(0)]
    );
    let (_, original) = store::read_unit(&store, "room", 0)
        .await
        .expect("read original")
        .expect("original remains");
    assert_eq!(original.text, format!("source-0-{}", "x".repeat(700)));

    elapse_idle(120).await;
    assert_eq!(fixture.requests.load(Ordering::SeqCst), failures + 1);
    fixture.close().await;
}

#[tokio::test]
async fn idle_retry_stops_after_leave_registration_loss_or_cancellation() {
    for ending in ["leave", "unregistered", "cancel"] {
        let fixture = IdleRoom::failed().await;
        let failures = fixture.requests.load(Ordering::SeqCst);
        fixture.failing.store(false, Ordering::SeqCst);

        match ending {
            "leave" => {
                fixture.room.leave("session".into()).await;
                fixture
                    .status_when(|status| status["participants"] == 0)
                    .await;
            }
            "unregistered" => {
                fixture.registered.store(false, Ordering::SeqCst);
                elapse_idle(31).await;
                fixture
                    .status_when(|status| status["participants"] == 0)
                    .await;
            }
            "cancel" => {
                fixture.cancel.cancel();
                fixture
                    .status_when(|status| status["participants"] == 0)
                    .await;
            }
            _ => unreachable!(),
        }

        elapse_idle(120).await;
        assert_eq!(
            fixture.requests.load(Ordering::SeqCst),
            failures,
            "{ending} prohibits another model start"
        );
        assert!(
            store::read_node(
                &Store::File(fixture.book.path().to_path_buf()),
                "room",
                NodeKey::leaf(0),
            )
            .await
            .expect("read stopped node")
            .is_none()
        );
        fixture.close().await;
    }
}
