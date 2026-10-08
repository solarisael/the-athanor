use super::*;
use crate::config::KnockAutonomy;

struct Fixture {
    root: std::path::PathBuf,
    state: AppState,
    meta: CommandMeta,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("athanor-context-{}", new_id()));
        let config = HostConfig {
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
            knock_autonomy: KnockAutonomy::Off,
        };
        crate::store::atomic_json_write(&config.room_state_path(), &json!({
            "room":"test-room","operator":"Operator","agentName":"Test","embodiedSpirit":"Test",
            "recallPolicy":{"requestedMode":"auto","resolvedMode":"conversation","activeProject":null,"resolutionReason":"default",
                "lastRefreshReason":null,"lastRefreshAt":null,"workingSetEntries":0,"recoveryPending":false,"recoveryTerms":[],"degraded":null,"updatedAt":null}
        })).unwrap();
        let state = Host::new(config, None, CancellationToken::new(), TaskTracker::new())
            .unwrap()
            .state;
        let meta = CommandMeta {
            schema_version: 1,
            message_id: "request".into(),
            house_id: "test-house".into(),
            sender_room: "test-room".into(),
            sender_spirit: "Test".into(),
            sender_session: "caller-session".into(),
            recipient: "house-host".into(),
            correlation_id: "request".into(),
            causation_id: String::new(),
            reply_target: "caller-session".into(),
            idempotency_key: "request".into(),
            source_record_refs: vec![],
            scope: "room:test-room:recall_policy".into(),
            visibility: "operator".into(),
            authority_class: "room_state".into(),
            created_at: timestamp(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            max_hops: 1,
            projection_id: "context".into(),
        };
        Self { root, state, meta }
    }

    async fn prove_frame(&self) {
        let authentication = authenticate_presence(&self.state, &self.meta).unwrap();
        let body = "Current authenticated identity";
        let material = materials::material(
            "identity:active-spirit".into(),
            json!({"kind":"identity","source":"active_spirit.md","sha256":format!("{:x}",Sha256::digest(body.as_bytes()))}),
            "identity",
            body.into(),
            1000,
        );
        let request = serde_json::from_value(
            json!({"binding":authentication.binding,"identity":[material],"previousBoat":null}),
        )
        .unwrap();
        let ledger = PresenceLedger {
            repair_rule_ids: vec!["rule:prior".into()],
            ..PresenceLedger::default()
        };
        self.state
            .runtime
            .lock()
            .await
            .presence
            .open_carrying(&authentication, "proven-frame", request, Some(ledger))
            .unwrap();
    }

    async fn request(&self, turn: &str) -> Value {
        self.request_for(
            turn,
            Capabilities {
                automatic_recall: false,
                top_level: false,
            },
            false,
        )
        .await
    }

    async fn request_for(
        &self,
        turn: &str,
        capabilities: Capabilities,
        native_user: bool,
    ) -> Value {
        let response = lesson_plan(&self.state,self.meta.clone(),json!({
            "turnId":turn,"activeProject":null,"managerAvailable":false,"deadline":now_ms()+CONTEXT_BUDGET_MS,
            "nativeUser":native_user,"capabilities":capabilities
        })).await;
        let envelope: Value = serde_json::from_str(&response.direct[0]).unwrap();
        let token = envelope["result"]["token"]
            .as_str()
            .expect("real native lesson plan token");
        json!({"turnId":turn,"prompt":"hello","nativeUser":native_user,"visibleTurnIds":[turn],
            "history":[{"kind":"user","text":"hello","turnId":turn}],"existingBlockKinds":[],
            "contextCharacters":5,"userTurnOrdinal":1,"freshConversation":false,"activeProject":null,
            "toolEvidenceRevision":0,"toolEvidenceEpoch":"test-epoch","capabilities":capabilities,
            "planToken":token,"credential":null,"legacyMemo":null,"priorPresence":null})
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.state.cancellation.cancel();
        std::fs::remove_dir_all(&self.root).expect("remove only this test's state");
    }
}

fn legacy(turn: &str, body: &str) -> Value {
    json!({"version":1,"turns":[{"turnId":turn,"blocks":[{"kind":"recall-context","content":body,"details":null,"timestamp":1}]}]})
}

#[tokio::test]
async fn adoption_replays_exact_bytes_and_never_replaces_newer_native_state() {
    let fixture = Fixture::new();
    fixture.prove_frame().await;
    let mut request = fixture.request("first").await;
    request["legacyMemo"] = legacy("first", "original bytes\n");
    let first = prepare_inner(&fixture.state, &fixture.meta, request.clone())
        .await
        .unwrap();
    assert_eq!(first["replayed"], true);
    assert_eq!(
        first["turns"][0]["blocks"][0]["content"],
        "original bytes\n"
    );
    *fixture.state.context_sessions.lock().await = ContextSessions::default();
    request = fixture.request("first").await;
    request["legacyMemo"] = legacy("first", "poisoned replacement");
    let second = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(second["turns"], first["turns"]);
}

#[tokio::test]
async fn adoption_refuses_unknown_blocks_invisible_turns_and_foreign_plan_tokens() {
    let fixture = Fixture::new();
    let mut request = fixture.request("first").await;
    request["legacyMemo"] = legacy("invisible", "not admitted");
    assert!(
        prepare_inner(&fixture.state, &fixture.meta, request.clone())
            .await
            .is_err()
    );
    request["legacyMemo"] = legacy("first", "not admitted");
    request["legacyMemo"]["turns"][0]["blocks"][0]["kind"] = json!("tool-call");
    assert!(
        prepare_inner(&fixture.state, &fixture.meta, request.clone())
            .await
            .is_err()
    );
    request["legacyMemo"] = Value::Null;
    let mut foreign = fixture.meta.clone();
    foreign.sender_session = "another-session".into();
    assert!(
        prepare_inner(&fixture.state, &foreign, request)
            .await
            .is_err()
    );
    assert!(!cache::file(&fixture.state, &fixture.meta.sender_session).exists());
}

#[tokio::test]
async fn replay_requires_a_fresh_lesson_plan_and_compaction_removes_only_recall() {
    let fixture = Fixture::new();
    fixture.prove_frame().await;
    let mut request = fixture.request("first").await;
    request["legacyMemo"] = legacy("first", "recall");
    let mut room_block = request["legacyMemo"]["turns"][0]["blocks"][0].clone();
    room_block["kind"] = json!("room-context");
    room_block["content"] = json!("identity");
    request["legacyMemo"]["turns"][0]["blocks"]
        .as_array_mut()
        .unwrap()
        .push(room_block);
    prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let request = fixture.request("first").await;
    let plan_token = text(&request["planToken"]);
    assert!(
        session(&fixture.state, &fixture.meta.sender_session)
            .await
            .plans
            .contains_key(plan_token)
    );
    let mut context = session(&fixture.state, &fixture.meta.sender_session).await;
    invalidate(&fixture.state, &fixture.meta.sender_session, &mut context).unwrap();
    drop(context);
    let response = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(response["turns"][0]["blocks"].as_array().unwrap().len(), 1);
    assert_eq!(response["turns"][0]["blocks"][0]["kind"], "room-context");
}

#[tokio::test]
async fn expired_assembly_and_cancelled_wait_do_not_publish_a_turn() {
    let fixture = Fixture::new();
    let request = fixture.request("first").await;
    let token = text(&request["planToken"]).to_owned();
    session(&fixture.state, &fixture.meta.sender_session)
        .await
        .plans
        .get_mut(&token)
        .unwrap()
        .deadline = 0;
    assert!(
        prepare_inner(&fixture.state, &fixture.meta, request)
            .await
            .is_err()
    );
    assert!(!cache::file(&fixture.state, &fixture.meta.sender_session).exists());
    let request = fixture.request("second").await;
    let guard = session(&fixture.state, &fixture.meta.sender_session).await;
    let state = fixture.state.clone();
    let meta = fixture.meta.clone();
    let task = tokio::spawn(async move { prepare_inner(&state, &meta, request).await });
    tokio::task::yield_now().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    drop(guard);
    assert!(!cache::file(&fixture.state, &fixture.meta.sender_session).exists());
}

#[tokio::test]
async fn visible_turn_pruning_preserves_current_bytes_and_drops_only_absent_turns() {
    let fixture = Fixture::new();
    fixture.prove_frame().await;
    let mut request = fixture.request("second").await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let mut memo = legacy("first", "old");
    memo["turns"]
        .as_array_mut()
        .unwrap()
        .push(legacy("second", "current")["turns"][0].clone());
    request["legacyMemo"] = memo;
    prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let request = fixture.request("second").await;
    let response = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(response["turns"].as_array().unwrap().len(), 1);
    assert_eq!(response["turns"][0]["blocks"][0]["content"], "current");
}

#[tokio::test]
async fn disabled_lesson_grant_preserves_baseline_without_provider_io() {
    let fixture = Fixture::new();
    let request: ContextPrepareRequest =
        serde_json::from_value(fixture.request("first").await).unwrap();
    let plan = lessons::LessonPlan {
        token: "token".into(),
        turn_id: "first".into(),
        rules: vec![],
        lessons: vec![],
        baseline: vec![lessons::Lesson {
            id: 1,
            body: "a standing rule".into(),
        }],
        warnings: vec![],
        deadline: now_ms() + CONTEXT_BUDGET_MS,
        requires_credential: false,
    };
    let mut cache = lessons::SieveCache::default();
    let (kept_ids, receipt) = lessons::sieve_once(
        &mut cache,
        &fixture.state,
        &request,
        &plan,
        CancellationToken::new(),
    )
    .await;
    assert!(kept_ids.is_none());
    assert_eq!(receipt["status"], "disabled");
    assert!(judge_deadline(now_ms() + COMMIT_RESERVE_MS).is_none());
    assert!(judge_deadline(now_ms() + COMMIT_RESERVE_MS + 500).is_some());
}

#[test]
fn work_evidence_ages_once_per_ordinal_and_resets_with_the_harness_epoch() {
    let mut evidence = cache::WorkEvidence::default();
    assert!(!evidence.observe("first", 0, 1));
    assert!(evidence.observe("first", 1, 1));
    let last_kept = cache::WORK_DECAY_TURNS;
    let expired = last_kept + 1;
    assert!(evidence.observe("first", 1, last_kept));
    assert!(evidence.observe("first", 1, last_kept));
    assert!(!evidence.observe("first", 1, expired));
    assert!(!evidence.observe("second", 0, expired));
}

#[test]
fn utf16_ceiling_never_splits_an_astral_character() {
    let astral = char::from_u32(0x1f30d).unwrap();
    assert_eq!(clip(&format!("a{astral}b"), 2), "a");
    assert_eq!(clip(&format!("a{astral}b"), 3), format!("a{astral}"));
}

#[test]
fn active_empty_rerank_removes_raw_lanes_but_failure_preserves_them() {
    let raw = json!({"found":true,"retrievalCandidates":[{"id":1}],"rerankCandidates":[{"id":2}],
        "semanticChunks":[{"body":"semantic"}],"contentChunks":[{"body":"lexical"}],"canonMatches":[],"dateMatches":[]});
    let active = recall::prepare_viewport(raw.clone(), &json!({"retrievalCandidates":[]}), true);
    assert_eq!(active["found"], false);
    assert_eq!(active["semanticChunks"], json!([]));
    assert_eq!(active["contentChunks"], json!([]));
    assert!(active.get("rerankCandidates").is_none());
    let baseline = recall::prepare_viewport(
        raw.clone(),
        &json!({"retrievalCandidates":[{"id":1}]}),
        false,
    );
    assert_eq!(baseline["found"], true);
    assert_eq!(baseline["semanticChunks"], raw["semanticChunks"]);
    assert!(baseline.get("rerankCandidates").is_none());
}

#[test]
fn baseline_lessons_keep_order_and_share_one_presence_directive() {
    let baseline: Vec<lessons::Lesson> = (1..=10)
        .map(|id| lessons::Lesson {
            id,
            body: format!("Rule {id}"),
        })
        .collect();
    let triggers = vec![
        lessons::Lesson {
            id: 1,
            body: "duplicate".into(),
        },
        lessons::Lesson {
            id: 11,
            body: "trigger".into(),
        },
    ];
    let materials = materials::lessons(&baseline, &triggers, "work", None);
    assert_eq!(materials.len(), 11);
    assert_eq!(materials[0]["body"], "Rule 1");
    let directives = materials::directives("Test", "Operator", &materials);
    assert!(
        directives
            .iter()
            .any(|directive| directive["instruction"] == "Rule 10")
    );
    assert_eq!(
        directives
            .iter()
            .filter(|directive| directive["sourceIds"] == json!(["lesson:1"]))
            .count(),
        1
    );
    for mode in ["conversation", "quiet", "mixed"] {
        assert_eq!(
            materials::lessons(&baseline, &triggers, mode, None).len(),
            2
        );
    }
    let no_winners = std::collections::HashSet::new();
    assert_eq!(
        materials::lessons(&baseline, &triggers, "work", Some(&no_winners)).len(),
        2
    );
}

#[tokio::test]
async fn a_sieve_retry_keeps_the_first_decision_after_policy_changes() {
    let fixture = Fixture::new();
    let request: ContextPrepareRequest =
        serde_json::from_value(fixture.request("first").await).unwrap();
    let plan = lessons::LessonPlan {
        token: "token".into(),
        turn_id: "first".into(),
        rules: vec![],
        lessons: vec![],
        baseline: vec![lessons::Lesson {
            id: 1,
            body: "Rule".into(),
        }],
        warnings: vec![],
        deadline: now_ms() + CONTEXT_BUDGET_MS,
        requires_credential: false,
    };
    let mut cache = lessons::SieveCache::default();
    let first = lessons::sieve_once(
        &mut cache,
        &fixture.state,
        &request,
        &plan,
        CancellationToken::new(),
    )
    .await;
    crate::store::atomic_json_write(&fixture.state.config.room_dir.join(".athanor-room.json"),
        &json!({"jevLessons":{"mode":"active","provider":"typesafe","grant":{"purpose":"lesson-sieve","allowPrivateLessonPackets":true,"policyRevision":"test"}}})).unwrap();
    let retry = lessons::sieve_once(
        &mut cache,
        &fixture.state,
        &request,
        &plan,
        CancellationToken::new(),
    )
    .await;
    assert_eq!(retry.1, first.1);
    assert_eq!(coverage(&fixture.state).await["lessons"]["turns"], 1);
}

#[tokio::test]
async fn previous_turn_uses_the_last_assistant_before_this_native_turn() {
    let fixture = Fixture::new();
    let mut raw = fixture.request("current").await;
    raw["history"] = json!([
        {"kind":"user","text":"previous request","turnId":"previous"},
        {"kind":"assistant","text":"earlier draft","turnId":null},
        {"kind":"assistant","text":"final answer","turnId":null},
        {"kind":"user","text":"current reply","turnId":"current"}
    ]);
    let request: ContextPrepareRequest = serde_json::from_value(raw.clone()).unwrap();
    let previous = assembly::previous(&request, &SavedSession::default()).unwrap();
    assert_eq!(previous.0, "previous request");
    assert_eq!(previous.1, "final answer");
    raw["history"] = json!([{"kind":"user","text":"current reply","turnId":"current"}]);
    assert!(
        assembly::previous(
            &serde_json::from_value(raw).unwrap(),
            &SavedSession::default()
        )
        .is_none()
    );
}

#[tokio::test]
async fn unverified_legacy_and_rejected_unicode_rebuild_without_poisoning_the_next_turn() {
    let fixture = Fixture::new();
    let mut request = fixture.request("first").await;
    request["legacyMemo"] = legacy("first", "unverified old authority");
    let rebuilt = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(rebuilt["adoption"], "rebuilt");
    assert_eq!(
        rebuilt["invalidationReason"],
        "legacy-context-identity-unverified"
    );
    assert!(
        !rebuilt["turns"]
            .to_string()
            .contains("unverified old authority")
    );

    let unicode = Fixture::new();
    let mut request = unicode.request("first").await;
    request["legacyRejection"] = json!("unpaired-surrogate");
    let rebuilt = prepare_inner(&unicode.state, &unicode.meta, request)
        .await
        .unwrap();
    assert_eq!(rebuilt["adoptionRejection"], "unpaired-surrogate");
    assert_eq!(rebuilt["adoption"], "rebuilt");
    let retry = unicode.request("first").await;
    let replay = prepare_inner(&unicode.state, &unicode.meta, retry)
        .await
        .unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["turns"], rebuilt["turns"]);
}

#[tokio::test]
async fn embodiment_change_rotates_the_frame_and_retires_the_old_contract() {
    let mut fixture = Fixture::new();
    fixture.prove_frame().await;
    let request = fixture
        .request_for(
            "first",
            Capabilities {
                automatic_recall: false,
                top_level: true,
            },
            true,
        )
        .await;
    let first = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let old_contract = text(&first["presenceSettlement"]["contractId"]).to_owned();
    assert!(!old_contract.is_empty());
    let old_frame = fixture
        .state
        .runtime
        .lock()
        .await
        .presence
        .session_state(&fixture.meta.sender_session)
        .unwrap()
        .0
        .frame_id
        .clone();
    let patch = serde_json::from_value(json!({"action":"patch","embodiedSpirit":"Other"})).unwrap();
    let room = crate::room_state::execute(
        &fixture.state.room_store,
        &fixture.state.config.room_dir,
        &fixture.state.config.spirit,
        patch,
    )
    .unwrap();
    assert_eq!(room["agentName"], "Test");
    fixture.meta.sender_spirit = "Other".into();
    let request = fixture
        .request_for(
            "first",
            Capabilities {
                automatic_recall: false,
                top_level: true,
            },
            true,
        )
        .await;
    let changed = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(changed["invalidationReason"], "identity-epoch-changed");
    assert_eq!(changed["replayed"], false);
    assert_ne!(changed["presenceSettlement"]["contractId"], old_contract);
    let mut runtime = fixture.state.runtime.lock().await;
    let (frame, ledger) = runtime
        .presence
        .session_state(&fixture.meta.sender_session)
        .unwrap();
    assert_eq!(frame.binding.spirit, "Other");
    assert_ne!(frame.frame_id, old_frame);
    assert!(ledger.repair_rule_ids.contains(&"rule:prior".to_owned()));
    let settle = serde_json::from_value(
        json!({"contractId":old_contract,"attempt":1,"evaluatedDirectives":[],"violations":[],
        "decision":"accept","responseDigest":format!("{:x}",Sha256::digest(b"answer"))}),
    )
    .unwrap();
    assert!(
        runtime
            .presence
            .settle(&fixture.meta.sender_session, "old-contract", settle)
            .is_err()
    );
}

#[tokio::test]
async fn lesson_plan_requests_no_typesafe_credential_for_disabled_or_laya_only_grants() {
    let fixture = Fixture::new();
    fixture
        .request_for(
            "disabled",
            Capabilities {
                automatic_recall: true,
                top_level: true,
            },
            true,
        )
        .await;
    {
        let context = session(&fixture.state, &fixture.meta.sender_session).await;
        assert!(
            !context
                .plans
                .values()
                .find(|plan| plan.turn_id == "disabled")
                .unwrap()
                .requires_credential
        );
    }
    let marker = fixture.state.config.room_dir.join(".athanor-room.json");
    crate::store::atomic_json_write(&marker,&json!({"room":"test-room","jevMode":{"mode":"active","provider":"laya",
        "endpoint":"http://127.0.0.1:8790/v1/systemone","grant":{"purpose":"recall-mode","allowPrivateConversationPackets":true,"policyRevision":"mode-r1"}}})).unwrap();
    fixture
        .request_for(
            "laya",
            Capabilities {
                automatic_recall: true,
                top_level: true,
            },
            true,
        )
        .await;
    {
        let context = session(&fixture.state, &fixture.meta.sender_session).await;
        assert!(
            !context
                .plans
                .values()
                .find(|plan| plan.turn_id == "laya")
                .unwrap()
                .requires_credential
        );
    }
    crate::store::atomic_json_write(&marker,&json!({"room":"test-room","jevLessons":{"mode":"active","provider":"typesafe",
        "grant":{"purpose":"lesson-sieve","allowPrivateLessonPackets":true,"policyRevision":"test-r1"}}})).unwrap();
    fixture
        .request_for(
            "typesafe",
            Capabilities {
                automatic_recall: true,
                top_level: true,
            },
            true,
        )
        .await;
    {
        let context = session(&fixture.state, &fixture.meta.sender_session).await;
        assert!(
            context
                .plans
                .values()
                .find(|plan| plan.turn_id == "typesafe")
                .unwrap()
                .requires_credential
        );
    }
    fixture.request("child").await;
    let context = session(&fixture.state, &fixture.meta.sender_session).await;
    assert!(
        !context
            .plans
            .values()
            .find(|plan| plan.turn_id == "child")
            .unwrap()
            .requires_credential
    );
}

#[test]
fn canon_presence_material_keeps_the_native_numeric_identity() {
    let materials =
        materials::recalled(&json!({"canonMatches":[{"id":42,"summary":"Authoritative fact"}]}));
    assert_eq!(materials[0]["id"], "canon:42");
    assert_eq!(materials[0]["authority"]["entity_id"], "42");
}

#[tokio::test]
async fn lost_native_presence_invalidates_replay_and_credential_gating_uses_the_same_rule() {
    let fixture = Fixture::new();
    let capabilities = Capabilities {
        automatic_recall: false,
        top_level: true,
    };
    let request = fixture.request_for("first", capabilities, true).await;
    let first = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert!(first["presenceSettlement"]["contractId"].is_string());
    let old_generation = session(&fixture.state, &fixture.meta.sender_session)
        .await
        .saved
        .as_ref()
        .unwrap()
        .generation
        .clone();
    crate::store::atomic_json_write(&fixture.state.config.room_dir.join(".athanor-room.json"),
        &json!({"room":"test-room","jevLessons":{"mode":"active","provider":"typesafe",
            "grant":{"purpose":"lesson-sieve","allowPrivateLessonPackets":true,"policyRevision":"test-r1"}}})).unwrap();
    fixture.request_for("first", capabilities, true).await;
    {
        let context = session(&fixture.state, &fixture.meta.sender_session).await;
        assert!(
            context
                .plans
                .values()
                .filter(|plan| plan.turn_id == "first")
                .all(|plan| !plan.requires_credential)
        );
    }
    *fixture.state.context_sessions.lock().await = ContextSessions::default();
    fixture.state.runtime.lock().await.presence = PresenceRuntime::default();
    let request = fixture.request_for("first", capabilities, true).await;
    {
        let context = session(&fixture.state, &fixture.meta.sender_session).await;
        assert!(context.plans.values().any(|plan| plan.requires_credential));
    }
    let rebuilt = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(rebuilt["replayed"], false);
    assert_eq!(rebuilt["invalidationReason"], "presence-frame-unavailable");
    assert!(
        fixture
            .state
            .runtime
            .lock()
            .await
            .presence
            .has_current_contract(
                &fixture.meta.sender_session,
                text(&rebuilt["presenceSettlement"]["contractId"])
            )
    );
    let context = session(&fixture.state, &fixture.meta.sender_session).await;
    let invalidation = context
        .saved
        .as_ref()
        .unwrap()
        .last_invalidation
        .as_ref()
        .unwrap();
    assert_eq!(invalidation.reason, "presence-frame-unavailable");
    assert_eq!(invalidation.prior_generation, old_generation);
    assert_ne!(context.saved.as_ref().unwrap().generation, old_generation);
    assert_eq!(invalidation.prior_content_sha256.len(), 64);
}

#[tokio::test]
async fn native_recall_export_is_opt_in_and_preserves_the_record_contract() {
    let fixture = Fixture::new();
    let mut request: ContextPrepareRequest =
        serde_json::from_value(fixture.request("first").await).unwrap();
    request.prompt = "private prompt that must not be copied".into();
    let file = fixture
        .state
        .config
        .room_dir
        .join(".omp/runtime/recall-turns.jsonl");
    let observation = || telemetry::Observation {
        status: "skipped",
        route: None,
        viewport: Value::Null,
        diagnostics: Value::Null,
        error: None,
        query: None,
    };
    assert!(!telemetry::record(&fixture.state, &fixture.meta, &request, observation()).await);
    assert!(!file.exists());
    crate::store::atomic_json_write(
        &fixture.state.config.room_dir.join(".athanor-room.json"),
        &json!({"recallTelemetry":true}),
    )
    .unwrap();
    request.recall_telemetry_override = Some("0".into());
    assert!(!telemetry::record(&fixture.state, &fixture.meta, &request, observation()).await);
    request.recall_telemetry_override = Some("yes".into());
    let route = json!({"intent":"memory_lookup","terms":["private"],"shouldAutoRecall":true});
    let error = format!(
        "{} Bearer secret-token password=hunter2 http://user:pass@localhost/",
        request.prompt
    );
    assert!(
        telemetry::record(
            &fixture.state,
            &fixture.meta,
            &request,
            telemetry::Observation {
                status: "error",
                route: Some(&route),
                viewport: json!({"found":true}),
                diagnostics: telemetry::diagnostic(
                    &json!({"error":error}),
                    &request,
                    Some(&route),
                    None,
                    "request_parse",
                    None
                ),
                error: Some(&error),
                query: None,
            }
        )
        .await
    );
    let source = std::fs::read_to_string(file).unwrap();
    for secret in [&*request.prompt, "secret-token", "hunter2", "user:pass"] {
        assert!(!source.contains(secret));
    }
    let entry: Value = serde_json::from_str(source.trim()).unwrap();
    assert_eq!(entry["schema_version"], 1);
    assert_eq!(entry["session_id"], fixture.meta.sender_session);
    assert_eq!(entry["room"], fixture.state.config.room);
    assert_eq!(entry["prompt_chars"], request.prompt.encode_utf16().count());
    assert_eq!(entry["prompt_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(entry["route"]["term_count"], 1);
}

#[tokio::test]
async fn concurrent_native_exports_append_whole_records() {
    let fixture = Fixture::new();
    let mut request: ContextPrepareRequest =
        serde_json::from_value(fixture.request("first").await).unwrap();
    request.recall_telemetry_override = Some("on".into());
    let writes = (0..12).map(|_| {
        telemetry::record(
            &fixture.state,
            &fixture.meta,
            &request,
            telemetry::Observation {
                status: "skipped",
                route: None,
                viewport: Value::Null,
                diagnostics: Value::Null,
                error: None,
                query: None,
            },
        )
    });
    assert!(
        futures_util::future::join_all(writes)
            .await
            .into_iter()
            .all(|written| written)
    );
    let source = std::fs::read_to_string(
        fixture
            .state
            .config
            .room_dir
            .join(".omp/runtime/recall-turns.jsonl"),
    )
    .unwrap();
    let entries = source
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 12);
    assert!(entries.iter().all(|entry| entry["status"] == "skipped"));
}

#[tokio::test]
async fn replay_recovers_pending_settlement_without_rearming_an_acknowledged_contract() {
    let fixture = Fixture::new();
    let capabilities = Capabilities {
        automatic_recall: false,
        top_level: true,
    };
    let request = fixture.request_for("resumed", capabilities, true).await;
    let first = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let request = fixture.request_for("resumed", capabilities, true).await;
    let replay = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(replay["turns"], first["turns"]);
    assert_eq!(replay["presenceSettlement"], first["presenceSettlement"]);

    let contract = text(&first["presenceSettlement"]["contractId"]);
    let settlement = serde_json::from_value(json!({
        "contractId": contract, "attempt": 1,
        "evaluatedDirectives": first["presenceSettlement"]["directiveIds"],
        "violations": [], "decision": "accept",
        "responseDigest": format!("{:x}", Sha256::digest(b"completed response"))
    }))
    .unwrap();
    fixture
        .state
        .runtime
        .lock()
        .await
        .presence
        .settle(&fixture.meta.sender_session, "acknowledged", settlement)
        .unwrap();
    let request = fixture.request_for("resumed", capabilities, true).await;
    let acknowledged = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(acknowledged["turns"], first["turns"]);
    assert!(acknowledged["presenceSettlement"].is_null());
    assert_eq!(acknowledged["presenceSettledContractId"], contract);
}

#[tokio::test]
async fn compaction_before_adoption_preserves_legacy_wake_and_removes_stale_recall() {
    for evict in [false, true] {
        let fixture = Fixture::new();
        fixture.prove_frame().await;
        command(
            &fixture.state,
            ClientCommand::InvalidateAfterCompaction {
                meta: child_meta(
                    &fixture.meta,
                    "compaction",
                    "before-adoption",
                    "recall_policy",
                ),
                summary: "compacted history".into(),
            },
            json!({"summary":"compacted history"}),
        )
        .await
        .unwrap();
        assert!(!cache::file(&fixture.state, &fixture.meta.sender_session).exists());
        if evict {
            *fixture.state.context_sessions.lock().await = ContextSessions::default();
        }
        let mut request = fixture.request("retained").await;
        let mut memo = legacy("retained", "stale working set");
        memo["turns"][0]["blocks"].as_array_mut().unwrap().push(json!({
            "kind":"wake-context", "content":"retained wake letter\n", "details":null, "timestamp":1
        }));
        request["legacyMemo"] = memo;
        let adopted = prepare_inner(&fixture.state, &fixture.meta, request)
            .await
            .unwrap();
        let blocks = adopted["turns"][0]["blocks"].as_array().unwrap();
        assert_eq!(
            blocks
                .iter()
                .find(|block| block["kind"] == "wake-context")
                .unwrap()["content"],
            "retained wake letter\n"
        );
        assert!(!blocks.iter().any(|block| block["kind"] == "recall-context"));
    }
}
