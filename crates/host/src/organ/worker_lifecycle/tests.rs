use super::{Failure, Journal};
use hearth::worker_lifecycle::{Call, LifecycleError};
use serde_json::{Value, json};
use std::path::PathBuf;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("athanor-worker-lifecycle-{}", uuid::Uuid::new_v4()));
        Self(dir.join("worker-lifecycle").join("journal.jsonl"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(root) = self.0.parent().and_then(|parent| parent.parent()) {
            // Test scratch only; a leftover temp directory is harmless.
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

fn call(session: &str, request_id: &str, params: Value) -> Call {
    let Value::Object(mut map) = params else {
        panic!("params object")
    };
    map.insert("houseId".into(), json!("house"));
    map.insert("room".into(), json!("kodo"));
    map.insert("spirit".into(), json!("Kodo"));
    map.insert("session".into(), json!(session));
    map.insert("requestId".into(), json!(request_id));
    Call::from_params(Value::Object(map)).expect("valid call")
}

fn run(journal: &mut Journal, session: &str, request_id: &str, params: Value) -> Value {
    match journal.execute(&call(session, request_id, params)) {
        Ok(value) => value,
        Err(Failure::Domain(error)) => panic!("domain refusal: {error:?}"),
        Err(Failure::Io(message)) => panic!("io failure: {message}"),
    }
}

fn refusal(journal: &mut Journal, session: &str, request_id: &str, params: Value) -> &'static str {
    match journal.execute(&call(session, request_id, params)) {
        Err(Failure::Domain(LifecycleError::Refused { code, .. })) => code,
        Ok(value) => panic!("expected refusal, got {value}"),
        Err(_) => panic!("expected a domain refusal"),
    }
}

fn token(seed: char) -> String {
    std::iter::repeat_n(seed, 64).collect()
}

fn prepare(seed: char) -> Value {
    json!({"action": "prepare", "swarmKey": "toolcall-1", "slotKey": "0", "agentName": "task", "task": "raise the wall", "token": token(seed)})
}

fn open(seed: char) -> Value {
    json!({"action": "open", "token": token(seed), "agentId": "0-Task", "agentName": "task"})
}

#[test]
fn a_second_journal_owner_is_refused_until_the_first_releases_it() {
    let scratch = Scratch::new();
    let first = Journal::load(&scratch.0).unwrap();
    assert!(matches!(Journal::load(&scratch.0), Err(Failure::Io(_))));
    drop(first);
    let mut reopened = Journal::load(&scratch.0).unwrap();
    let prepared = run(
        &mut reopened,
        "parent",
        "prepare-after-release",
        prepare('a'),
    );
    assert_eq!(prepared["state"], "working");
}

// Kills: Vault state living only in memory, a cold reload forgetting the
// handoff latch or retirement, or retries minting new identities after restart.
#[test]
fn cold_reload_preserves_latch_retirement_and_retry_receipts() {
    let scratch = Scratch::new();
    let mut journal = Journal::load(&scratch.0).unwrap();
    let prepared = run(&mut journal, "parent", "prep", prepare('a'));
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    run(&mut journal, "child", "open", open('a'));
    run(
        &mut journal,
        "child",
        "start",
        json!({"action": "append", "bookId": book, "kind": "tool-start", "operationId": "call-1", "body": {"tool": "edit"}}),
    );
    let latched = run(
        &mut journal,
        "child",
        "obs",
        json!({"action": "observe", "bookId": book, "tokens": 610, "contextWindow": 1000, "requestStarted": true}),
    );
    assert_eq!(latched["state"], "handoff_only");
    drop(journal);

    let mut journal = Journal::load(&scratch.0).unwrap();
    assert_eq!(run(&mut journal, "parent", "prep", prepare('a')), prepared);
    let resumed = run(&mut journal, "child", "open-again", open('a'));
    assert_eq!(resumed["wake"]["kind"], "resume");
    assert_eq!(resumed["state"], "handoff_only");
    assert_eq!(resumed["pendingOperations"][0]["operationId"], "call-1");
    let handoff = run(
        &mut journal,
        "child",
        "handoff",
        json!({"action": "checkpoint", "bookId": book, "report": {"done": "half", "pending": "call-1 edit may have applied"}}),
    );
    assert_eq!(handoff["state"], "retired");
    drop(journal);

    let mut journal = Journal::load(&scratch.0).unwrap();
    assert_eq!(
        refusal(&mut journal, "child", "late", open('a')),
        "instance_retired"
    );
    let text = std::fs::read_to_string(&scratch.0).unwrap();
    assert!(
        !text.contains(&token('a')),
        "the journal stores token hashes only"
    );
}

// Kills: Vault mode starting a named familiar without role lessons, the
// refusal binding or journaling the reservation anyway, or ordinary agents
// losing their Vault wake.
#[test]
fn vault_mode_refuses_named_familiars_without_journaling_the_open() {
    let scratch = Scratch::new();
    let mut journal = Journal::load(&scratch.0).unwrap();
    run(
        &mut journal,
        "parent",
        "prep-familiar",
        json!({"action": "prepare", "swarmKey": "toolcall-2", "slotKey": "0", "agentName": "lavra-dragonling", "task": "raise the wall", "token": token('f')}),
    );
    let familiar = json!({"action": "open", "token": token('f'), "agentId": "0-Lavra", "agentName": "lavra-dragonling", "acceptsFamiliarLessons": true});
    assert_eq!(
        refusal(&mut journal, "child", "open", familiar.clone()),
        "familiar_lessons_unavailable"
    );
    drop(journal);

    // Nothing was journaled: after a cold reload another session still meets
    // the same door instead of a claimed reservation.
    let mut journal = Journal::load(&scratch.0).unwrap();
    assert_eq!(
        refusal(&mut journal, "other-child", "open", familiar),
        "familiar_lessons_unavailable"
    );
    run(&mut journal, "parent", "prep", prepare('a'));
    let opened = run(&mut journal, "child", "open-task", open('a'));
    assert_eq!(opened["wake"]["kind"], "opening");
    assert_eq!(opened.get("familiarLessons"), Some(&Value::Null));
}

// Kills: a torn final line making the journal unloadable, or later appends
// landing after garbage.
#[test]
fn torn_tail_is_truncated_and_appends_continue() {
    let scratch = Scratch::new();
    let mut journal = Journal::load(&scratch.0).unwrap();
    let prepared = run(&mut journal, "parent", "prep", prepare('a'));
    drop(journal);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&scratch.0)
        .unwrap();
    std::io::Write::write_all(&mut file, br#"{"version":1,"entries":[{"#).unwrap();
    drop(file);

    let mut journal = Journal::load(&scratch.0).unwrap();
    let book = prepared["bookId"].as_str().unwrap();
    run(&mut journal, "child", "open", open('a'));
    run(
        &mut journal,
        "child",
        "note",
        json!({"action": "append", "bookId": book, "kind": "note", "body": "after the tear"}),
    );
    drop(journal);
    let mut journal = Journal::load(&scratch.0).unwrap();
    let page = run(
        &mut journal,
        "parent",
        "read",
        json!({"action": "read", "bookId": book}),
    );
    assert!(
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["body"] == "after the tear")
    );
}

// Kills: disposal leaving transient entries in the live journal, or losing
// accepted evidence and the receipt with them.
#[test]
fn disposal_compacts_entries_and_keeps_retained_evidence() {
    let scratch = Scratch::new();
    let mut journal = Journal::load(&scratch.0).unwrap();
    let prepared = run(&mut journal, "parent", "prep", prepare('a'));
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    let swarm = prepared["swarmId"].as_str().unwrap().to_owned();
    run(&mut journal, "child", "open", open('a'));
    run(
        &mut journal,
        "child",
        "note",
        json!({"action": "append", "bookId": book, "kind": "note", "body": "transient-scratch-marker"}),
    );
    run(
        &mut journal,
        "child",
        "done",
        json!({"action": "checkpoint", "bookId": book, "report": "finished", "terminal": true}),
    );
    run(
        &mut journal,
        "parent",
        "accept",
        json!({"action": "accept", "bookId": book, "body": "retained-evidence-marker"}),
    );
    let receipt = run(
        &mut journal,
        "parent",
        "dispose",
        json!({"action": "dispose", "swarmId": swarm}),
    );
    assert_eq!(receipt["backupsErased"], false);
    let text = std::fs::read_to_string(&scratch.0).unwrap();
    assert!(!text.contains("transient-scratch-marker"));
    assert!(text.contains("retained-evidence-marker"));
    drop(journal);

    let mut journal = Journal::load(&scratch.0).unwrap();
    assert_eq!(
        refusal(
            &mut journal,
            "parent",
            "read",
            json!({"action": "read", "bookId": book})
        ),
        "unknown_book"
    );
    assert_eq!(
        run(
            &mut journal,
            "parent",
            "dispose",
            json!({"action": "dispose", "swarmId": swarm})
        ),
        receipt
    );
    assert_eq!(journal.retained.len(), 1);
    assert_eq!(journal.dispositions.len(), 1);
}

// Kills: accepted evidence unreadable after disposal (or after a cold reload
// of the compacted journal), and children or foreign rooms reading it.
#[test]
fn retained_evidence_is_readable_by_its_parent_after_disposal() {
    let scratch = Scratch::new();
    let mut journal = Journal::load(&scratch.0).unwrap();
    let prepared = run(&mut journal, "parent", "prep", prepare('a'));
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    let swarm = prepared["swarmId"].as_str().unwrap().to_owned();
    run(&mut journal, "child", "open", open('a'));
    run(
        &mut journal,
        "child",
        "note",
        json!({"action": "append", "bookId": book, "kind": "note", "body": "transient-only"}),
    );
    run(
        &mut journal,
        "child",
        "done",
        json!({"action": "checkpoint", "bookId": book, "report": "finished", "terminal": true}),
    );
    run(
        &mut journal,
        "parent",
        "acc-1",
        json!({"action": "accept", "bookId": book, "body": "kept-one"}),
    );
    run(
        &mut journal,
        "parent",
        "acc-2",
        json!({"action": "accept", "bookId": book, "body": "kept-two"}),
    );
    run(
        &mut journal,
        "parent",
        "dispose",
        json!({"action": "dispose", "swarmId": swarm}),
    );
    drop(journal);

    let mut journal = Journal::load(&scratch.0).unwrap();
    let first = run(
        &mut journal,
        "parent",
        "r1",
        json!({"action": "retained", "swarmId": swarm, "limit": 1}),
    );
    assert_eq!(first["retained"].as_array().unwrap().len(), 1);
    assert_eq!(first["retained"][0]["acceptedBySession"], "parent");
    let cursor = first["nextCursor"].as_str().unwrap().to_owned();
    assert_eq!(first["retained"][0]["retainedId"], json!(cursor));
    let second = run(
        &mut journal,
        "parent",
        "r2",
        json!({"action": "retained", "swarmId": swarm, "after": cursor}),
    );
    assert_eq!(second["retained"].as_array().unwrap().len(), 1);
    assert_eq!(second["nextCursor"], Value::Null);
    // Two accepts may share a millisecond; retainedId breaks the tie, so
    // assert the pages cover both records exactly once.
    let mut bodies = [
        &first["retained"][0]["body"],
        &second["retained"][0]["body"],
    ]
    .map(|body| body.as_str().unwrap().to_owned());
    bodies.sort();
    assert_eq!(bodies, ["kept-one".to_owned(), "kept-two".to_owned()]);
    assert!(!second.to_string().contains("transient-only"));

    assert_eq!(
        refusal(
            &mut journal,
            "child",
            "r3",
            json!({"action": "retained", "swarmId": swarm})
        ),
        "parent_only"
    );
    assert_eq!(
        refusal(
            &mut journal,
            "parent",
            "r4",
            json!({"action": "retained", "swarmId": swarm, "after": "missing"})
        ),
        "unknown_cursor"
    );
    let foreign = journal.execute(&{
        let mut foreign = call(
            "parent",
            "r5",
            json!({"action": "retained", "swarmId": swarm}),
        );
        foreign.actor.room = "elsewhere".into();
        foreign
    });
    assert!(matches!(
        foreign,
        Err(Failure::Domain(LifecycleError::Refused {
            code: "unknown_swarm",
            ..
        }))
    ));
}

#[test]
fn cold_reload_finishes_disposal_committed_before_compaction() {
    let scratch = Scratch::new();
    let mut journal = Journal::load(&scratch.0).unwrap();
    let prepared = run(&mut journal, "parent", "prep", prepare('a'));
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    let swarm = prepared["swarmId"].as_str().unwrap().to_owned();
    run(&mut journal, "child", "open", open('a'));
    run(
        &mut journal,
        "child",
        "note",
        json!({"action":"append","bookId":book,"kind":"note","body":"uncompacted-transient"}),
    );
    run(
        &mut journal,
        "child",
        "done",
        json!({"action":"checkpoint","bookId":book,"report":"done","terminal":true}),
    );
    run(
        &mut journal,
        "parent",
        "accept",
        json!({"action":"accept","bookId":book,"body":"retained-after-recovery"}),
    );
    let mut before_compaction = std::fs::read(&scratch.0).unwrap();
    let receipt = run(
        &mut journal,
        "parent",
        "dispose",
        json!({"action":"dispose","swarmId":swarm}),
    );
    let committed = super::Commit {
        version: super::JOURNAL_VERSION,
        swarm: Some(journal.swarms[&swarm].clone()),
        disposition: Some(journal.dispositions[0].clone()),
        requests: journal.requests.values().cloned().collect(),
        ..super::Commit::default()
    };
    serde_json::to_writer(&mut before_compaction, &committed).unwrap();
    before_compaction.push(b'\n');
    drop(journal);
    std::fs::write(&scratch.0, before_compaction).unwrap();

    let mut restored = Journal::load(&scratch.0).unwrap();
    assert_eq!(
        run(
            &mut restored,
            "parent",
            "dispose",
            json!({"action":"dispose","swarmId":swarm})
        ),
        receipt
    );
    let live = std::fs::read_to_string(&scratch.0).unwrap();
    assert!(!live.contains("uncompacted-transient"));
    assert!(live.contains("retained-after-recovery"));
}

fn host_config(root: &std::path::Path) -> crate::HostConfig {
    let config = crate::HostConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        bearer_token: "test-token".into(),
        room_dir: root.join("room"),
        state_dir: root.join("state"),
        house_id: "test-house".into(),
        room: "test-room".into(),
        spirit: "Test".into(),
        session: "host-session".into(),
        database_url: None,
        nats_url: None,
        nats_auth: None,
        knock_autonomy: crate::config::KnockAutonomy::Off,
        operator: None,
    };
    let state = config.room_state_path();
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::write(&state, r#"{"room":"test-room","embodiedSpirit":"Test"}"#).unwrap();
    config
}

fn meta(session: &str) -> protocol::CommandMeta {
    protocol::CommandMeta {
        schema_version: 1,
        message_id: format!("message-{session}"),
        house_id: "test-house".into(),
        sender_room: "test-room".into(),
        sender_spirit: "Test".into(),
        sender_session: session.into(),
        recipient: "house-host".into(),
        correlation_id: "correlation".into(),
        causation_id: String::new(),
        reply_target: session.into(),
        idempotency_key: "key".into(),
        source_record_refs: vec![],
        scope: "room:test-room:recall_policy".into(),
        visibility: "operator".into(),
        authority_class: "room_state".into(),
        created_at: "2026-10-09T00:00:00Z".into(),
        expires_at: "2099-01-01T00:00:00Z".into(),
        max_hops: 1,
        projection_id: "context".into(),
    }
}

async fn organ(config: &crate::HostConfig, session: &str, params: Value) -> Result<Value, String> {
    let giga = tokio::sync::Mutex::new(crate::organ::GigaRoomWorker::default());
    let request = protocol::organ::OrganRequest {
        operation: protocol::organ::OrganOperation::WorkerLifecycle,
        params: params.as_object().unwrap().clone(),
        target_scope: protocol::organ::OrganTargetScope::Room,
    };
    match crate::organ::call(config, None, &meta(session), request, &giga).await {
        protocol::ResponsePayload::Result { result } => Ok(result),
        protocol::ResponsePayload::Error { error } => Err(error.code),
    }
}

// Kills: callers naming their own identity, the Host binding the wrong session
// as owner or worker, or AKASHA mode silently falling back to the Vault journal.
#[tokio::test]
async fn host_injects_identity_and_selects_exactly_one_store() {
    let scratch = Scratch::new();
    let root = scratch.0.parent().unwrap().parent().unwrap().to_path_buf();
    let config = host_config(&root);
    let spoof = organ(
        &config,
        "parent",
        json!({"action": "inspect", "requestId": "i", "session": "child"}),
    )
    .await;
    assert_eq!(spoof.unwrap_err(), "invalid_params");

    let mut prepare = prepare('a');
    prepare["requestId"] = json!("prep");
    let prepared = organ(&config, "parent", prepare).await.unwrap();
    let mut opening = open('a');
    opening["requestId"] = json!("open");
    organ(&config, "child", opening.clone()).await.unwrap();
    assert_eq!(
        organ(&config, "thief", opening).await.unwrap_err(),
        "reservation_claimed"
    );
    let book = prepared["bookId"].as_str().unwrap();
    let parent_note = organ(
        &config,
        "parent",
        json!({"action": "append", "requestId": "n", "bookId": book, "kind": "note", "body": "x"}),
    )
    .await;
    assert_eq!(parent_note.unwrap_err(), "kind_not_allowed");
    let journal = super::journal_path(&config);
    assert!(journal.starts_with(&config.state_dir));
    let written = std::fs::metadata(&journal).unwrap().len();

    let mut native = config.clone();
    native.database_url = Some("postgres://worker:test@127.0.0.1:1/worker_lifecycle_test".into());
    let refused = organ(
        &native,
        "parent",
        json!({"action": "inspect", "requestId": "i2"}),
    )
    .await;
    assert!(
        refused.is_err(),
        "AKASHA mode without a runtime must refuse, not use the journal"
    );
    assert_eq!(std::fs::metadata(&journal).unwrap().len(), written);
}
