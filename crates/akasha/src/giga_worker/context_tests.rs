use super::*;
use crate::EmbeddingMode;
use crate::giga_worker::classify::target_candidate;
use crate::giga_worker::typesafe::parse_judgment;
use crate::migrations::run_migrations;
use hearth::{
    GigaCandidateKind, GigaEventType, GigaLifecycle, GigaScope, GigaSourceRef, GigaSourceType,
    GigaVisibility, MemoryScope, RememberMemoryDetails, RememberRequest,
};
use serde_json::Map;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::path::PathBuf;
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ROOM: &str = "giga-context-proof";
const TARGET_ID: &str = "target-turn";
const KINDS: [&str; 6] = [
    "coding_lesson",
    "correction",
    "memory",
    "none",
    "project_lesson",
    "supersession",
];

fn test_classifier() -> GigaClassifierConfig {
    GigaClassifierConfig::new(
        "test-credential-value".into(),
        "giga-r1".into(),
        PathBuf::from("unused-room-dir"),
        ROOM.into(),
    )
    .unwrap()
}

fn turn(offset: i64, text: &str) -> ContextTurn {
    let role = if offset % 2 == 0 { "user" } else { "assistant" };
    ContextTurn {
        offset,
        role: role.into(),
        text: text.into(),
    }
}

fn judged_target(text: &str, context: Vec<ContextTurn>) -> (GigaEvent, ResolvedSource) {
    let scope = GigaScope::new(Some(ROOM.into()), None, GigaVisibility::Private, true).unwrap();
    let source = GigaSourceRef::new(
        GigaSourceType::Turn,
        TARGET_ID.into(),
        "user".into(),
        "2030-01-01T00:00:00Z".into(),
        "a".repeat(64),
        scope,
        None,
    )
    .unwrap();
    let event = GigaEvent::new(
        "context-event".into(),
        GigaEventType::ConversationWindow,
        RoomKey::new(ROOM).unwrap(),
        "context-session".into(),
        Vec::new(),
        vec![source.clone()],
        GigaLifecycle::conversation_window(),
        "2030-01-01T00:00:00Z".into(),
    )
    .unwrap();
    let resolved = ResolvedSource {
        source,
        text: text.into(),
        context,
        withheld_context: 1,
    };
    (event, resolved)
}

/// A completed lookup holding one canon entry and one memory, as Recall shapes them.
fn completed_memory(operator: Option<&str>, excerpt: &str) -> MemoryContext {
    let mut memory = MemoryContext::new(
        operator.map(|key| OperatorKey::new(key).unwrap()),
        Some("recall-r1".into()),
    );
    memory.status = MemoryStatus::Complete;
    memory.reason = None;
    memory.queries.push("lighthouse keeper".into());
    memory.canon.push(
        canon_entry(&json!({"termKey": "Lighthouse", "entry": {
            "id": 7, "type": "place", "summary": "The lighthouse on the north cape.", "exact": true
        }}))
        .unwrap(),
    );
    memory.memories.push(
        memory_entry(&json!({
            "memory_id": 42, "title": "Keeper schedule", "source_path": "memories/keeper.md",
            "excerpt": excerpt, "score": 0.812_345, "body": "full body never sent"
        }))
        .unwrap(),
    );
    memory
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

fn cited_ids(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "source_id"
                    && let Some(id) = value.as_str()
                {
                    found.push(id.to_owned());
                }
                cited_ids(value, found);
            }
        }
        Value::Array(values) => values.iter().for_each(|value| cited_ids(value, found)),
        _ => {}
    }
}

fn plan(memory: &MemoryContext, lookup: Vec<String>) -> JudgmentPlan {
    JudgmentPlan {
        source_id: TARGET_ID.into(),
        novelty: memory.status() == MemoryStatus::Complete,
        lookup,
    }
}

#[test]
fn packets_cite_only_the_target_and_state_memory_availability() {
    let (event, target) = judged_target(
        "Sol decided the lighthouse keeper logs at dusk.",
        vec![
            turn(-1, "When should the lighthouse keeper log?"),
            turn(1, "Logging at dusk then, every evening."),
        ],
    );
    let classifier = test_classifier();

    let mut refused = MemoryContext::new(None, None);
    let mut refused_plan = plan(&refused, Vec::new());
    let body = fitted_body(
        &classifier,
        &event,
        &target,
        &mut refused,
        &mut refused_plan,
    )
    .unwrap();
    let packet: Value = serde_json::from_str(&body).unwrap();
    let memory = &packet["state"]["event"]["memory_context"];
    assert_eq!(memory["status"], "refused");
    assert_eq!(memory["reason"], "recall_grant_missing");
    assert_eq!(memory["scope"], "local_house");
    assert_eq!(memory["memories"], json!([]));
    assert_eq!(
        sorted_keys(&packet["questions"]),
        ["durability_0", "kind_0", "priority_0"]
    );

    let mut complete = completed_memory(Some("staff"), "The keeper logs at dawn.");
    let options = lookup_options(&target.text, &target.context, None);
    assert!(!options.is_empty());
    let mut complete_plan = plan(&complete, options);
    let body = fitted_body(
        &classifier,
        &event,
        &target,
        &mut complete,
        &mut complete_plan,
    )
    .unwrap();
    let packet: Value = serde_json::from_str(&body).unwrap();
    let state = &packet["state"]["event"];
    assert_eq!(state["memory_context"]["status"], "complete");
    assert_eq!(state["memory_context"]["scope"], "bound_operator");
    assert_eq!(state["memory_context"]["canon"][0]["canon_id"], 7);
    assert_eq!(state["memory_context"]["memories"][0]["memory_id"], 42);
    assert_eq!(
        state["memory_context"]["memories"][0]["source_path"],
        "memories/keeper.md"
    );
    assert_eq!(
        sorted_keys(&packet["questions"]),
        [
            "durability_0",
            "kind_0",
            "lookup_0",
            "novelty_0",
            "priority_0"
        ]
    );
    assert_eq!(state["nearby_turns_withheld"], 1);
    for nearby in state["nearby_turns"].as_array().unwrap() {
        assert_eq!(sorted_keys(nearby), ["offset", "role", "text"]);
    }
    let mut cited = Vec::new();
    cited_ids(&packet, &mut cited);
    assert_eq!(cited, [TARGET_ID]);
    for private in ["test-credential-value", "staff", "full body never sent"] {
        assert!(
            !body.contains(private),
            "{private} must not leave the House"
        );
    }
}

#[test]
fn provider_limit_trims_memories_then_far_turns_and_never_the_target() {
    let text = "\"quoted\" ".repeat(1_200);
    // Nearest first, earlier on a tie: the order in which turns are kept.
    let nearest = [-1, 1, -2, 2, -3, 3, -4];
    let context = nearest
        .into_iter()
        .map(|offset| turn(offset, &"\"say\" ".repeat(400)))
        .collect();
    let (event, target) = judged_target(&text, context);
    let classifier = test_classifier();
    let mut memory = completed_memory(None, &"\"".repeat(480));
    for canon_id in 8..11 {
        memory
            .canon
            .push(json!({"canon_id": canon_id, "summary": "\"".repeat(480)}));
    }
    memory
        .memories
        .push(json!({"memory_id": 43, "excerpt": "\"".repeat(480)}));
    let mut fitted_plan = plan(&memory, Vec::new());

    let body = fitted_body(&classifier, &event, &target, &mut memory, &mut fitted_plan).unwrap();
    assert!(body.len() <= TYPESAFE_MAX_PACKET_BYTES);
    let packet: Value = serde_json::from_str(&body).unwrap();
    let state = &packet["state"]["event"];
    assert_eq!(state["target"]["text"], text.as_str());
    assert_eq!(state["memory_context"]["memories"], json!([]));
    assert_eq!(state["memory_context"]["trimmed"], 2);
    assert_eq!(
        state["memory_context"]["canon"].as_array().unwrap().len(),
        4
    );
    let trimmed_turns = usize::try_from(state["nearby_turns_trimmed"].as_u64().unwrap()).unwrap();
    assert!(trimmed_turns >= 1);
    let mut kept = state["nearby_turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|turn| turn["offset"].as_i64().unwrap())
        .collect::<Vec<_>>();
    let mut expected = nearest[..nearest.len() - trimmed_turns].to_vec();
    kept.sort_unstable();
    expected.sort_unstable();
    assert_eq!(kept, expected, "the farthest turns go first");

    let (event, oversized) = judged_target(&"\"x\" ".repeat(9_000), Vec::new());
    let mut refused = MemoryContext::new(None, None);
    let mut oversized_plan = plan(&refused, Vec::new());
    let failure = fitted_body(
        &classifier,
        &event,
        &oversized,
        &mut refused,
        &mut oversized_plan,
    )
    .unwrap_err();
    assert_eq!(failure.class(), "GigaClassifierRequestError");
}

#[test]
fn empty_or_removed_memory_never_supports_a_novelty_judgment() {
    let (event, target) = judged_target("The keeper changed the dusk schedule.", Vec::new());
    let classifier = test_classifier();
    let mut memory = completed_memory(None, "The keeper previously logged at dawn.");
    let mut judgment_plan = plan(&memory, vec!["keeper dusk schedule".into()]);
    memory.canon.clear();
    memory.memories.clear();

    let body = fitted_body(
        &classifier,
        &event,
        &target,
        &mut memory,
        &mut judgment_plan,
    )
    .unwrap();
    let packet: Value = serde_json::from_str(&body).unwrap();

    assert_eq!(memory.status(), MemoryStatus::NotFound);
    assert!(memory.may_lookup());
    assert!(!judgment_plan.novelty);
    assert!(packet["questions"].get("novelty_0").is_none());
    assert!(packet["questions"].get("lookup_0").is_some());
}

#[test]
fn recall_evidence_requires_the_existing_recall_private_packet_grant() {
    let approved = json!({"room": ROOM, "jevRecall": {"mode": "active", "provider": "typesafe",
        "grant": {"purpose": "recall-rerank", "allowPrivateRecallPackets": true, "policyRevision": "recall-r1"}}});
    let variant = |edit: fn(&mut Value)| {
        let mut marker = approved.clone();
        edit(&mut marker);
        marker
    };
    assert_eq!(
        recall_grant_revision(&approved, ROOM).as_deref(),
        Some("recall-r1")
    );
    let shadow = variant(|marker| marker["jevRecall"]["mode"] = json!("shadow"));
    assert_eq!(
        recall_grant_revision(&shadow, ROOM).as_deref(),
        Some("recall-r1")
    );
    let refusals = [
        variant(|marker| marker["room"] = json!("another-room")),
        variant(|marker| marker["jevRecall"]["mode"] = json!("off")),
        variant(|marker| marker["jevRecall"]["provider"] = json!("laya")),
        variant(|marker| marker["jevRecall"]["grant"]["purpose"] = json!("lesson-sieve")),
        variant(|marker| marker["jevRecall"]["grant"]["allowPrivateRecallPackets"] = json!(false)),
        variant(|marker| {
            let grant = marker["jevRecall"]["grant"].as_object_mut().unwrap();
            grant.remove("allowPrivateRecallPackets");
            grant.insert("allowPrivateConversationPackets".into(), json!(true));
        }),
        variant(|marker| marker["jevRecall"]["grant"]["policyRevision"] = json!("")),
        json!({"room": ROOM, "jevGiga": {"mode": "active", "provider": "typesafe",
            "grant": {"purpose": "giga-classify", "allowPrivateConversationPackets": true, "policyRevision": "giga-r1"}}}),
    ];
    for marker in refusals {
        assert_eq!(recall_grant_revision(&marker, ROOM), None, "{marker}");
        let memory = MemoryContext::new(None, recall_grant_revision(&marker, ROOM));
        assert_eq!(memory.status(), MemoryStatus::Refused);
        assert_eq!(memory.recall_revision(), None);
        assert!(!memory.may_lookup());
    }
}

#[test]
fn lookups_are_bounded_and_drawn_from_the_conversation() {
    let target_text =
        "The lighthouse keeper moved the dusk log to the north cape tower. ".repeat(20);
    let context = vec![
        turn(
            -1,
            "Kintsu asked whether Portuguese stays the default language for sessions.",
        ),
        turn(1, "ok"),
        turn(
            -2,
            "Earlier we chose PostgreSQL for the durable queue release.",
        ),
    ];
    let initial = initial_query(&target_text, &context).unwrap();
    assert!(initial.len() <= QUERY_MAX_BYTES);
    let target_prefix = clipped(&target_text, QUERY_TARGET_BYTES);
    assert!(initial.starts_with(&target_prefix));
    let context_terms = initial[target_prefix.len()..]
        .split_whitespace()
        .collect::<Vec<_>>();
    assert!(!context_terms.is_empty() && context_terms.len() <= QUERY_CONTEXT_TERMS);
    for term in context_terms {
        assert!(
            context
                .iter()
                .any(|turn| turn.text.to_lowercase().contains(term)),
            "{term} must come from a nearby turn"
        );
    }

    let options = lookup_options(&target_text, &context, Some(&initial));
    assert!(!options.is_empty() && options.len() <= LOOKUP_OPTIONS);
    let conversation = std::iter::once(target_text.as_str())
        .chain(context.iter().map(|turn| turn.text.as_str()))
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>();
    for option in &options {
        assert!(option.len() <= LOOKUP_OPTION_BYTES);
        assert!(
            conversation
                .iter()
                .any(|text| text.contains(option.as_str()))
        );
    }
    assert_eq!(
        options
            .iter()
            .filter(|option| option.starts_with("The lighthouse keeper"))
            .count(),
        1,
        "a repeated sentence is one option"
    );
    assert_eq!(initial_query("?! ...", &[turn(-1, "?!")]), None);
}

fn choice(chosen: &str, keys: &[&str]) -> Value {
    let rest = 0.1 / (keys.len() - 1) as f64;
    let probabilities = keys
        .iter()
        .map(|key| {
            let probability = if *key == chosen { 0.9 } else { rest };
            ((*key).to_owned(), json!(probability))
        })
        .collect::<Map<_, _>>();
    json!({"type": "choice", "choice": chosen, "confidence": 0.9, "probabilities": probabilities})
}

fn score(value: f64) -> Value {
    json!({"type": "score", "score": value, "confidence": 0.8,
        "probabilities": {"0": 1.0 - value, "1": value}})
}

#[test]
fn judgment_parsing_keeps_the_fixed_choice_and_score_contract() {
    let lookup_plan = JudgmentPlan {
        source_id: TARGET_ID.into(),
        novelty: true,
        lookup: vec!["first query text".into(), "second query text".into()],
    };
    let lookup_keys = ["none", "query_1", "query_2"];
    let answers = |lookup: Value, extra: Option<(&str, Value)>| {
        let mut answers = json!({
            "kind_0": choice("memory", &KINDS), "priority_0": score(0.7),
            "durability_0": score(0.6), "novelty_0": score(0.3), "lookup_0": lookup,
        });
        if let Some((key, value)) = extra {
            answers[key] = value;
        }
        json!({"model": "jev-test", "answers": answers}).to_string()
    };
    let judgment = parse_judgment(
        &answers(choice("query_2", &lookup_keys), None),
        &lookup_plan,
    )
    .unwrap();
    assert_eq!(judgment.kind, "memory");
    assert_eq!(judgment.novelty, Some(0.3));
    assert_eq!(judgment.lookup, Some(1));
    let declined =
        parse_judgment(&answers(choice("none", &lookup_keys), None), &lookup_plan).unwrap();
    assert_eq!(declined.lookup, None);

    let unoffered = choice("query_3", &["none", "query_1", "query_2", "query_3"]);
    let extra = Some(("summary_0", score(0.5)));
    for raw in [
        answers(unoffered, None),
        answers(choice("none", &lookup_keys), extra),
    ] {
        assert_eq!(
            parse_judgment(&raw, &lookup_plan).unwrap_err().class(),
            "GigaClassifierOutputError"
        );
    }
    let without_memory = JudgmentPlan {
        source_id: TARGET_ID.into(),
        novelty: false,
        lookup: Vec::new(),
    };
    // Novelty asked of a packet without completed memory context is refused output.
    let unasked = json!({"model": "jev-test", "answers": {
        "kind_0": choice("memory", &KINDS), "priority_0": score(0.7),
        "durability_0": score(0.6), "novelty_0": score(0.9),
    }})
    .to_string();
    assert_eq!(
        parse_judgment(&unasked, &without_memory)
            .unwrap_err()
            .class(),
        "GigaClassifierOutputError"
    );
}

#[test]
fn candidates_cite_only_the_target_never_recalled_or_nearby_context() {
    let (event, target) = judged_target(
        "Sol corrected the keeper: logs happen at dusk, not dawn as noted before.",
        vec![turn(-1, "The keeper logs at dawn.")],
    );
    let memory = completed_memory(Some("staff"), "The keeper logs at dawn.");
    let judgment = JevJudgment {
        model: "jev-test".into(),
        kind: "correction".into(),
        priority: 0.8,
        durability: 0.7,
        confidence: 0.9,
        novelty: Some(0.2),
        lookup: None,
    };
    let digest = "d".repeat(64);
    let candidate = target_candidate(
        &event,
        &target,
        GigaCandidateKind::Correction,
        judgment.clone(),
        &memory,
        &digest,
    )
    .unwrap();
    assert_eq!(candidate.source_refs(), [target.source.clone()]);
    assert_eq!(candidate.proof_refs(), [TARGET_ID]);
    assert_eq!(candidate.scores().novelty(), 0.2);
    assert!(candidate.rationale().contains("canon: 7; memories: 42"));
    assert!(candidate.rationale().contains("bound_operator"));

    let unavailable = MemoryContext::new(None, Some("recall-r1".into()));
    let unmeasured = target_candidate(
        &event,
        &target,
        GigaCandidateKind::Memory,
        JevJudgment {
            kind: "memory".into(),
            novelty: Some(0.9),
            ..judgment
        },
        &unavailable,
        &digest,
    )
    .unwrap();
    assert_eq!(unmeasured.source_refs(), [target.source.clone()]);
    assert!(unmeasured.proof_refs().is_empty());
    assert_eq!(unmeasured.scores().novelty(), 0.0);
    assert!(
        unmeasured
            .rationale()
            .starts_with("Novelty was not measured: memory context was unavailable")
    );
}

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let options: PgConnectOptions = url.parse().expect("valid test URL");
    let database = options
        .get_database()
        .expect("explicit test database")
        .to_ascii_lowercase();
    assert!(
        database.contains("test") && !database.contains("solarisael"),
        "refusing a non-test or live database"
    );
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing the live database"
    );
    url
}

async fn isolated_schema(suffix: &str) -> TestResult<(PgPool, String, Config)> {
    let base = std::env::var("ATHANOR_SUBSTRATE_TEST_SCHEMA")
        .expect("this proof requires ATHANOR_SUBSTRATE_TEST_SCHEMA");
    assert!(base.starts_with("solarisael_tuner_test_"));
    let schema = format!("{base}_{suffix}");
    assert!(
        schema
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    );
    let url = isolated_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(PgConnectOptions::from_str(&url)?)
        .await?;
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(&pool)
        .await?;
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await?;
    sqlx::query(&format!("SET search_path TO {schema}, public"))
        .execute(&pool)
        .await?;
    run_migrations(&pool).await?;
    let config = Config {
        database_url: url,
        embed_url: None,
        embed_model: "disabled".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "UTC".into(),
        nats_url: None,
    };
    Ok((pool, schema, config))
}

async fn remember_as(
    pool: &PgPool,
    config: &Config,
    writer: Option<&str>,
    scope: Option<MemoryScope>,
    marker: &str,
) -> TestResult<i64> {
    let request = RememberRequest::new_memory(
        RoomKey::for_memory_write(ROOM)?,
        format!("lighthouse {marker}"),
        format!("The lighthouse keeper logged {marker} at dusk."),
        RememberMemoryDetails {
            source_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec![],
            backup: false,
        },
    )?
    .with_operator(writer.map(OperatorKey::new).transpose()?, scope)?;
    let receipt = crate::remember::remember(pool, config, request).await?;
    Ok(i64::try_from(receipt.memory_id)?)
}

async fn looked_up(
    pool: &PgPool,
    config: &Config,
    operator: Option<&str>,
    grant: Option<&str>,
) -> TestResult<MemoryContext> {
    let mut memory = MemoryContext::new(
        operator.map(OperatorKey::new).transpose()?,
        grant.map(str::to_owned),
    );
    let query = initial_query("The lighthouse keeper logs at dusk.", &[]).unwrap();
    memory
        .lookup(pool, config, &RoomKey::new(ROOM)?, query)
        .await;
    Ok(memory)
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn memory_lookups_read_in_the_trusted_host_scope_and_stop_at_two() -> TestResult {
    let (pool, schema, config) = isolated_schema("giga_context").await?;
    sqlx::query(
        "INSERT INTO units (path, display_name) VALUES
           ('acme'::ltree, 'Acme'), ('acme.eng'::ltree, 'Engineering')",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO operators (key, display_name, unit, rank) VALUES
           ('staff', 'Staff', 'acme.eng'::ltree, 1), ('peer', 'Peer', 'acme.eng'::ltree, 1)",
    )
    .execute(&pool)
    .await?;
    let legacy = remember_as(&pool, &config, None, None, "legacy-row").await?;
    let staff_private = remember_as(
        &pool,
        &config,
        Some("staff"),
        Some(MemoryScope::Private),
        "staff-private",
    )
    .await?;
    let staff_company = remember_as(
        &pool,
        &config,
        Some("staff"),
        Some(MemoryScope::Company),
        "staff-company",
    )
    .await?;
    let peer_private = remember_as(
        &pool,
        &config,
        Some("peer"),
        Some(MemoryScope::Private),
        "peer-private",
    )
    .await?;

    // Local House: the Host binds no operator, so Recall reads as it does without one.
    let local = looked_up(&pool, &config, None, Some("recall-r1")).await?;
    assert_eq!(local.status(), MemoryStatus::Complete);
    assert_eq!(local.scope(), "local_house");
    let ids = local.memory_ids();
    assert!(
        ids.contains(&legacy) && ids.contains(&staff_company),
        "{ids:?}"
    );
    assert!(
        !ids.contains(&staff_private) && !ids.contains(&peer_private),
        "{ids:?}"
    );

    // A bound operator reads its own private rows, never another operator's.
    for (operator, own, foreign) in [
        ("staff", staff_private, peer_private),
        ("peer", peer_private, staff_private),
    ] {
        let bound = looked_up(&pool, &config, Some(operator), Some("recall-r1")).await?;
        assert_eq!(bound.scope(), "bound_operator");
        let ids = bound.memory_ids();
        assert!(
            ids.contains(&own) && ids.contains(&legacy),
            "{operator}: {ids:?}"
        );
        assert!(!ids.contains(&foreign), "{operator}: {ids:?}");
    }
    sqlx::query("UPDATE memories SET archived_at=NOW(),superseded_by=$2 WHERE id=$1")
        .bind(legacy)
        .bind(staff_company)
        .execute(&pool)
        .await?;
    let mut historical = MemoryContext::new(None, Some("recall-r1".into()));
    historical
        .lookup(
            &pool,
            &config,
            &RoomKey::new(ROOM)?,
            format!("memory {legacy}"),
        )
        .await;
    let record = historical
        .memories
        .iter()
        .find(|record| record["memory_id"].as_i64() == Some(legacy))
        .expect("an explicit historical reference remains available");
    assert_eq!(record["historical"], true);
    assert_eq!(record["retrieval_source"], "exact_id");
    assert_eq!(record["historical_reasons"].as_array().unwrap().len(), 2);

    // Without the Recall private-packet grant no lookup runs at all.
    let refused = looked_up(&pool, &config, Some("staff"), None).await?;
    assert_eq!(refused.status(), MemoryStatus::Refused);
    assert_eq!(refused.lookups(), 0);
    assert!(refused.memory_ids().is_empty());

    // Each lookup is bounded, and a target gets at most two.
    for marker in ["first", "second", "third", "fourth", "fifth", "sixth"] {
        remember_as(
            &pool,
            &config,
            None,
            Some(MemoryScope::Company),
            &format!("company-{marker}"),
        )
        .await?;
    }
    let room = RoomKey::new(ROOM)?;
    let mut bounded = MemoryContext::new(None, Some("recall-r1".into()));
    assert!(
        bounded
            .lookup(&pool, &config, &room, "lighthouse keeper".into())
            .await
    );
    assert!(bounded.memory_ids().len() <= MEMORIES_PER_LOOKUP);
    assert!(
        bounded
            .lookup(&pool, &config, &room, "dusk logged".into())
            .await
    );
    assert!(
        !bounded
            .lookup(&pool, &config, &room, "lighthouse".into())
            .await
    );
    assert_eq!(bounded.lookups(), GIGA_MAX_RECALL_LOOKUPS);
    assert!(bounded.memory_ids().len() <= GIGA_MAX_RECALL_LOOKUPS * MEMORIES_PER_LOOKUP);
    for memory in &bounded.memories {
        let excerpt = memory["excerpt"].as_str().unwrap_or_default();
        assert!(excerpt.chars().count() <= EVIDENCE_EXCERPT_CHARS + 1);
    }

    sqlx::query("SET search_path TO public")
        .execute(&pool)
        .await?;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
