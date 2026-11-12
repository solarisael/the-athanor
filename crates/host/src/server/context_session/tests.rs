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
            operator: None,
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
                top_level: true,
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

    async fn attach(&self, session: &str) {
        use tower::ServiceExt;

        std::fs::write(self.root.join("room/AGENTS.md"), "Isolated test home.\n").unwrap();
        let attachment = json!({
            "action":"attach","conversationId":session,
            "instanceId":uuid::Uuid::new_v4(),"workspace":self.root,
            "harness":"omp","sessionFile":self.root.join("session.jsonl"),
            "processId":std::process::id(),"controls":[]
        });
        let response = crate::server::portable::router(self.state.clone())
            .oneshot(
                axum::http::Request::post("/session")
                    .header("authorization", "Bearer test-token")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(attachment.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    async fn orihon_mode(&self, session: &str, params: Value) -> Value {
        let mut meta = self.meta.clone();
        meta.sender_session = session.to_owned();
        meta.reply_target = session.to_owned();
        let request = serde_json::from_value(json!({
            "operation":"orihon_mode","target_scope":"room","params":params
        }))
        .unwrap();
        let response = crate::server::native_commands::organ(&self.state, meta, request).await;
        let envelope: Value = serde_json::from_str(&response.direct[0]).unwrap();

        assert_eq!(envelope["result"]["ok"], true, "{envelope}");
        envelope["result"].clone()
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
async fn worker_context_discards_room_replay_and_legacy_continuity() {
    let fixture = Fixture::new();
    let kinds = [
        BlockKind::RoomContext,
        BlockKind::WakeContext,
        BlockKind::AnamnesisWake,
        BlockKind::HallwayBell,
        BlockKind::RecallContext,
        BlockKind::PresenceContext,
    ];
    let mut saved = SavedSession {
        turns: vec![cache::TurnBlocks {
            turn_id: "worker".into(),
            blocks: kinds
                .into_iter()
                .map(|kind| Block {
                    kind,
                    content: "Parent-only continuity".into(),
                    details: Value::Null,
                    timestamp: 1,
                })
                .collect(),
        }],
        woken: true,
        ..SavedSession::default()
    };
    assert!(cache::retire_recall(&mut saved));
    cache::commit(&fixture.state, &fixture.meta.sender_session, &saved).unwrap();
    let mut request = fixture
        .request_for(
            "worker",
            Capabilities {
                automatic_recall: false,
                top_level: false,
            },
            false,
        )
        .await;
    request["freshConversation"] = json!(true);
    request["legacyMemo"] = legacy("worker", "Another parent memory");
    request["priorPresence"] = json!({
        "frameId": "parent-frame",
        "frameRendered": "Parent identity"
    });
    let result = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(result["turns"], json!([]));
    assert!(result["presenceSettlement"].is_null());
    assert!(result["presenceSettledContractId"].is_null());
    let persisted = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert!(persisted.turns.is_empty());
    assert!(!persisted.woken);
    assert!(persisted.retired_dynamic_through.is_empty());

    // A cold resume must not restore the previous room wake from disk.
    *fixture.state.context_sessions.lock().await = ContextSessions::default();
    let request = fixture
        .request_for(
            "resumed",
            Capabilities {
                automatic_recall: false,
                top_level: false,
            },
            false,
        )
        .await;
    let resumed = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(resumed["turns"], json!([]));
}

#[tokio::test]
async fn adoption_retires_old_evidence_preserves_static_bytes_and_ignores_later_legacy() {
    let fixture = Fixture::new();
    fixture.prove_frame().await;
    let mut request = fixture.request("first").await;
    request["legacyMemo"] = legacy("first", "original bytes\n");
    let mut wake = request["legacyMemo"]["turns"][0]["blocks"][0].clone();
    wake["kind"] = json!("wake-context");
    wake["content"] = json!("original wake bytes\n");
    request["legacyMemo"]["turns"][0]["blocks"]
        .as_array_mut()
        .unwrap()
        .push(wake);
    let first = prepare_inner(&fixture.state, &fixture.meta, request.clone())
        .await
        .unwrap();
    assert_eq!(first["replayed"], false);
    assert_eq!(first["invalidationReason"], "context-format-changed");
    let blocks = first["turns"][0]["blocks"].as_array().unwrap();
    assert!(
        blocks
            .iter()
            .any(|block| block["content"] == "original wake bytes\n")
    );
    assert!(!blocks.iter().any(|block| block["kind"] == "recall-context"));
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
async fn replay_requires_a_fresh_lesson_plan_and_compaction_retires_only_dynamic_authority() {
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
    let first = prepare_inner(&fixture.state, &fixture.meta, request)
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
    assert_eq!(response["replayed"], true);
    assert_eq!(response["turns"], first["turns"]);
    assert!(response["presenceSettlement"].is_null());
    assert!(response["presenceSettledContractId"].is_null());
    let saved = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert!(!cache::has_kind(&saved, BlockKind::RecallContext));
    assert!(!cache::has_kind(&saved, BlockKind::PresenceContext));
    assert!(cache::has_kind(&saved, BlockKind::RoomContext));
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
    fixture
        .request_for(
            "child",
            Capabilities {
                automatic_recall: false,
                top_level: false,
            },
            false,
        )
        .await;
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

/// The owner answers `subagent` after every earlier command and publishes before each one.
async fn participants(live: &crate::organ::orihon::live::Room) -> Value {
    let _ = live.subagent().await;
    live.status()["participants"].clone()
}

#[tokio::test]
async fn fold_consent_is_explicit_and_ends_with_the_last_participant() {
    let fixture = Fixture::new();
    let live = fixture.state.orihon.clone();
    let owner = tokio::spawn(crate::organ::orihon::live::serve(
        live.clone(),
        (*fixture.state.config).clone(),
        None,
        registered_conversations(fixture.state.portable.clone()),
        fixture.state.cancellation.clone(),
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !live.status()["lastError"].is_null() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the live owner starts");
    fixture.attach("a").await;
    fixture.attach("b").await;

    let status = fixture.orihon_mode("a", json!({})).await;
    assert_eq!(
        status["instructions"],
        hearth::orihon::live::LIVE_INSTRUCTIONS
    );
    assert_eq!(
        status["instructionsVersion"],
        hearth::orihon::live::LIVE_VERSION
    );
    assert!(status.get("pageRecipe").is_none() && status.get("folderStatus").is_none());
    assert_eq!(status["liveStatus"]["participants"], 0);

    fixture
        .orihon_mode("a", json!({"mode":"fold","model":"proof:unused"}))
        .await;
    fixture.attach("c").await;
    // c starts in fold mode from the spirit default; reading its status is not consent.
    assert_eq!(fixture.orihon_mode("c", json!({})).await["mode"], "fold");
    assert_eq!(participants(&live).await, 1);

    fixture
        .orihon_mode("b", json!({"mode":"fold","model":"proof:unused"}))
        .await;
    assert_eq!(participants(&live).await, 2);
    // One fold session leaving keeps the other's compaction.
    fixture.orihon_mode("a", json!({"mode":"log"})).await;
    assert_eq!(participants(&live).await, 1);
    fixture.orihon_mode("b", json!({"mode":"off"})).await;
    assert_eq!(participants(&live).await, 0);

    let status = live.status();
    assert_eq!(status["running"], 0, "{status}");
    assert!(status["cacheGate"].is_null(), "no engine opened: {status}");
    assert!(
        fixture.state.tasks.is_empty(),
        "fold mode spawns no Host worker"
    );

    fixture.state.cancellation.cancel();
    owner
        .await
        .expect("the live owner releases the room before its directory is removed");
}

#[tokio::test]
async fn orihon_defaults_affect_new_sessions_without_changing_existing_sessions() {
    let fixture = Fixture::new();
    fixture.attach("a").await;
    fixture.orihon_mode("a", json!({"mode":"log"})).await;
    fixture.attach("b").await;

    fixture.orihon_mode("a", json!({"mode":"off"})).await;
    let b = fixture.orihon_mode("b", json!({})).await;

    assert_eq!(b["mode"], "log");
    assert_eq!(b["defaults"]["mode"], "off");

    fixture.attach("c").await;
    let c = fixture.orihon_mode("c", json!({})).await;

    assert_eq!(c["mode"], "off");
    assert_ne!(b["generation"], c["generation"]);

    fixture.orihon_mode("b", json!({"mode":"log"})).await;
    let a = fixture.orihon_mode("a", json!({})).await;
    let c_after = fixture.orihon_mode("c", json!({})).await;
    let b_after = fixture.orihon_mode("b", json!({})).await;

    assert_eq!(a["mode"], "off");
    assert_eq!(a["defaults"]["mode"], "log");
    assert_eq!(c_after["mode"], "off");
    assert_eq!(c_after["generation"], c["generation"]);
    assert_eq!(b_after["generation"], b["generation"]);

    fixture.attach("d").await;
    let d = fixture.orihon_mode("d", json!({})).await;

    assert_eq!(d["mode"], "log");
}

fn evidence_block(kind: BlockKind, content: &str) -> Block {
    Block {
        kind,
        content: content.into(),
        details: json!({"presentation":{"found":true,"retrievalCandidates":[
            {"memory_id":42,"excerpt":content}
        ]}}),
        timestamp: 1,
    }
}

#[test]
fn dynamic_snapshots_preserve_sent_blocks_and_use_only_current_evidence() {
    let mut saved = SavedSession::default();
    cache::merge(
        &mut saved,
        "first",
        vec![
            evidence_block(BlockKind::RoomContext, "static"),
            evidence_block(BlockKind::RecallContext, "first subject"),
            evidence_block(BlockKind::PresenceContext, "first contract"),
        ],
    );
    let first_bytes = serde_json::to_vec(&saved.turns[0]).unwrap();
    cache::merge(
        &mut saved,
        "second",
        vec![
            evidence_block(BlockKind::RoomContext, "replacement refused"),
            evidence_block(BlockKind::PresenceContext, "second contract"),
        ],
    );

    assert_eq!(
        cache::latest(&saved, BlockKind::RecallContext)
            .unwrap()
            .content,
        "first subject"
    );
    assert_eq!(serde_json::to_vec(&saved.turns[0]).unwrap(), first_bytes);
    assert_eq!(
        cache::latest(&saved, BlockKind::PresenceContext)
            .unwrap()
            .content,
        "second contract"
    );
    assert!(cache::latest_for_turn(&saved, "first", BlockKind::PresenceContext).is_none());

    cache::merge(
        &mut saved,
        "second",
        vec![
            evidence_block(BlockKind::RecallContext, "new subject"),
            evidence_block(BlockKind::PresenceContext, "new contract"),
        ],
    );

    assert_eq!(saved.turns.len(), 2);
    assert_eq!(serde_json::to_vec(&saved.turns[0]).unwrap(), first_bytes);
    assert_eq!(saved.turns[1].blocks.len(), 3);
    assert_eq!(
        cache::latest(&saved, BlockKind::RecallContext)
            .unwrap()
            .content,
        "new subject"
    );
    let sent_bytes = serde_json::to_vec(&saved.turns).unwrap();
    let second_bytes = serde_json::to_vec(&saved.turns[1].blocks).unwrap();
    assert!(cache::retire_recall(&mut saved));
    assert!(!cache::retire_recall(&mut saved));
    assert_eq!(serde_json::to_vec(&saved.turns).unwrap(), sent_bytes);
    assert!(!cache::has_kind(&saved, BlockKind::RecallContext));
    assert!(!cache::has_kind(&saved, BlockKind::PresenceContext));
    assert_eq!(
        cache::latest(&saved, BlockKind::RoomContext)
            .unwrap()
            .content,
        "static"
    );

    cache::merge(
        &mut saved,
        "second",
        vec![
            evidence_block(BlockKind::RecallContext, "recompiled subject"),
            evidence_block(BlockKind::PresenceContext, "recompiled contract"),
        ],
    );

    assert_eq!(serde_json::to_vec(&saved.turns[0]).unwrap(), first_bytes);
    assert_eq!(
        serde_json::to_vec(&saved.turns[1].blocks[..3]).unwrap(),
        second_bytes
    );
    assert_eq!(
        cache::latest(&saved, BlockKind::RecallContext)
            .unwrap()
            .content,
        "recompiled subject"
    );
    assert_eq!(
        cache::latest_for_turn(&saved, "second", BlockKind::PresenceContext)
            .unwrap()
            .content,
        "recompiled contract"
    );
}

#[tokio::test]
async fn skipped_retrieval_preserves_one_evidence_snapshot_without_promoting_it_to_identity() {
    let fixture = Fixture::new();
    let request = fixture.request("first").await;
    prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let mut context = session(&fixture.state, &fixture.meta.sender_session).await;
    let saved = context.saved.as_mut().unwrap();
    cache::merge(
        saved,
        "first",
        vec![evidence_block(
            BlockKind::RecallContext,
            "Current topical evidence",
        )],
    );
    let first_bytes = serde_json::to_vec(&serde_json::to_value(&saved.turns[0]).unwrap()).unwrap();
    cache::commit(&fixture.state, &fixture.meta.sender_session, saved).unwrap();
    drop(context);
    *fixture.state.context_sessions.lock().await = ContextSessions::default();

    let mut request = fixture.request("second").await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let result = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&result["turns"][0]).unwrap(),
        first_bytes
    );
    let blocks = result["turns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|turn| turn["blocks"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        blocks
            .iter()
            .filter(|block| block["kind"] == "recall-context")
            .count(),
        1
    );
    let presence = blocks
        .iter()
        .filter(|block| block["kind"] == "presence-context")
        .collect::<Vec<_>>();
    assert_eq!(presence.len(), 2);
    assert_eq!(presence[1]["details"]["turnId"], "second");
    assert!(!text(&presence[1]["content"]).contains("Current topical evidence"));
    let recall = blocks
        .iter()
        .find(|block| block["kind"] == "recall-context")
        .unwrap();
    assert_eq!(recall["content"], "Current topical evidence");
    assert_eq!(
        recall["details"]["presentation"]["retrievalCandidates"][0]["memory_id"],
        42
    );

    let runtime = fixture.state.runtime.lock().await;
    let (frame, _) = runtime
        .presence
        .session_state(&fixture.meta.sender_session)
        .unwrap();
    assert!(!frame.rendered.contains("Current topical evidence"));
    drop(runtime);

    let mut context = session(&fixture.state, &fixture.meta.sender_session).await;
    invalidate(&fixture.state, &fixture.meta.sender_session, &mut context).unwrap();
    drop(context);
    *fixture.state.context_sessions.lock().await = ContextSessions::default();
    let persisted = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(&persisted.turns).unwrap()).unwrap(),
        serde_json::to_vec(&result["turns"]).unwrap()
    );
    assert!(cache::latest(&persisted, BlockKind::RecallContext).is_none());
    assert!(cache::latest(&persisted, BlockKind::PresenceContext).is_none());
    assert!(cache::has_kind(&persisted, BlockKind::RoomContext));
}

#[tokio::test]
async fn native_format_cutover_rotates_topical_frames_and_preserves_the_ledger() {
    let fixture = Fixture::new();
    let authentication = authenticate_presence(&fixture.state, &fixture.meta).unwrap();
    let identity = materials::material(
        "identity:active-spirit".into(),
        json!({"kind":"identity","source":"active_spirit.md","sha256":"a".repeat(64)}),
        "identity",
        "Authenticated identity".into(),
        1000,
    );
    let topical = materials::material(
        "memory:42".into(),
        json!({"kind":"memory","memory_id":42}),
        "continuity",
        "Obsolete topical frame".into(),
        650,
    );
    let boat = materials::material(
        "paper-boat:9".into(),
        json!({"kind":"paper_boat","memory_id":9}),
        "continuity",
        "Retained boat continuity".into(),
        900,
    );
    let open = serde_json::from_value(json!({
        "binding":authentication.binding,"identity":[identity],"continuity":[topical,boat]
    }))
    .unwrap();
    fixture
        .state
        .runtime
        .lock()
        .await
        .presence
        .open_carrying(
            &authentication,
            "old-frame",
            open,
            Some(PresenceLedger {
                repair_rule_ids: vec!["rule:preserved".into()],
                ..PresenceLedger::default()
            }),
        )
        .unwrap();
    let mut saved = SavedSession {
        version: 1,
        ..SavedSession::default()
    };
    cache::merge(
        &mut saved,
        "first",
        vec![
            evidence_block(BlockKind::RoomContext, "Retained static bytes"),
            evidence_block(BlockKind::RecallContext, "Obsolete topical frame"),
        ],
    );
    cache::commit(&fixture.state, &fixture.meta.sender_session, &saved).unwrap();
    *fixture.state.context_sessions.lock().await = ContextSessions::default();

    let request = fixture.request("first").await;
    let result = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    assert_eq!(result["invalidationReason"], "context-format-changed");
    assert_eq!(result["replayed"], false);
    assert!(
        !result["turns"]
            .to_string()
            .contains("Obsolete topical frame")
    );
    assert!(
        result["turns"]
            .to_string()
            .contains("Retained static bytes")
    );
    let runtime = fixture.state.runtime.lock().await;
    let (frame, ledger) = runtime
        .presence
        .session_state(&fixture.meta.sender_session)
        .unwrap();
    assert!(!frame.rendered.contains("Obsolete topical frame"));
    assert!(frame.rendered.contains("Retained boat continuity"));
    assert_eq!(ledger.repair_rule_ids, vec!["rule:preserved"]);
    drop(runtime);
    let persisted = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert_eq!(persisted.version, cache::CONTEXT_VERSION);
}

#[tokio::test]
async fn recall_retirement_survives_an_uncommitted_assembly_timeout_and_restart() {
    let fixture = Fixture::new();
    let mut saved = SavedSession::default();
    cache::merge(
        &mut saved,
        "first",
        vec![
            evidence_block(BlockKind::RoomContext, "Retained startup"),
            evidence_block(BlockKind::RecallContext, "Retired subject"),
            evidence_block(BlockKind::PresenceContext, "Retired turn contract"),
        ],
    );
    let sent_bytes = serde_json::to_vec(&saved.turns).unwrap();
    cache::commit(&fixture.state, &fixture.meta.sender_session, &saved).unwrap();
    let mut context = session(&fixture.state, &fixture.meta.sender_session).await;
    context.saved = Some(saved.clone());
    let expired = tokio::time::timeout(Duration::from_millis(10), async {
        assembly::retire_working_set(&fixture.state, &fixture.meta, &mut saved, &mut context)
            .unwrap();
        std::future::pending::<()>().await;
    })
    .await;

    assert!(expired.is_err());
    assert_eq!(
        serde_json::to_vec(&context.saved.as_ref().unwrap().turns).unwrap(),
        sent_bytes
    );
    assert!(!cache::has_kind(
        context.saved.as_ref().unwrap(),
        BlockKind::RecallContext
    ));
    drop(context);
    *fixture.state.context_sessions.lock().await = ContextSessions::default();
    let restored = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert_eq!(serde_json::to_vec(&restored.turns).unwrap(), sent_bytes);
    assert!(!cache::has_kind(&restored, BlockKind::RecallContext));
    assert!(!cache::has_kind(&restored, BlockKind::PresenceContext));
    assert_eq!(
        cache::latest(&restored, BlockKind::RoomContext)
            .unwrap()
            .content,
        "Retained startup"
    );

    cache::merge(
        &mut saved,
        "recovery",
        vec![evidence_block(
            BlockKind::RecallContext,
            "Recovered subject",
        )],
    );
    cache::commit(&fixture.state, &fixture.meta.sender_session, &saved).unwrap();
    let recovered = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert_eq!(
        cache::latest(&recovered, BlockKind::RecallContext)
            .unwrap()
            .content,
        "Recovered subject"
    );
    assert_eq!(
        serde_json::to_vec(&recovered.turns[..1]).unwrap(),
        sent_bytes
    );
}

#[tokio::test]
async fn existing_memo_without_retirement_metadata_replays_without_a_format_cutover() {
    let fixture = Fixture::new();
    let request = fixture.request("first").await;
    let first = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let saved = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    let generation = saved.generation.clone();
    let mut memo = serde_json::to_value(saved).unwrap();
    memo["version"] = json!(2);
    memo.as_object_mut()
        .unwrap()
        .remove("retiredDynamicThrough");
    crate::store::atomic_json_write(
        &cache::file(&fixture.state, &fixture.meta.sender_session),
        &memo,
    )
    .unwrap();
    *fixture.state.context_sessions.lock().await = ContextSessions::default();

    let request = fixture.request("first").await;
    let replay = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(replay["replayed"], true);
    assert!(replay["invalidationReason"].is_null());
    assert_eq!(
        serde_json::to_vec(&replay["turns"]).unwrap(),
        serde_json::to_vec(&first["turns"]).unwrap()
    );
    assert_eq!(replay["presenceSettlement"], first["presenceSettlement"]);
    let restored = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert_eq!(restored.generation, generation);
    assert!(restored.retired_dynamic_through.is_empty());
}

#[tokio::test]
async fn quiet_prepare_preserves_sent_turns_and_supersedes_retired_recall() {
    let fixture = Fixture::new();
    let request = fixture.request("first").await;
    prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let mut context = session(&fixture.state, &fixture.meta.sender_session).await;
    let saved = context.saved.as_mut().unwrap();
    cache::merge(
        saved,
        "first",
        vec![evidence_block(
            BlockKind::RecallContext,
            "Retired topical evidence",
        )],
    );
    let first_bytes = serde_json::to_vec(&serde_json::to_value(&saved.turns[0]).unwrap()).unwrap();
    cache::commit(&fixture.state, &fixture.meta.sender_session, saved).unwrap();
    drop(context);

    let base_version = fixture.state.runtime.lock().await.cursor.version;
    command(
        &fixture.state,
        ClientCommand::SetRequestedMode {
            meta: child_meta(&fixture.meta, "quiet", "set-mode", "recall_policy"),
            base_version,
            requested_mode: protocol::RecallRequestedMode::Quiet,
        },
        json!("quiet"),
    )
    .await
    .unwrap();
    let mut request = fixture
        .request_for(
            "second",
            Capabilities {
                automatic_recall: true,
                top_level: true,
            },
            false,
        )
        .await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let result = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(
        serde_json::to_vec(&result["turns"][0]).unwrap(),
        first_bytes
    );
    let saved = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    let recall = cache::latest_for_turn(&saved, "second", BlockKind::RecallContext).unwrap();
    assert_eq!(recall.details["presentation"]["found"], false);
    assert!(
        recall
            .content
            .contains("supersedes every earlier Athanor Recall working set")
    );
    assert!(!recall.content.contains("Retired topical evidence"));
    let presentation = materials::recalled(&recall.details["presentation"]);
    assert!(presentation.is_empty());
    let presence = cache::latest_for_turn(&saved, "second", BlockKind::PresenceContext).unwrap();
    assert!(!presence.content.contains("Retired topical evidence"));
    assert!(cache::latest_for_turn(&saved, "first", BlockKind::RecallContext).is_none());
    assert!(cache::latest_for_turn(&saved, "first", BlockKind::PresenceContext).is_none());

    let mut request = fixture.request("first").await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let historical = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(historical["replayed"], true);
    assert!(historical["invalidationReason"].is_null());
    assert!(historical["presenceSettlement"].is_null());
    assert!(historical["presenceSettledContractId"].is_null());
    assert_eq!(
        serde_json::to_vec(&historical["turns"]).unwrap(),
        serde_json::to_vec(&result["turns"]).unwrap()
    );
}

#[tokio::test]
async fn retired_history_survives_host_restart_and_replay_prunes_only_invisible_turns() {
    let mut fixture = Fixture::new();
    let request = fixture.request("first").await;
    let first = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();
    let mut request = fixture.request("second").await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let second = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(
        serde_json::to_vec(&second["turns"][0]).unwrap(),
        serde_json::to_vec(&first["turns"][0]).unwrap()
    );
    let mut request = fixture.request("first").await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let historical = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(historical["replayed"], true);
    assert!(historical["invalidationReason"].is_null());
    assert!(historical["presenceSettlement"].is_null());
    assert!(historical["presenceSettledContractId"].is_null());
    assert_eq!(
        serde_json::to_vec(&historical["turns"]).unwrap(),
        serde_json::to_vec(&second["turns"]).unwrap()
    );

    let mut context = session(&fixture.state, &fixture.meta.sender_session).await;
    invalidate(&fixture.state, &fixture.meta.sender_session, &mut context).unwrap();
    drop(context);
    fixture.state.cancellation.cancel();
    fixture.state = Host::new(
        fixture.state.config.as_ref().clone(),
        None,
        CancellationToken::new(),
        TaskTracker::new(),
    )
    .unwrap()
    .state;

    let mut request = fixture.request("second").await;
    request["visibleTurnIds"] = json!(["first", "second"]);
    let restarted = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(restarted["replayed"], true);
    assert!(restarted["invalidationReason"].is_null());
    assert!(restarted["presenceSettlement"].is_null());
    assert!(restarted["presenceSettledContractId"].is_null());
    assert_eq!(
        serde_json::to_vec(&restarted["turns"]).unwrap(),
        serde_json::to_vec(&second["turns"]).unwrap()
    );
    let saved = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert!(!cache::has_kind(&saved, BlockKind::PresenceContext));
    assert_eq!(saved.retired_dynamic_through.len(), 2);

    let request = fixture.request("second").await;
    let pruned = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(pruned["replayed"], true);
    assert_eq!(pruned["turns"].as_array().unwrap().len(), 1);
    assert_eq!(
        serde_json::to_vec(&pruned["turns"][0]).unwrap(),
        serde_json::to_vec(&second["turns"][1]).unwrap()
    );
    let persisted = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert_eq!(persisted.turns.len(), 1);
    assert_eq!(persisted.turns[0].turn_id, "second");
    assert!(!persisted.retired_dynamic_through.contains_key("first"));
    assert!(persisted.retired_dynamic_through.contains_key("second"));

    let mut request = fixture.request("first").await;
    request["visibleTurnIds"] = json!(["second", "first"]);
    let reused = prepare_inner(&fixture.state, &fixture.meta, request)
        .await
        .unwrap();

    assert_eq!(reused["replayed"], false);
    assert_eq!(
        serde_json::to_vec(&reused["turns"][0]).unwrap(),
        serde_json::to_vec(&pruned["turns"][0]).unwrap()
    );
    let saved = cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();
    assert!(cache::latest_for_turn(&saved, "first", BlockKind::PresenceContext).is_some());
}

#[tokio::test]
async fn pruning_newest_dynamic_turns_does_not_reactivate_superseded_evidence() {
    let fixture = Fixture::new();
    let mut saved = SavedSession::default();
    cache::merge(
        &mut saved,
        "first",
        vec![
            evidence_block(BlockKind::RoomContext, "Retained identity"),
            evidence_block(BlockKind::RecallContext, "Original evidence"),
            evidence_block(BlockKind::PresenceContext, "Original contract"),
        ],
    );
    let first_bytes = serde_json::to_vec(&saved.turns[0]).unwrap();
    cache::merge(
        &mut saved,
        "second",
        vec![evidence_block(
            BlockKind::PresenceContext,
            "Second contract",
        )],
    );
    cache::commit(&fixture.state, &fixture.meta.sender_session, &saved).unwrap();
    let mut restored =
        cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();

    assert_eq!(
        cache::latest(&restored, BlockKind::RecallContext)
            .unwrap()
            .content,
        "Original evidence"
    );
    assert_eq!(
        cache::latest_for_turn(&restored, "second", BlockKind::PresenceContext)
            .unwrap()
            .content,
        "Second contract"
    );
    assert!(cache::prune(&mut restored, &["first".into()]));
    assert_eq!(serde_json::to_vec(&restored.turns[0]).unwrap(), first_bytes);
    assert!(cache::has_kind(&restored, BlockKind::RecallContext));
    assert!(!cache::has_kind(&restored, BlockKind::PresenceContext));

    cache::merge(
        &mut restored,
        "replacement",
        vec![
            evidence_block(BlockKind::RecallContext, "Replacement evidence"),
            evidence_block(BlockKind::PresenceContext, "Replacement contract"),
        ],
    );
    cache::commit(&fixture.state, &fixture.meta.sender_session, &restored).unwrap();
    let mut restored =
        cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();

    assert_eq!(
        cache::latest(&restored, BlockKind::RecallContext)
            .unwrap()
            .content,
        "Replacement evidence"
    );
    assert!(cache::prune(&mut restored, &["first".into()]));
    cache::commit(&fixture.state, &fixture.meta.sender_session, &restored).unwrap();
    let mut restored =
        cache::load(&fixture.state, &fixture.meta.sender_session, None, &[]).unwrap();

    assert_eq!(serde_json::to_vec(&restored.turns[0]).unwrap(), first_bytes);
    assert!(cache::has_kind(&restored, BlockKind::RoomContext));
    assert!(!cache::has_kind(&restored, BlockKind::RecallContext));
    assert!(!cache::has_kind(&restored, BlockKind::PresenceContext));
    assert!(cache::latest_for_turn(&restored, "first", BlockKind::PresenceContext).is_none());
    assert_eq!(restored.retired_dynamic_through.len(), 1);
    assert!(restored.retired_dynamic_through.contains_key("first"));
    assert!(!cache::prune(&mut restored, &["first".into()]));
}
