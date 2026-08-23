use crate::Config;
use crate::config::EmbeddingMode;
use hearth::{
    GIGA_MAX_PROCESS_SOURCE_BYTES, GigaEvent, GigaEventType, GigaLifecycle, GigaScope,
    GigaSourceRef, GigaSourceType, GigaVisibility, RoomKey,
};
use reqwest::Url;
use serde_json::{Value, json};
use std::path::Path;
use std::{env, time::Duration};
use tokio::{fs, sync::watch};
use uuid::Uuid;
use super::bounds::{
    GIGA_MAX_STORED_RATIONALE_BYTES, GIGA_RATIONALE_TRUNCATION_MARKER, truncate_with_marker,
};
use super::identity::{candidate_id, configuration_digest, sha256_bytes};
use super::ledger::resolve_sources_from_ledger;
use super::ollama::{OllamaConfig, salvage_json_slice};
use super::schema::{ExtractionOutput, GateKind, GateOutput, extraction_schema};
use super::validation::{validate_extraction, validate_gate};
use super::worker::giga_worker_loop;

fn conversation_event(
    room: &RoomKey,
    project: Option<&str>,
    texts: &[(&str, &str, &str)],
) -> GigaEvent {
    let scope = GigaScope::new(
        Some(room.to_string()),
        project.map(str::to_owned),
        GigaVisibility::Private,
        true,
    )
    .unwrap();
    let sources = texts
        .iter()
        .enumerate()
        .map(|(index, (source_id, role, text))| {
            GigaSourceRef::new(
                GigaSourceType::Turn,
                (*source_id).into(),
                (*role).into(),
                format!("2026-07-24T12:00:0{index}Z"),
                sha256_bytes(text.as_bytes()),
                scope.clone(),
                None,
            )
            .unwrap()
        })
        .collect();
    GigaEvent::new(
        "event-1".into(),
        GigaEventType::ConversationWindow,
        room.clone(),
        "session-1".into(),
        project.into_iter().map(str::to_owned).collect(),
        sources,
        GigaLifecycle::conversation_window(),
        "2026-07-24T12:00:10Z".into(),
    )
    .unwrap()
}

fn source_config(directory: &Path, room: &str) -> Config {
    Config {
        database_url: "postgres://unused".into(),
        embed_url: None,
        embed_model: "unused".into(),
        embed_dimension: 2_048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: Some(directory.to_owned()),
        giga_source_room: Some(room.into()),
        house_tz: "America/Sao_Paulo".into(),
    }
}

async fn write_ledger(directory: &Path, records: &[Value]) {
    fs::create_dir_all(directory).await.unwrap();
    let body = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(directory.join("2026-07-24.jsonl"), format!("{body}\n"))
        .await
        .unwrap();
}

fn ledger_record(source_id: &str, role: &str, text: &str) -> Value {
    json!({
        "sessionID": "session-1",
        "messageID": source_id,
        "role": role,
        "spirit": "Lab",
        "text": text
    })
}

#[test]
fn classifier_salvage_parse_strips_thinking_preamble() {
    let content = "Thinking Process:\n1. blah\n{\"foo\":1}";
    assert!(serde_json::from_str::<Value>(content).is_err());
    let salvaged = salvage_json_slice(content).expect("object slice");
    assert_eq!(salvaged, "{\"foo\":1}");
    assert_eq!(
        serde_json::from_str::<Value>(salvaged).unwrap(),
        json!({ "foo": 1 })
    );
    assert_eq!(salvage_json_slice("preamble [1,2]"), Some("[1,2]"));
    assert_eq!(salvage_json_slice("no json here"), None);
}

#[tokio::test]
async fn persisted_source_loader_preserves_event_order_and_types_failures() {
    let room = RoomKey::new("lab").unwrap();
    let event = conversation_event(
        &room,
        None,
        &[
            ("turn-1", "user", "exact user text"),
            ("turn-2", "assistant", "exact assistant text"),
        ],
    );
    let directory = env::temp_dir().join(format!("giga-source-loader-{}", Uuid::new_v4()));
    let config = source_config(&directory, "lab");

    write_ledger(
        &directory,
        &[
            ledger_record("turn-2", "assistant", "exact assistant text"),
            ledger_record("turn-1", "user", "exact user text"),
        ],
    )
    .await;
    let resolved = resolve_sources_from_ledger(&config, &event).await.unwrap();
    assert_eq!(resolved[0].source.source_id(), "turn-1");
    assert_eq!(resolved[1].source.source_id(), "turn-2");

    write_ledger(
        &directory,
        &[ledger_record("turn-1", "user", "exact user text")],
    )
    .await;
    let missing = resolve_sources_from_ledger(&config, &event)
        .await
        .unwrap_err();
    assert_eq!(missing.class(), "GigaSourceMissingError");

    write_ledger(
        &directory,
        &[
            ledger_record("turn-1", "user", "changed"),
            ledger_record("turn-2", "assistant", "exact assistant text"),
        ],
    )
    .await;
    let mismatch = resolve_sources_from_ledger(&config, &event)
        .await
        .unwrap_err();
    assert_eq!(mismatch.class(), "GigaSourceHashMismatchError");

    let oversized_text = "x".repeat(GIGA_MAX_PROCESS_SOURCE_BYTES + 1);
    let oversized_event =
        conversation_event(&room, None, &[("turn-large", "user", &oversized_text)]);
    write_ledger(
        &directory,
        &[ledger_record("turn-large", "user", &oversized_text)],
    )
    .await;
    let oversized = resolve_sources_from_ledger(&config, &oversized_event)
        .await
        .unwrap_err();
    assert_eq!(oversized.class(), "GigaSourceWindowTooLargeError");

    let wrong_room = source_config(&directory, "other-room");
    let unverified = resolve_sources_from_ledger(&wrong_room, &event)
        .await
        .unwrap_err();
    assert_eq!(unverified.class(), "GigaSourceVerificationError");

    fs::remove_dir_all(directory).await.unwrap();
}

#[test]
fn semantic_gate_and_extractor_validation_rejects_invalid_kinds_proofs_and_scores() {
    let room = RoomKey::new("lab").unwrap();
    let event = conversation_event(
        &room,
        None,
        &[("turn-1", "user", "rule"), ("turn-2", "assistant", "proof")],
    );
    let valid_gate = GateOutput {
        kind: GateKind::CodingLesson,
        source_ids: vec!["turn-1".into(), "turn-2".into()],
        reason: "Explicit reusable rule with proof".into(),
    };
    validate_gate(&valid_gate, &event).unwrap();
    assert!(
        validate_gate(
            &GateOutput {
                kind: GateKind::ProjectLesson,
                source_ids: vec!["turn-1".into()],
                reason: "No project exists".into(),
            },
            &event,
        )
        .is_err()
    );
    assert!(
        validate_gate(
            &GateOutput {
                kind: GateKind::None,
                source_ids: vec!["turn-1".into()],
                reason: "none".into(),
            },
            &event,
        )
        .is_err()
    );

    let invalid_extraction = ExtractionOutput {
        source_ids: vec!["turn-1".into(), "turn-2".into()],
        proof_source_ids: Vec::new(),
        proposed_title: "Rule".into(),
        gist: "Apply the rule".into(),
        rationale: "Observed proof".into(),
        priority: 2.0,
        novelty: 0.5,
        durability: 0.8,
        confidence: 0.9,
        retrieval_terms: vec!["rule".into()],
    };
    assert!(validate_extraction(&invalid_extraction, &valid_gate).is_err());
}

#[test]
fn candidate_identity_is_deterministic_across_source_order() {
    let room = RoomKey::new("lab").unwrap();
    let event = conversation_event(
        &room,
        None,
        &[("turn-1", "user", "rule"), ("turn-2", "assistant", "proof")],
    );
    let forward = candidate_id(
        &event,
        GateKind::CodingLesson,
        &["turn-1".into(), "turn-2".into()],
    )
    .unwrap();
    let reverse = candidate_id(
        &event,
        GateKind::CodingLesson,
        &["turn-2".into(), "turn-1".into()],
    )
    .unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 64);
}

#[test]
fn classifier_configuration_digest_includes_normalized_provider_base_path() {
    let first = OllamaConfig {
        endpoint: Url::parse("http://127.0.0.1:11435/route-a").unwrap(),
    };
    let second = OllamaConfig {
        endpoint: Url::parse("http://127.0.0.1:11435/route-b").unwrap(),
    };
    assert_ne!(
        configuration_digest(&first).unwrap(),
        configuration_digest(&second).unwrap()
    );
}
#[test]
fn extraction_schema_keeps_rationale_before_gist() {
    let schema = extraction_schema(&["turn-1".into()], true).unwrap();
    let properties = schema.split("\"properties\":").nth(1).unwrap();
    assert!(properties.find("\"rationale\"").unwrap() < properties.find("\"gist\"").unwrap());
}

#[test]
fn classifier_rationale_is_bounded_with_an_explicit_marker() {
    let source = "r".repeat(GIGA_MAX_STORED_RATIONALE_BYTES + 32);
    let stored = truncate_with_marker(
        &source,
        GIGA_MAX_STORED_RATIONALE_BYTES,
        GIGA_RATIONALE_TRUNCATION_MARKER,
    );
    assert!(stored.len() <= GIGA_MAX_STORED_RATIONALE_BYTES);
    assert!(stored.ends_with(GIGA_RATIONALE_TRUNCATION_MARKER));
}

#[tokio::test]
async fn pre_signaled_shutdown_exits_before_any_claim() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let config = Config {
        database_url: "unused".into(),
        embed_url: None,
        embed_model: "unused".into(),
        embed_dimension: 1,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: Some("lab".into()),
        house_tz: "America/Sao_Paulo".into(),
    };
    let (_shutdown, receiver) = watch::channel(true);
    tokio::time::timeout(
        Duration::from_millis(50),
        giga_worker_loop(pool, config, RoomKey::new("lab").unwrap(), receiver),
    )
    .await
    .expect("a pre-signaled shutdown must not wait for PostgreSQL");
}
