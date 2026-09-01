//! Conversation capture rules.
//!
//! Turn identity, freshness, transcript shape, and the dedupe marker are House
//! rules. Whoever owns the filesystem performs the reads and writes this module
//! describes; it never performs them itself and never reads a clock.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const TRANSCRIPT_DEBUG_LOG: &str = "athanor-transcript-debug.jsonl";
pub const TRANSCRIPT_LOG_DIRECTORY: &str = "logs";
pub const TURN_MARKER_PREFIX: &str = "athanor-turn-key";

pub const GIGA_SOURCE_LEDGER_DIRECTORY: &str = "giga-sources";
/// One visible harness message, already reduced to text by the adapter that
/// knows OMP's message shapes.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VisibleMessage {
    #[serde(default)]
    pub role: String,
    /// Identity supplied by the harness, when it supplies one at all.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: String,
    /// Source timestamp supplied by the harness, normalized to RFC3339.
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTurn {
    pub role: String,
    pub text: String,
    pub index: usize,
    pub message_id: String,
    pub has_visible_id: bool,
    pub has_stable_id: bool,
    pub source_timestamp: String,
    pub content_hash: String,
}

fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

/// Stable source identity for a turn the harness left anonymous. Index is
/// load-bearing: duplicate source ids null an entire event window, and index is
/// only safe because the whole append-only array is resent every turn.
fn derived_source_id(role: &str, index: usize, text: &str) -> String {
    let digest = sha256_hex(text);
    format!("omp-derived:{role}:{index}:{}", &digest[..32])
}

/// FNV-1a over UTF-16 code units, matching the marker keys already written into
/// existing room transcripts.
fn small_hash(value: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for character in value.chars() {
        let unit = if (character as u32) > 0xFFFF {
            0xD800 + (((character as u32) - 0x10000) >> 10)
        } else {
            character as u32
        };
        hash ^= unit & 0xFFFF;
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:x}")
}

/// Visible turns, in harness order, each carrying an identity and a source
/// timestamp. `captured_at` is an observation time and is only used where the
/// harness supplied nothing; it never overwrites a real value.
pub fn conversation_turns(messages: &[VisibleMessage], captured_at: &str) -> Vec<ConversationTurn> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            if message.role != "user" && message.role != "assistant" {
                return None;
            }
            let text = message.text.trim();
            if text.is_empty() {
                return None;
            }
            let visible = message
                .id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let source_timestamp = message
                .timestamp
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(captured_at);
            Some(ConversationTurn {
                message_id: visible
                    .map(str::to_owned)
                    .unwrap_or_else(|| derived_source_id(&message.role, index, text)),
                has_visible_id: visible.is_some(),
                // Always true: identity is derived when the harness omits one.
                // Kept explicit because GIGA validates it as an exact-source
                // contract.
                has_stable_id: true,
                content_hash: sha256_hex(text),
                role: message.role.clone(),
                text: text.to_owned(),
                index,
                source_timestamp: source_timestamp.to_owned(),
            })
        })
        .collect()
}

/// A conversation is fresh while at most one visible turn exists.
pub fn is_fresh_conversation(turns: &[ConversationTurn]) -> bool {
    turns.len() <= 1
}

pub fn turn_key(session_id: &str, turn: &ConversationTurn) -> String {
    format!(
        "{session_id}:{}:id:{}:{}",
        turn.role,
        turn.message_id,
        small_hash(&turn.text)
    )
}

pub fn turn_marker(key: &str) -> String {
    format!("<!-- {TURN_MARKER_PREFIX}: {key} -->")
}

fn join_path(base: &str, tail: &str) -> String {
    let separator = if base.contains('\\') && !base.contains('/') {
        '\\'
    } else {
        '/'
    };
    format!("{}{separator}{tail}", base.trim_end_matches(['/', '\\']))
}

pub fn transcript_path(room_dir: &str, date_stamp: &str) -> String {
    join_path(room_dir, &format!("conversation_log_{date_stamp}.md"))
}

pub fn transcript_debug_path(room_dir: &str) -> String {
    join_path(
        &join_path(room_dir, TRANSCRIPT_LOG_DIRECTORY),
        TRANSCRIPT_DEBUG_LOG,
    )
}

/// Who a turn is attributed to in the room transcript.
pub fn turn_label<'a>(role: &str, operator: &'a str, spirit: &'a str) -> &'a str {
    if role == "user" { operator } else { spirit }
}

/// The exact bytes to append for one turn, or `None` when the marker shows the
/// turn is already durable in this transcript.
pub fn transcript_entry(
    existing: &str,
    marker: &str,
    date_stamp: &str,
    clock: &str,
    label: &str,
    text: &str,
) -> Option<String> {
    if existing.contains(marker) {
        return None;
    }
    let header = if existing.trim().is_empty() {
        [
            format!("# Conversation log — {date_stamp}"),
            String::new(),
            "Append-only raw-ish transcript captured by The Athanor OMP extension.".into(),
            String::new(),
            "---".into(),
            String::new(),
        ]
        .join("\n")
    } else {
        String::new()
    };
    let separator = if !existing.is_empty() && !existing.ends_with("\n\n") {
        "\n\n"
    } else {
        ""
    };
    Some(format!(
        "{separator}{header}{marker}\n## {clock} — {label}\n\n{text}\n\n"
    ))
}

/// The record GIGA ingests for one durable turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LoggedTurn {
    pub role: String,
    pub text: String,
    #[serde(rename = "sourceID")]
    pub source_id: String,
    pub content_hash: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub source_timestamp: String,
    #[serde(rename = "hasStableID")]
    pub has_stable_id: bool,
}

pub fn logged_turn(turn: &ConversationTurn, session_id: &str) -> LoggedTurn {
    LoggedTurn {
        role: turn.role.clone(),
        text: turn.text.clone(),
        source_id: turn.message_id.clone(),
        content_hash: turn.content_hash.clone(),
        session_id: session_id.to_owned(),
        source_timestamp: turn.source_timestamp.clone(),
        has_stable_id: true,
    }
}

/// Private, append-only source records used to verify GIGA event pointers.
/// The directory is room-local so Host capture and substrate workers share one
/// authority without a legacy home-directory topology.
pub fn source_ledger_directory_path(room_dir: &Path) -> PathBuf {
    room_dir
        .join(".omp")
        .join("runtime")
        .join(GIGA_SOURCE_LEDGER_DIRECTORY)
}

pub fn source_ledger_directory(room_dir: &str) -> String {
    source_ledger_directory_path(Path::new(room_dir))
        .to_string_lossy()
        .into_owned()
}

pub fn source_ledger_path(room_dir: &str, date_stamp: &str) -> String {
    source_ledger_directory_path(Path::new(room_dir))
        .join(format!("{date_stamp}.jsonl"))
        .to_string_lossy()
        .into_owned()
}

#[derive(Deserialize, Serialize)]
struct SourceLedgerRecord {
    #[serde(rename = "sessionID")]
    session_id: String,
    #[serde(rename = "messageID")]
    message_id: String,
    role: String,
    text: String,
}

impl From<&LoggedTurn> for SourceLedgerRecord {
    fn from(turn: &LoggedTurn) -> Self {
        Self {
            session_id: turn.session_id.clone(),
            message_id: turn.source_id.clone(),
            role: turn.role.clone(),
            text: turn.text.clone(),
        }
    }
}

/// Return one JSONL record when the source is absent. An existing exact record
/// is idempotent; reusing the same session/message identity for different
/// content is refused rather than creating an ambiguous GIGA source.
pub fn source_ledger_entry(existing: &str, turn: &LoggedTurn) -> Result<Option<String>, String> {
    let wanted = SourceLedgerRecord::from(turn);
    for line in existing.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<SourceLedgerRecord>(line) else {
            continue;
        };
        if record.session_id == wanted.session_id && record.message_id == wanted.message_id {
            return if record.role == wanted.role && record.text == wanted.text {
                Ok(None)
            } else {
                Err(format!(
                    "source identity conflict for session {} message {}",
                    wanted.session_id, wanted.message_id
                ))
            };
        }
    }
    serde_json::to_string(&wanted)
        .map(|record| Some(format!("{record}\n")))
        .map_err(|error| format!("source ledger serialization failed: {error}"))
}
