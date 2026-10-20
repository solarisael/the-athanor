use super::*;
use crate::config::KnockAutonomy;
use crate::store::atomic_json_write;
use std::fs;
use std::path::{Path, PathBuf};

const MANUAL_SPIRIT_BODY: &str = "# Fixture\n\nKeep this manually authored 𝔽₂ body.\n";

struct Fixture {
    root: PathBuf,
    config: HostConfig,
    room_root: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("athanor-recall-atomicity-{}", new_id()));
        let config = HostConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            bearer_token: "test-token".into(),
            room_dir: root.join("room"),
            state_dir: root.join("state"),
            house_id: "test-house".into(),
            room: "atomicity-test".into(),
            spirit: "Fixture".into(),
            session: "host-session".into(),
            database_url: None,
            nats_url: None,
            nats_auth: None,
            knock_autonomy: KnockAutonomy::Off,
            operator: None,
        };
        let room_root = json!({
            "room": config.room,
            "operator": "Fixture operator",
            "agentName": "Fixture agent",
            "embodiedSpirit": config.spirit,
            "unrelated": {"nested": ["preserve", "𝔽₂"], "enabled": true},
            "recallPolicy": {
                "requestedMode": "auto",
                "resolvedMode": "conversation",
                "activeProject": null,
                "resolutionReason": "fixture",
                "lastRefreshReason": null,
                "lastRefreshAt": null,
                "workingSetEntries": 0,
                "recoveryPending": true,
                "recoveryTerms": ["𝔽₂", "naïve"],
                "degraded": null,
                "updatedAt": null,
                "extension": {"owner": "not-recall", "values": [3, 8]}
            }
        });
        atomic_json_write(&config.room_state_path(), &room_root).unwrap();
        fs::write(config.room_dir.join("active_spirit.md"), MANUAL_SPIRIT_BODY).unwrap();

        Self {
            root,
            config,
            room_root,
        }
    }

    fn open(&self) -> Host {
        self.try_open().expect("open only the temporary Host store")
    }

    fn try_open(&self) -> Result<Host, String> {
        Host::new(
            self.config.clone(),
            None,
            CancellationToken::new(),
            TaskTracker::new(),
        )
    }

    fn command(&self, kind: &str, key: &str, session: &str, payload: Value) -> Value {
        let mut command = json!({
            "schema_version": HOST_SCHEMA_VERSION,
            "message_id": key,
            "house_id": self.config.house_id,
            "sender_room": self.config.room,
            "sender_spirit": self.config.spirit,
            "sender_session": session,
            "recipient": HOST_RECIPIENT,
            "command_or_event_type": kind,
            "correlation_id": key,
            "causation_id": "",
            "reply_target": session,
            "idempotency_key": key,
            "source_record_refs": [],
            "scope": self.config.scope(),
            "visibility": "operator",
            "authority_class": "room_state",
            "created_at": timestamp(),
            "expires_at": "2099-01-01T00:00:00Z",
            "max_hops": 1,
            "projection_id": RECALL_POLICY_PROJECTION_ID
        });
        command
            .as_object_mut()
            .unwrap()
            .extend(payload.as_object().unwrap().clone());

        command
    }

    fn completion(&self, key: &str, session: &str, entries: u64) -> Value {
        self.command(
            protocol::RECALL_POLICY_COMPLETE_REFRESH,
            key,
            session,
            json!({"refresh": {
                "query_terms": ["𝔽₂", "naïve"],
                "refresh_reason": "explicit-lookup",
                "entries": entries,
                "has_working_set": true,
                "warning": null
            }}),
        )
    }

    async fn seed(&self, host: &Host) {
        for (key, session, entries) in [("seed-alpha", "alpha", 3), ("seed-beta", "beta", 5)] {
            let command = self.completion(key, session, entries);
            let response = process_text(&host.state, &command.to_string()).await;
            let accepted = outcome_event(&response);

            assert_eq!(
                accepted.meta.command_or_event_type,
                RECALL_POLICY_COMMAND_ACCEPTED
            );
            assert_eq!(accepted.state.unwrap().working_set_entries, entries);
            assert!(
                response.delta.is_some(),
                "the healthy write must publish its delta"
            );
        }
    }

    fn assert_room_fields(&self, host: &Host) {
        let mut actual = host.state.room_store.read_root().unwrap();
        let mut original = self.room_root.clone();
        assert_eq!(
            actual["recallPolicy"]["extension"],
            original["recallPolicy"]["extension"]
        );

        for root in [&mut actual, &mut original] {
            root.as_object_mut().unwrap().remove("recallPolicy");
            root.as_object_mut().unwrap().remove("lastUpdatedAt");
        }

        assert_eq!(
            actual, original,
            "Recall must preserve unrelated room fields"
        );
        assert_eq!(
            fs::read_to_string(self.config.room_dir.join("active_spirit.md")).unwrap(),
            MANUAL_SPIRIT_BODY,
        );
    }

    fn checkpoint_bytes(&self) -> Vec<(PathBuf, Vec<u8>)> {
        [
            self.config.room_state_path(),
            self.config.state_dir.join("recall-policy-sessions.json"),
            self.config.state_dir.join("recall-policy-cursor.json"),
            self.config.state_dir.join("recall-policy-receipts.json"),
        ]
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove only this test's temporary files");
    }
}

struct BlockedFile {
    path: PathBuf,
    saved: PathBuf,
}

impl BlockedFile {
    fn new(path: PathBuf) -> Self {
        let saved = path.with_extension("hardening-saved");
        fs::rename(&path, &saved).expect("preserve the existing checkpoint");
        fs::create_dir(&path).expect("a directory must block atomic file replacement");

        Self { path, saved }
    }
}

impl Drop for BlockedFile {
    fn drop(&mut self) {
        fs::remove_dir(&self.path).expect("remove only the injected directory");
        fs::rename(&self.saved, &self.path).expect("restore the untouched checkpoint");
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ObservedPolicy {
    projection: RecallPolicyState,
    cursor: ProjectionCursor,
    sessions: HashMap<String, RecallPolicySession>,
}

async fn observe(host: &Host) -> ObservedPolicy {
    let runtime = host.state.runtime.lock().await;

    ObservedPolicy {
        projection: runtime.projection.clone(),
        cursor: runtime.cursor.clone(),
        sessions: runtime.sessions.clone(),
    }
}

fn outcome_event(response: &Responses) -> CommandOutcomeEvent {
    assert_eq!(response.direct.len(), 1);

    serde_json::from_str(&response.direct[0]).expect("a real Host command outcome")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

async fn finish_replay(
    fixture: &Fixture,
    host: Host,
    command: &Value,
    before: &ObservedPolicy,
    first: &Responses,
) {
    let first = outcome_event(first);
    let host = if first.meta.command_or_event_type == RECALL_POLICY_COMMAND_FAILED {
        assert_eq!(
            observe(&host).await,
            *before,
            "FAILED must not publish a partial commit"
        );
        assert_eq!(first.state.as_ref(), Some(&before.projection));
        assert_eq!(first.version, before.cursor.version);
        assert_eq!(first.meta.sequence, before.cursor.sequence);
        assert_eq!(first.meta.state_hash, before.cursor.state_hash);
        drop(host);
        let reopened = fixture.open();
        assert_eq!(
            observe(&reopened).await,
            *before,
            "a rejected commit must stay rejected on reopen"
        );
        reopened
    } else {
        assert_eq!(
            first.meta.command_or_event_type,
            RECALL_POLICY_COMMAND_ACCEPTED
        );
        host
    };
    let response = process_text(&host.state, &command.to_string()).await;
    let accepted = outcome_event(&response);
    let committed = observe(&host).await;

    assert_eq!(
        accepted.meta.command_or_event_type,
        RECALL_POLICY_COMMAND_ACCEPTED
    );
    assert_eq!(accepted.version, before.cursor.version + 1);
    assert_eq!(accepted.meta.sequence, before.cursor.sequence + 1);
    assert_eq!(committed.projection.working_set_entries, 9);
    assert_eq!(
        committed.sessions["alpha"]
            .projection(String::new())
            .working_set_entries,
        9
    );
    assert_eq!(committed.sessions["beta"], before.sessions["beta"]);
    assert_eq!(host.state.room_store.load().unwrap(), committed.projection);
    fixture.assert_room_fields(&host);
    drop(host);

    let reopened = fixture.open();
    assert_eq!(
        observe(&reopened).await,
        committed,
        "all policy state must reopen together"
    );
    let replay = process_text(&reopened.state, &command.to_string()).await;

    assert_eq!(
        replay.direct, response.direct,
        "the accepted receipt must replay byte-for-byte after reopen"
    );
    assert!(
        replay.delta.is_none(),
        "a replay must not publish a second delta"
    );
    assert_eq!(
        observe(&reopened).await,
        committed,
        "a replay must not advance the cursor or session"
    );
    fixture.assert_room_fields(&reopened);
}

#[tokio::test]
async fn hardening_recall_room_write_refusal_keeps_the_admitted_session() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let before = observe(&host).await;
    let command = fixture.completion("room-unavailable", "alpha", 9);
    let parsed = parse_client_command(command.clone()).unwrap();
    validate_command(&host.state, &parsed).unwrap();
    let hash = semantic_command_hash(&command).unwrap();

    // Admission precedes a room file becoming unavailable to write_policy.
    let blocked = BlockedFile::new(fixture.config.room_state_path());
    let response = execute_command(&host.state, parsed, hash).await;
    let failed = outcome_event(&response);
    let after = observe(&host).await;
    drop(blocked);

    assert_eq!(
        failed.meta.command_or_event_type,
        RECALL_POLICY_COMMAND_FAILED
    );
    assert!(response.delta.is_none());
    assert_eq!(
        after.sessions, before.sessions,
        "room write refusal must not consume the caller's session state"
    );
    finish_replay(&fixture, host, &command, &before, &response).await;
}

#[tokio::test]
async fn hardening_recall_session_checkpoint_failure_is_not_a_partial_commit() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let before = observe(&host).await;
    let command = fixture.completion("session-checkpoint", "alpha", 9);
    let blocked = BlockedFile::new(fixture.config.state_dir.join("recall-policy-sessions.json"));
    let response = process_text(&host.state, &command.to_string()).await;
    let event = outcome_event(&response);
    let after = observe(&host).await;
    drop(blocked);

    if event.meta.command_or_event_type == RECALL_POLICY_COMMAND_FAILED {
        assert!(response.delta.is_none());
        assert_eq!(
            after.sessions, before.sessions,
            "a failed checkpoint must not publish the new working set"
        );
    }

    finish_replay(&fixture, host, &command, &before, &response).await;
}

#[tokio::test]
async fn hardening_recall_cursor_checkpoint_failure_keeps_the_published_version() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let before = observe(&host).await;
    let command = fixture.completion("cursor-checkpoint", "alpha", 9);
    let cursor_path = fixture.config.state_dir.join("recall-policy-cursor.json");
    let durable_cursor: ProjectionCursor = serde_json::from_value(read_json(&cursor_path)).unwrap();
    assert_eq!(durable_cursor, before.cursor);

    let blocked = BlockedFile::new(cursor_path);
    let response = process_text(&host.state, &command.to_string()).await;
    let event = outcome_event(&response);
    let after = observe(&host).await;
    drop(blocked);

    if event.meta.command_or_event_type == RECALL_POLICY_COMMAND_FAILED {
        assert!(response.delta.is_none());
        assert_eq!(
            after.cursor, durable_cursor,
            "FAILED must not advertise an uncommitted version, sequence, or hash"
        );
    }

    finish_replay(&fixture, host, &command, &before, &response).await;
}

#[tokio::test]
async fn hardening_recall_receipt_checkpoint_retry_survives_reopen() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let before = observe(&host).await;
    let command = fixture.completion("receipt-checkpoint", "alpha", 9);
    let blocked = BlockedFile::new(fixture.config.state_dir.join("recall-policy-receipts.json"));
    let first = process_text(&host.state, &command.to_string()).await;
    let first_kind = outcome_event(&first).meta.command_or_event_type;
    assert!(matches!(
        first_kind.as_str(),
        RECALL_POLICY_COMMAND_FAILED | RECALL_POLICY_COMMAND_ACCEPTED
    ));
    drop(blocked);

    let retry = process_text(&host.state, &command.to_string()).await;
    let accepted = outcome_event(&retry);
    assert_eq!(
        accepted.meta.command_or_event_type,
        RECALL_POLICY_COMMAND_ACCEPTED
    );
    assert_eq!(accepted.version, before.cursor.version + 1);
    let committed = observe(&host).await;
    drop(host);

    let reopened = fixture.open();
    let replay = process_text(&reopened.state, &command.to_string()).await;
    assert_eq!(
        replay.direct, retry.direct,
        "an accepted retry must not exist only in the old process's receipt cache"
    );
    assert!(replay.delta.is_none());
    assert_eq!(
        observe(&reopened).await,
        committed,
        "reopen and replay must not commit the command twice"
    );
    assert_eq!(committed.projection.working_set_entries, 9);
    assert_eq!(committed.sessions["beta"], before.sessions["beta"]);
    fixture.assert_room_fields(&reopened);
}

#[tokio::test]
async fn hardening_recall_failed_receipt_save_does_not_change_the_cache() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let mut runtime = host.state.runtime.lock().await;
    let incumbent = runtime.durable.receipt("seed-alpha").unwrap().clone();
    let mut candidate = incumbent.clone();
    candidate.idempotency_key = "unwritten-receipt".into();
    candidate.outcome.meta.idempotency_key = candidate.idempotency_key.clone();
    let blocked = BlockedFile::new(fixture.config.state_dir.join("recall-policy-receipts.json"));
    let result = runtime.durable.save_receipt(candidate.clone());
    let leaked = runtime.durable.receipt(&candidate.idempotency_key).cloned();
    let retained = runtime
        .durable
        .receipt(&incumbent.idempotency_key)
        .unwrap()
        .clone();
    drop(blocked);

    assert!(
        result.is_err(),
        "the real atomic replacement must reject a directory target"
    );
    assert_eq!(
        retained.outcome, incumbent.outcome,
        "a failed save must preserve an existing receipt"
    );
    assert!(
        leaked.is_none(),
        "save_receipt returned Err but exposed the unpersisted receipt"
    );

    runtime.durable.save_receipt(candidate.clone()).unwrap();
    drop(runtime);
    drop(host);
    let reopened = fixture.open();
    let runtime = reopened.state.runtime.lock().await;
    assert_eq!(
        runtime
            .durable
            .receipt(&candidate.idempotency_key)
            .unwrap()
            .outcome,
        candidate.outcome
    );
    assert_eq!(
        runtime
            .durable
            .receipt(&incumbent.idempotency_key)
            .unwrap()
            .outcome,
        incumbent.outcome
    );
}

#[tokio::test]
async fn hardening_recall_failed_judgment_save_does_not_change_the_next_decision() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let before = observe(&host).await;
    let command = fixture.command(
        protocol::RECALL_POLICY_JUDGED_MODE,
        "judge-work",
        "alpha",
        json!({"judged_mode": {"mode": "work", "source": protocol::RECALL_POLICY_JUDGED_MODE_SOURCE, "revision": "fixture"}}),
    );
    let blocked = BlockedFile::new(fixture.config.state_dir.join("recall-policy-sessions.json"));
    let response = process_text(&host.state, &command.to_string()).await;
    let event = outcome_event(&response);
    let after = observe(&host).await;
    drop(blocked);

    if event.meta.command_or_event_type == RECALL_POLICY_COMMAND_FAILED {
        assert_eq!(
            after.sessions, before.sessions,
            "an unpersisted judgment must not influence the next turn"
        );
    } else {
        assert_eq!(event.meta.command_or_event_type, RECALL_POLICY_SNAPSHOT);
    }

    drop(host);
    let reopened = fixture.open();
    assert_eq!(observe(&reopened).await, after);
    let retry = process_text(&reopened.state, &command.to_string()).await;
    assert_eq!(
        outcome_event(&retry).meta.command_or_event_type,
        RECALL_POLICY_SNAPSHOT
    );
    let judged = observe(&reopened).await;
    assert_ne!(judged.sessions["alpha"], before.sessions["alpha"]);
    assert_eq!(judged.sessions["beta"], before.sessions["beta"]);
    drop(reopened);

    let reopened = fixture.open();
    assert_eq!(observe(&reopened).await, judged);
    let evaluate = fixture.command(
        protocol::RECALL_POLICY_EVALUATE,
        "consume-judgment",
        "alpha",
        json!({"facts": {"query_route": {"intent": "general"}, "working_set_present": true}}),
    );
    let response = process_text(&reopened.state, &evaluate.to_string()).await;
    let accepted = outcome_event(&response);
    assert_eq!(
        accepted.meta.command_or_event_type,
        RECALL_POLICY_COMMAND_ACCEPTED
    );
    assert_eq!(
        accepted.state.unwrap().resolved_mode,
        protocol::RecallResolvedMode::Work
    );
    fixture.assert_room_fields(&reopened);
}

#[tokio::test]
async fn hardening_recall_unavailable_commit_record_preserves_live_and_durable_state() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let before = observe(&host).await;
    let files_before = fixture.checkpoint_bytes();
    let command = fixture.completion("commit-record-blocked", "alpha", 9);
    let commit_path = fixture.config.state_dir.join("recall-policy-commit.json");
    fs::create_dir(&commit_path).unwrap();

    let response = process_text(&host.state, &command.to_string()).await;
    fs::remove_dir(&commit_path).unwrap();

    assert_eq!(
        outcome_event(&response).meta.command_or_event_type,
        RECALL_POLICY_COMMAND_FAILED
    );
    assert!(response.delta.is_none());
    assert_eq!(
        observe(&host).await,
        before,
        "no live state may precede the commit record"
    );
    assert_eq!(
        fixture.checkpoint_bytes(),
        files_before,
        "no checkpoint may precede the commit record"
    );
    finish_replay(&fixture, host, &command, &before, &response).await;
}

#[tokio::test]
async fn hardening_recall_interrupted_materialization_recovers_before_replay() {
    for file in [
        "recall-policy-sessions.json",
        "recall-policy-cursor.json",
        "recall-policy-receipts.json",
    ] {
        let mut fixture = Fixture::new();
        let host = fixture.open();
        fixture.seed(&host).await;
        let before = observe(&host).await;
        let command = fixture.completion("recover-commit", "alpha", 9);
        let blocked = BlockedFile::new(fixture.config.state_dir.join(file));
        let response = process_text(&host.state, &command.to_string()).await;
        let accepted = outcome_event(&response);
        let committed = observe(&host).await;
        let commit_path = fixture.config.state_dir.join("recall-policy-commit.json");
        let pending = fs::read(&commit_path)
            .expect("keep the authoritative commit until every checkpoint lands");

        assert_eq!(
            accepted.meta.command_or_event_type,
            RECALL_POLICY_COMMAND_ACCEPTED
        );
        assert!(response.delta.is_some());
        assert_eq!(committed.projection.working_set_entries, 9);
        assert_eq!(committed.sessions["beta"], before.sessions["beta"]);
        assert_eq!(committed.cursor.version, before.cursor.version + 1);

        let incompatible = fixture.completion("different-commit", "alpha", 11);
        let refused = process_text(&host.state, &incompatible.to_string()).await;
        assert_eq!(
            outcome_event(&refused).meta.command_or_event_type,
            RECALL_POLICY_COMMAND_FAILED
        );
        assert!(refused.delta.is_none());
        assert_eq!(observe(&host).await, committed);
        assert_eq!(
            fs::read(&commit_path).unwrap(),
            pending,
            "a new command must not replace unfinished recovery"
        );

        let replay = process_text(&host.state, &command.to_string()).await;
        assert_eq!(replay.direct, response.direct);
        assert!(replay.delta.is_none());

        let mut root = host.state.room_store.read_root().unwrap();
        root["unrelated"]["later"] = json!("written after the policy commit");
        root["recallPolicy"]["extension"]["later"] = json!(true);
        host.state.room_store.write_room_state(&root).unwrap();
        fixture.room_root = root;
        drop(host);

        assert!(
            fixture.try_open().is_err(),
            "startup must not serve a partial checkpoint"
        );
        assert_eq!(fs::read(&commit_path).unwrap(), pending);
        drop(blocked);

        let reopened = fixture.open();
        assert_eq!(
            observe(&reopened).await,
            committed,
            "recovery must precede projection and session loading"
        );
        assert_eq!(
            reopened.state.room_store.load().unwrap(),
            committed.projection
        );
        assert!(
            !commit_path.exists(),
            "retire the record only after complete materialization"
        );
        fixture.assert_room_fields(&reopened);
        let replay = process_text(&reopened.state, &command.to_string()).await;
        assert_eq!(
            replay.direct, response.direct,
            "recovery must keep the original accepted event identity"
        );
        assert!(replay.delta.is_none());
        assert_eq!(observe(&reopened).await, committed);

        let mut changed_body = command.clone();
        changed_body["refresh"]["entries"] = json!(12);
        let conflict = process_text(&reopened.state, &changed_body.to_string()).await;
        assert_eq!(
            outcome_event(&conflict).meta.command_or_event_type,
            RECALL_POLICY_COMMAND_REFUSED
        );
        assert!(conflict.delta.is_none());
        assert_eq!(
            observe(&reopened).await,
            committed,
            "a recovered retry key cannot authorize another body"
        );
    }
}

#[tokio::test]
async fn hardening_recall_corrupt_recovery_record_refuses_without_checkpoint_writes() {
    let fixture = Fixture::new();
    let host = fixture.open();
    fixture.seed(&host).await;
    let command = fixture.completion("corrupt-recovery", "alpha", 9);
    let blocked = BlockedFile::new(fixture.config.state_dir.join("recall-policy-sessions.json"));
    let response = process_text(&host.state, &command.to_string()).await;
    assert_eq!(
        outcome_event(&response).meta.command_or_event_type,
        RECALL_POLICY_COMMAND_ACCEPTED
    );
    let committed = observe(&host).await;
    let commit_path = fixture.config.state_dir.join("recall-policy-commit.json");
    let valid = fs::read(&commit_path).unwrap();
    drop(host);
    drop(blocked);
    let checkpoints = fixture.checkpoint_bytes();

    let parsed: Value = serde_json::from_slice(&valid).unwrap();
    let mut foreign = parsed.clone();
    foreign["room"] = json!("another-room");
    let mut inconsistent = parsed.clone();
    inconsistent["checkpoint"]["cursor"]["state_hash"] = json!("not-the-projection-hash");
    let mut unknown = parsed;
    unknown["unexpected"] = json!(true);

    for corrupt in [
        b"{".to_vec(),
        serde_json::to_vec(&foreign).unwrap(),
        serde_json::to_vec(&inconsistent).unwrap(),
        serde_json::to_vec(&unknown).unwrap(),
    ] {
        fs::write(&commit_path, &corrupt).unwrap();
        assert!(
            fixture.try_open().is_err(),
            "a corrupt recovery record must not fall back to partial files"
        );
        assert_eq!(fixture.checkpoint_bytes(), checkpoints);
        assert_eq!(
            fs::read(&commit_path).unwrap(),
            corrupt,
            "a refusal must preserve the recovery evidence"
        );
    }

    fs::write(&commit_path, valid).unwrap();
    let reopened = fixture.open();
    assert_eq!(observe(&reopened).await, committed);
    let replay = process_text(&reopened.state, &command.to_string()).await;
    assert_eq!(replay.direct, response.direct);
    assert!(replay.delta.is_none());
    fixture.assert_room_fields(&reopened);
}
