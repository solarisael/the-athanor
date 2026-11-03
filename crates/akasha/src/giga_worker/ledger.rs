use super::bounds::truncate_with_marker;
use super::failure::{WorkerFailure, WorkerFailureKind};
use super::identity::sha256_bytes;
use crate::{Config, typesafe::is_secret};
use hearth::{
    GIGA_MAX_PROCESS_SOURCE_BYTES, GIGA_MAX_PROCESS_SOURCES, GIGA_MAX_PROCESS_WINDOW_BYTES,
    GigaEvent, GigaEventType, GigaSourceRef, GigaSourceType, GigaVisibility,
};
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
};

/// Most nearby distinct turns of the target's session sent as scoring context.
pub(super) const GIGA_MAX_CONTEXT_TURNS: usize = 7;
const CONTEXT_TURN_MAX_BYTES: usize = 1_200;

/// A nearby turn of the target's room and session. It is scoring context only:
/// it carries no source ID, so it can never become a candidate source or proof.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ContextTurn {
    /// Distance in distinct session turns: negative before the target, positive after.
    pub(super) offset: i64,
    pub(super) role: String,
    pub(super) text: String,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedSource {
    pub(super) source: GigaSourceRef,
    pub(super) text: String,
    pub(super) context: Vec<ContextTurn>,
    /// Nearby turns left out because their identity conflicts or their text looks secret.
    pub(super) withheld_context: usize,
}

#[derive(Deserialize)]
struct LedgerSession<'a> {
    #[serde(rename = "sessionID", borrow)]
    session_id: Option<Cow<'a, str>>,
}

#[derive(Deserialize)]
struct LedgerSourceRecord {
    #[serde(rename = "messageID")]
    message_id: Option<Value>,
    role: Option<String>,
    text: Option<String>,
}

/// One distinct turn of a session, at its first ledger position. An exact
/// repeat (same role and full stored text) collapses into it; any other repeat
/// marks the identity as conflicted.
struct SessionTurn {
    role: Option<String>,
    text: Option<String>,
    conflicted: bool,
}

/// A session's distinct turns in ledger order, with each source ID's position.
struct SessionLedger {
    turns: Vec<SessionTurn>,
    positions: HashMap<String, usize>,
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

fn ledger_unavailable(_: std::io::Error) -> WorkerFailure {
    WorkerFailure::new(WorkerFailureKind::LedgerUnavailable)
}

/// Every distinct turn of one session, in ledger order across the dated files.
async fn session_ledger(
    directory: &Path,
    session_id: &str,
) -> Result<SessionLedger, WorkerFailure> {
    let mut entries = fs::read_dir(directory).await.map_err(ledger_unavailable)?;
    let mut paths = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(ledger_unavailable)? {
        let file_type = entry.file_type().await.map_err(ledger_unavailable)?;
        let name = entry.file_name();
        if file_type.is_file() && name.to_str().is_some_and(is_conversation_ledger) {
            paths.push(entry.path());
        }
    }
    paths.sort();

    let mut ledger = SessionLedger {
        turns: Vec::new(),
        positions: HashMap::new(),
    };
    for path in paths {
        let file = fs::File::open(path).await.map_err(ledger_unavailable)?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        loop {
            line.clear();
            if reader
                .read_line(&mut line)
                .await
                .map_err(ledger_unavailable)?
                == 0
            {
                break;
            }
            let Ok(session) = serde_json::from_str::<LedgerSession<'_>>(&line) else {
                continue;
            };
            if session.session_id.as_deref() != Some(session_id) {
                continue;
            }
            let Ok(record) = serde_json::from_str::<LedgerSourceRecord>(&line) else {
                continue;
            };
            let Some(source_id) = record.message_id.as_ref().and_then(ledger_source_id) else {
                continue;
            };
            if let Some(&index) = ledger.positions.get(&source_id) {
                let turn = &mut ledger.turns[index];
                if turn.role != record.role || turn.text != record.text {
                    turn.conflicted = true;
                }
                continue;
            }
            ledger.positions.insert(source_id, ledger.turns.len());
            ledger.turns.push(SessionTurn {
                role: record.role,
                text: record.text,
                conflicted: false,
            });
        }
    }
    Ok(ledger)
}

/// The nearest usable turns around `target`, closest first and earlier on a tie,
/// returned in conversation order with the count of turns withheld on the way.
fn nearby_turns(turns: &[SessionTurn], target: usize, limit: usize) -> (Vec<ContextTurn>, usize) {
    let mut selected = Vec::with_capacity(limit);
    let mut withheld = 0;
    let mut distance = 1;
    while selected.len() < limit && (distance <= target || target + distance < turns.len()) {
        let before = target.checked_sub(distance);
        let after = Some(target + distance).filter(|index| *index < turns.len());
        for index in [before, after].into_iter().flatten() {
            if selected.len() == limit {
                break;
            }
            let turn = &turns[index];
            let Some(role) = turn
                .role
                .as_deref()
                .filter(|role| matches!(*role, "user" | "assistant"))
            else {
                continue;
            };
            let Some(text) = turn
                .text
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            else {
                continue;
            };
            if turn.conflicted || is_secret(text) {
                withheld += 1;
                continue;
            }
            selected.push(ContextTurn {
                offset: index as i64 - target as i64,
                role: role.to_owned(),
                text: truncate_with_marker(text, CONTEXT_TURN_MAX_BYTES, " [turn truncated]"),
            });
        }
        distance += 1;
    }
    selected.sort_by_key(|turn| turn.offset);
    (selected, withheld)
}

/// Each event source with its exact ledger text and nearby session turns.
pub(super) async fn resolve_sources_from_ledger(
    config: &Config,
    event: &GigaEvent,
) -> Result<Vec<ResolvedSource>, WorkerFailure> {
    resolve_sources(config, event, event.source_refs(), GIGA_MAX_CONTEXT_TURNS).await
}

/// Selected event sources with their exact ledger text and no nearby turns.
pub(super) async fn resolve_candidate_sources(
    config: &Config,
    event: &GigaEvent,
    source_refs: &[GigaSourceRef],
) -> Result<Vec<ResolvedSource>, WorkerFailure> {
    resolve_sources(config, event, source_refs, 0).await
}

async fn resolve_sources(
    config: &Config,
    event: &GigaEvent,
    source_refs: &[GigaSourceRef],
    context_limit: usize,
) -> Result<Vec<ResolvedSource>, WorkerFailure> {
    if source_refs.is_empty()
        || source_refs.iter().enumerate().any(|(index, source)| {
            !event.source_refs().contains(source) || source_refs[..index].contains(source)
        })
    {
        return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
    }
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
    let ledger = session_ledger(directory, event.session_id()).await?;

    let mut total_bytes = 0usize;
    source_refs
        .iter()
        .map(|source| {
            let index = *ledger
                .positions
                .get(source.source_id())
                .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::SourceMissing))?;
            let turn = &ledger.turns[index];
            if turn.conflicted {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceAmbiguous));
            }
            if turn.role.as_deref() != Some(source.role()) {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
            }
            let text = turn
                .text
                .as_deref()
                .map(str::trim)
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
            let (context, withheld_context) = nearby_turns(&ledger.turns, index, context_limit);
            Ok(ResolvedSource {
                source: source.clone(),
                text: text.to_owned(),
                context,
                withheld_context,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
