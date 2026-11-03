use super::*;
use crate::EmbeddingMode;
use hearth::{GigaLifecycle, GigaScope, RoomKey};
use serde_json::json;
use std::path::PathBuf;
use uuid::Uuid;

fn ledger_config(directory: &Path, room: &RoomKey) -> Config {
    Config {
        database_url: "unused".into(),
        embed_url: None,
        embed_model: "unused".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: Some(directory.to_path_buf()),
        giga_source_room: Some(room.to_string()),
        house_tz: "UTC".into(),
        nats_url: None,
    }
}

fn turn_ref(room: &RoomKey, id: &str, role: &str, hash: String) -> GigaSourceRef {
    let scope =
        GigaScope::new(Some(room.to_string()), None, GigaVisibility::Private, true).unwrap();
    GigaSourceRef::new(
        GigaSourceType::Turn,
        id.into(),
        role.into(),
        "2030-01-01T00:00:00Z".into(),
        hash,
        scope,
        None,
    )
    .unwrap()
}

fn event_for(room: &RoomKey, session: &str, sources: Vec<GigaSourceRef>) -> GigaEvent {
    GigaEvent::new(
        format!("{session}-event"),
        GigaEventType::ConversationWindow,
        room.clone(),
        session.into(),
        Vec::new(),
        sources,
        GigaLifecycle::conversation_window(),
        "2030-01-01T00:00:00Z".into(),
    )
    .unwrap()
}

fn ledger_line(session: &str, message: &str, role: &str, text: &str) -> String {
    json!({"sessionID": session, "messageID": message, "role": role, "text": text}).to_string()
}

async fn ledger_directory(label: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("giga-ledger-{label}-{}", Uuid::new_v4()));
    fs::create_dir_all(&directory).await.unwrap();
    directory
}

async fn write_day(directory: &Path, day: &str, lines: &[String]) {
    fs::write(directory.join(day), format!("{}\n", lines.join("\n")))
        .await
        .unwrap();
}

#[tokio::test]
async fn candidate_sources_survive_a_missing_peer_without_accepting_forged_evidence() {
    let room = RoomKey::new("source-proof").unwrap();
    let text = "The operator chose Portuguese for future sessions.";
    let kept = turn_ref(&room, "kept", "user", sha256_bytes(text.as_bytes()));
    let missing = turn_ref(
        &room,
        "missing",
        "user",
        sha256_bytes(b"A different turn in the same batch."),
    );
    let forged = turn_ref(&room, "kept", "user", "f".repeat(64));
    let event = event_for(&room, "source-proof-session", vec![kept.clone(), missing]);
    let directory = ledger_directory("source-proof").await;
    let record = |body: &str| ledger_line("source-proof-session", "kept", "user", body);
    write_day(&directory, "2030-01-01.jsonl", &[record(text)]).await;
    let config = ledger_config(&directory, &room);

    let whole_batch = resolve_sources_from_ledger(&config, &event).await;
    let selected = resolve_candidate_sources(&config, &event, std::slice::from_ref(&kept)).await;
    let foreign = resolve_candidate_sources(&config, &event, &[forged]).await;
    let duplicate = resolve_candidate_sources(&config, &event, &[kept.clone(), kept.clone()]).await;
    write_day(
        &directory,
        "2030-01-01.jsonl",
        &[record("The stored source was changed.")],
    )
    .await;
    let changed = resolve_candidate_sources(&config, &event, std::slice::from_ref(&kept)).await;
    fs::remove_dir_all(directory).await.unwrap();

    assert_eq!(whole_batch.unwrap_err().class(), "GigaSourceMissingError");
    let selected = selected.unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].source, kept);
    assert_eq!(selected[0].text, text);
    assert_eq!(foreign.unwrap_err().class(), "GigaSourceVerificationError");
    assert_eq!(
        duplicate.unwrap_err().class(),
        "GigaSourceVerificationError"
    );
    assert_eq!(changed.unwrap_err().class(), "GigaSourceHashMismatchError");
}

#[tokio::test]
async fn exact_ledger_repeats_collapse_but_any_conflicting_row_refuses() {
    let room = RoomKey::new("ledger-repeat").unwrap();
    let session = "repeat-session";
    let text = "Sol chose PostgreSQL to own GIGA batch release.";
    let source = turn_ref(&room, "repeat-turn", "user", sha256_bytes(text.as_bytes()));
    let event = event_for(&room, session, vec![source.clone()]);
    let directory = ledger_directory("repeat").await;
    let config = ledger_config(&directory, &room);
    let row =
        |session: &str, role: &str, body: &str| ledger_line(session, "repeat-turn", role, body);
    // The observed failure: one turn logged again on the next day.
    write_day(
        &directory,
        "2030-01-01.jsonl",
        &[row(session, "user", text), row(session, "user", text)],
    )
    .await;
    let second_day = |extra: String| {
        vec![
            row(session, "user", text),
            row(
                "other-session",
                "assistant",
                "Another session may reuse the message ID.",
            ),
            extra,
        ]
    };
    write_day(
        &directory,
        "2030-01-02.jsonl",
        &second_day(row(session, "user", text)),
    )
    .await;
    let repeated = resolve_sources_from_ledger(&config, &event).await;
    // Trimmed text still hashes to the pointer, but the full stored text differs.
    write_day(
        &directory,
        "2030-01-02.jsonl",
        &second_day(row(session, "user", &format!("{text}\n"))),
    )
    .await;
    let padded = resolve_sources_from_ledger(&config, &event).await;
    write_day(
        &directory,
        "2030-01-02.jsonl",
        &second_day(row(session, "assistant", text)),
    )
    .await;
    let role_conflict = resolve_sources_from_ledger(&config, &event).await;
    write_day(
        &directory,
        "2030-01-02.jsonl",
        &second_day(row(
            session,
            "user",
            "A rewritten turn under the same identity.",
        )),
    )
    .await;
    let text_conflict = resolve_sources_from_ledger(&config, &event).await;
    fs::remove_dir_all(directory).await.unwrap();

    let repeated = repeated.unwrap();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].source, source);
    assert_eq!(repeated[0].text, text);
    for refused in [padded, role_conflict, text_conflict] {
        assert_eq!(refused.unwrap_err().class(), "GigaSourceAmbiguousError");
    }
}

#[tokio::test]
async fn nearby_turns_are_bounded_session_context_without_source_identity() {
    let room = RoomKey::new("ledger-context").unwrap();
    let session = "context-session";
    let text_of = |index: usize| format!("Session turn {index} discusses the lighthouse schedule.");
    let role_of = |index: usize| if index % 2 == 0 { "user" } else { "assistant" };
    let target_text = text_of(6);
    let target = turn_ref(
        &room,
        "turn-6",
        "user",
        sha256_bytes(target_text.as_bytes()),
    );
    let event = event_for(&room, session, vec![target.clone()]);
    let mut first_day = Vec::new();
    for index in 0..12 {
        let text = match index {
            3 => "A system notice is not a conversation turn.".to_owned(),
            4 => "The deploy password: hunter2 stays local.".to_owned(),
            _ => text_of(index),
        };
        let role = if index == 3 { "system" } else { role_of(index) };
        first_day.push(ledger_line(session, &format!("turn-{index}"), role, &text));
        first_day.push(ledger_line(
            "other-session",
            &format!("other-{index}"),
            "user",
            "A different session must never leak into this context.",
        ));
    }
    let directory = ledger_directory("context").await;
    write_day(&directory, "2030-01-01.jsonl", &first_day).await;
    write_day(
        &directory,
        "2030-01-02.jsonl",
        &[
            ledger_line(session, "turn-5", role_of(5), &text_of(5)),
            ledger_line(
                session,
                "turn-7",
                role_of(7),
                "A conflicting rewrite of turn seven.",
            ),
        ],
    )
    .await;
    let config = ledger_config(&directory, &room);

    let resolved = resolve_sources_from_ledger(&config, &event).await;
    let candidate = resolve_candidate_sources(&config, &event, std::slice::from_ref(&target)).await;
    fs::remove_dir_all(directory).await.unwrap();

    let resolved = resolved.unwrap();
    assert_eq!(resolved.len(), 1);
    let source = &resolved[0];
    assert_eq!(source.text, target_text);
    // Closest first: 5, 7 (conflict), 4 (secret), 8, 3 (system), 9, 2, 10, 1, 11.
    let offsets = source
        .context
        .iter()
        .map(|turn| turn.offset)
        .collect::<Vec<_>>();
    assert_eq!(offsets, vec![-5, -4, -1, 2, 3, 4, 5]);
    assert_eq!(source.context.len(), GIGA_MAX_CONTEXT_TURNS);
    assert_eq!(source.withheld_context, 2);
    for turn in &source.context {
        let index = usize::try_from(6 + turn.offset).unwrap();
        assert_eq!(turn.text, text_of(index));
        assert_eq!(turn.role, role_of(index));
    }
    let candidate = candidate.unwrap();
    assert!(candidate[0].context.is_empty());
    assert_eq!(candidate[0].withheld_context, 0);
}
