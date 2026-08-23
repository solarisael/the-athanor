use crate::Config;
use hearth::{
    GIGA_MAX_PROCESS_SOURCE_BYTES, GIGA_MAX_PROCESS_SOURCES, GIGA_MAX_PROCESS_WINDOW_BYTES,
    GigaEvent, GigaEventType, GigaSourceRef, GigaSourceType, GigaVisibility,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use tokio::fs;
use super::failure::{WorkerFailure, WorkerFailureKind};
use super::identity::sha256_bytes;

#[derive(Clone, Debug)]
pub(super) struct ResolvedSource {
    pub(super) source: GigaSourceRef,
    pub(super) text: String,
}

#[derive(Clone, Deserialize)]
struct LedgerSourceRecord {
    #[serde(rename = "sessionID")]
    session_id: Option<String>,
    #[serde(rename = "messageID")]
    message_id: Option<Value>,
    role: Option<String>,
    text: Option<String>,
}

pub(super) fn verify_event_sources(
    event: &GigaEvent,
    trusted_room: &str,
) -> Result<(), WorkerFailure> {
    if event.event_type() != GigaEventType::ConversationWindow
        || event.room().as_str() != trusted_room
        || event.project_keys().len() > 1
        || event.source_refs().is_empty()
        || event.source_refs().len() > GIGA_MAX_PROCESS_SOURCES
    {
        return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
    }
    let expected_project = event.project_keys().first().map(String::as_str);
    for (index, source) in event.source_refs().iter().enumerate() {
        if event.source_refs()[..index]
            .iter()
            .any(|known| known.source_id() == source.source_id())
            || source.source_type() != GigaSourceType::Turn
            || !matches!(source.role(), "user" | "assistant")
            || source.scope().visibility() != GigaVisibility::Private
            || source.scope().room() != Some(event.room())
            || source.scope().project() != expected_project
            || !source.scope().publication_review_required()
            || source.range().is_some()
        {
            return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
        }
    }
    Ok(())
}

fn is_conversation_ledger(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 16
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && &bytes[10..] == b".jsonl"
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

fn ledger_source_id(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) async fn resolve_sources_from_ledger(
    config: &Config,
    event: &GigaEvent,
) -> Result<Vec<ResolvedSource>, WorkerFailure> {
    let trusted_room = config
        .giga_source_room
        .as_deref()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
    verify_event_sources(event, trusted_room)?;
    let directory = config
        .giga_source_ledger_dir
        .as_deref()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
    let mut entries = fs::read_dir(directory)
        .await
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
    let mut paths = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
        let name = entry.file_name();
        if file_type.is_file() && name.to_str().is_some_and(is_conversation_ledger) {
            paths.push(entry.path());
        }
    }
    paths.sort();

    let wanted = event
        .source_refs()
        .iter()
        .enumerate()
        .map(|(index, source)| (source.source_id().to_owned(), index))
        .collect::<HashMap<_, _>>();
    let mut matches = vec![Vec::<LedgerSourceRecord>::new(); event.source_refs().len()];
    for path in paths {
        let contents = fs::read_to_string(path)
            .await
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
        for line in contents.lines().filter(|line| !line.is_empty()) {
            let Ok(record) = serde_json::from_str::<LedgerSourceRecord>(line) else {
                continue;
            };
            let Some(source_id) = record.message_id.as_ref().and_then(ledger_source_id) else {
                continue;
            };
            let Some(&index) = wanted.get(&source_id) else {
                continue;
            };
            if record.session_id.as_deref() == Some(event.session_id()) {
                matches[index].push(record);
            }
        }
    }

    let mut total_bytes = 0usize;
    event
        .source_refs()
        .iter()
        .zip(matches)
        .map(|(source, mut records)| {
            if records.is_empty() {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceMissing));
            }
            if records.len() != 1 {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceAmbiguous));
            }
            let record = records.pop().expect("one ledger record was checked");
            if record.role.as_deref() != Some(source.role()) {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
            }
            let text = record
                .text
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
                .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::SourceVerification))?;
            if sha256_bytes(text.as_bytes()) != source.content_hash() {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceHashMismatch));
            }
            total_bytes = total_bytes
                .checked_add(text.len())
                .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::SourceWindowTooLarge))?;
            if text.len() > GIGA_MAX_PROCESS_SOURCE_BYTES
                || total_bytes > GIGA_MAX_PROCESS_WINDOW_BYTES
            {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceWindowTooLarge));
            }
            Ok(ResolvedSource {
                source: source.clone(),
                text,
            })
        })
        .collect()
}
