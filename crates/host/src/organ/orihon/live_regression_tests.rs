use super::*;
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

fn capture(
    source_id: String,
    parent: Option<&str>,
    kind: Kind,
    text: String,
) -> hearth::orihon::CaptureRecord {
    hearth::orihon::CaptureRecord {
        source_id,
        parent_user_id: parent.map(str::to_owned),
        kind,
        speaker: "Operator".into(),
        text,
        said_at: "2030-01-01T00:00:00Z".into(),
        timestamp_observed: false,
        redacted: false,
        fold_text: None,
        children: None,
    }
}

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
                .map(|index| {
                    capture(
                        format!("user-{index}"),
                        None,
                        Kind::User,
                        format!("source-{index}-{}", "x".repeat(bytes)),
                    )
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

    /// Accepts one page of a live harness session: a user input and the talk records it
    /// parents. Returns the seq of the page's last record.
    fn page(&self, session: &str, input: &str, replies: &[&str]) -> i64 {
        let user = capture(input.into(), None, Kind::User, format!("said {input}"));
        let replies = replies.iter().map(|reply| {
            let text = format!("said {reply}");
            capture((*reply).into(), Some(input), Kind::Talk, text)
        });
        origami::orihon::append_file(
            &self.0,
            "room",
            "writer",
            hearth::orihon::CaptureBatch {
                harness: HARNESS.into(),
                source_session: session.into(),
                records: std::iter::once(user).chain(replies).collect(),
            },
        )
        .expect("append test page")
        .accepted_records
        .last()
        .expect("the page was accepted")
        .seq
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

#[tokio::test]
async fn eviction_removes_only_view_lines_and_never_starts_parent_work() {
    let book = TestBook::new();
    book.append(310, 430);
    let mut owner = open_owner(&book).await;
    owner.boundary(Vec::new()).await.expect("ingest records");
    assert_eq!(
        (owner.units, owner.pending.len()),
        (310, 0),
        "every source fits verbatim"
    );

    let (endpoint, requests, model_cancel, model_task) = model_server().await;
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

    let mut jobs = JoinSet::new();
    owner
        .dispatch(&mut jobs, &CancellationToken::new())
        .await
        .expect("dispatch after ingest");
    assert!(jobs.is_empty(), "short messages must not start parent jobs");
    assert_eq!(requests.load(Ordering::Relaxed), 0);

    let keys = owner.chat.cut(owner.units, &owner.sizes).expect("cut view");
    let first = keys.first().expect("view has recent records").index;
    assert!(first > 0, "crossing the high watermark must evict the head");
    assert_eq!(keys.last(), Some(&NodeKey::leaf(309)));
    assert!(keys.iter().all(|key| key.level == 0));
    let rendered = owner.render(&keys).await.expect("render chat");
    assert!(rendered.bytes <= tree::CHAT_LOW + tree::CHAT_OPEN.len() + tree::CHAT_CLOSE.len());
    assert!(owner.compactor.bytes(&owner.sizes) <= tree::COMPACTOR_LOW);
    println!(
        "retained {first}..310; {} rendered bytes; zero parent jobs",
        rendered.bytes
    );

    let expected_chat = owner.chat.clone();
    drop(owner);
    let reopened = open_owner(&book).await;
    assert_eq!(reopened.chat, expected_chat);
    let (_, original) = store::read_unit(&Store::File(book.path().to_path_buf()), "room", 0)
        .await
        .expect("read evicted original")
        .expect("source survives view eviction");
    assert_eq!(original.text, format!("source-0-{}", "x".repeat(430)));
    close_model(model_cancel, model_task).await;
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

async fn fold_owner(book: &TestBook) -> Owner {
    let mut owner = open_owner(book).await;
    owner.config.folder = Some(folder());
    owner
}

/// Places one turn of session `live`: the seq its view runs through and the view's text, or
/// the refusal code.
async fn place(
    owner: &mut Owner,
    through: Option<&str>,
    generation_start_seq: i64,
) -> Result<(Option<i64>, String), String> {
    let now = Instant::now();
    let request = TurnRequest {
        session: "live".into(),
        through_source_id: through.map(str::to_owned),
        generation_start_seq,
        prefix: Prefix {
            system: Vec::new(),
            tools: Vec::new(),
        },
        model: TurnModel {
            provider: "test".into(),
            id: "test-model".into(),
        },
        folder: folder(),
        started: now,
        deadline: now,
    };
    let accepted = owner
        .begin_turn(&request, Vec::new())
        .await
        .map_err(|error| error.code)?;
    let keys = owner
        .chat
        .cut(accepted.end, &owner.sizes)
        .expect("short records are built at ingest");
    let view = owner.render(&keys).await.expect("render the turn view");
    Ok((accepted.through_seq, view.text()))
}

#[tokio::test]
async fn the_first_turn_of_a_generation_keeps_older_pages_before_its_new_input() {
    let book = TestBook::new();
    book.page("live", "old-a", &["old-a-reply"]);
    let start = book.page("live", "old-b", &[]) + 1;
    let before = book.page("peer", "peer-a", &["peer-a-reply"]);
    book.page("live", "new-a", &["new-a-reply"]);
    let mut owner = fold_owner(&book).await;

    let (through_seq, view) = place(&mut owner, None, start)
        .await
        .expect("older pages of the session do not make its first turn stale");
    assert_eq!(through_seq, Some(before));
    for kept in ["old-a", "old-a-reply", "old-b", "peer-a", "peer-a-reply"] {
        assert!(view.contains(&format!("said {kept}\n")), "{kept} stays");
    }
    assert!(!view.contains("said new-a"), "the new input is not history");
}

#[tokio::test]
async fn a_first_input_at_the_room_start_has_no_history() {
    let book = TestBook::new();
    book.page("live", "new-a", &["new-a-reply"]);
    let mut owner = fold_owner(&book).await;

    let (through_seq, view) = place(&mut owner, None, 0)
        .await
        .expect("the room's first turn");
    assert_eq!(through_seq, None);
    assert!(!view.contains("said new-a"));
}

#[tokio::test]
async fn a_generation_without_records_of_its_session_is_cut_at_the_ingested_room() {
    let book = TestBook::new();
    book.page("live", "old-a", &["old-a-reply"]);
    let start = book.page("live", "old-b", &[]) + 1;
    let last = book.page("peer", "peer-a", &[]);
    let mut owner = fold_owner(&book).await;

    let (through_seq, view) = place(&mut owner, None, start)
        .await
        .expect("a turn before the session's next record");
    assert_eq!(through_seq, Some(last));
    for kept in ["old-a", "old-a-reply", "old-b", "peer-a"] {
        assert!(view.contains(&format!("said {kept}\n")), "{kept} stays");
    }
}

#[tokio::test]
async fn more_than_one_new_input_after_the_frontier_is_stale() {
    let book = TestBook::new();
    let start = book.page("live", "old-a", &[]) + 1;
    book.page("live", "new-a", &["new-a-reply"]);
    book.page("live", "new-b", &[]);
    book.page("live", "new-c", &[]);
    let mut owner = fold_owner(&book).await;

    for through in [None, Some("new-a-reply")] {
        assert_eq!(
            place(&mut owner, through, start).await.unwrap_err(),
            "orihon_boundary_stale",
            "through {through:?}"
        );
    }
}

#[tokio::test]
async fn a_named_source_is_an_accepted_record_of_the_current_generation() {
    let book = TestBook::new();
    let start = book.page("live", "old-a", &["old-a-reply"]) + 1;
    book.page("peer", "peer-a", &[]);
    let named = book.page("live", "new-a", &["new-a-reply"]);
    let mut owner = fold_owner(&book).await;

    // Another session's record is not this session's source.
    for absent in ["absent", "peer-a"] {
        assert_eq!(
            place(&mut owner, Some(absent), start).await.unwrap_err(),
            "orihon_boundary_missing",
            "through {absent}"
        );
    }
    // One new input follows the older source, yet an earlier generation accepted it.
    assert_eq!(
        place(&mut owner, Some("old-a-reply"), start)
            .await
            .unwrap_err(),
        "orihon_boundary_stale"
    );

    book.page("live", "new-b", &[]);
    let (through_seq, view) = place(&mut owner, Some("new-a-reply"), start)
        .await
        .expect("a source of the current generation");
    assert_eq!(through_seq, Some(named));
    for kept in ["old-a", "old-a-reply", "peer-a", "new-a", "new-a-reply"] {
        assert!(view.contains(&format!("said {kept}\n")), "{kept} stays");
    }
    assert!(!view.contains("said new-b"), "the new input is not history");
}
