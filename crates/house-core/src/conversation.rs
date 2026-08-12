//! Conversation capture rules.
//!
//! Turn identity, freshness, transcript shape, and the dedupe marker are House
//! rules. Whoever owns the filesystem performs the reads and writes this module
//! describes; it never performs them itself and never reads a clock.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TRANSCRIPT_DEBUG_LOG: &str = "solarisael-house-transcript-debug.jsonl";
pub const TRANSCRIPT_LOG_DIRECTORY: &str = "logs";
pub const TURN_MARKER_PREFIX: &str = "solarisael-turn-key";

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
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, text: &str) -> VisibleMessage {
        VisibleMessage {
            role: role.into(),
            text: text.into(),
            ..VisibleMessage::default()
        }
    }

    #[test]
    fn derives_a_distinct_stable_identity_per_anonymous_turn() {
        let messages = vec![message("user", "ok"), message("assistant", "ok")];
        let first = conversation_turns(&messages, "2026-08-12T00:00:00.000Z");
        let second = conversation_turns(&messages, "2026-08-12T01:00:00.000Z");

        assert_eq!(first[0].message_id, second[0].message_id);
        assert_ne!(first[0].message_id, first[1].message_id);
        assert!(!first[0].has_visible_id);
        assert!(first[0].has_stable_id);
        assert_eq!(first[0].source_timestamp, "2026-08-12T00:00:00.000Z");
    }

    #[test]
    fn harness_identity_and_timestamp_win() {
        let turns = conversation_turns(
            &[VisibleMessage {
                role: "user".into(),
                id: Some("harness-uuid-7".into()),
                text: "Stamped.".into(),
                timestamp: Some("2026-07-24T12:00:00.000Z".into()),
            }],
            "2026-08-12T00:00:00.000Z",
        );

        assert_eq!(turns[0].message_id, "harness-uuid-7");
        assert!(turns[0].has_visible_id);
        assert_eq!(turns[0].source_timestamp, "2026-07-24T12:00:00.000Z");
    }

    #[test]
    fn a_marked_turn_is_never_appended_twice() {
        let turns = conversation_turns(&[message("user", "Hello.")], "2026-08-12T00:00:00.000Z");
        let marker = turn_marker(&turn_key("session-1", &turns[0]));
        let first = transcript_entry("", &marker, "2026-08-12", "09:41", "Sol", "Hello.")
            .expect("a new turn appends");

        assert!(first.starts_with("# Conversation log — 2026-08-12"));
        assert!(
            transcript_entry(&first, &marker, "2026-08-12", "09:41", "Sol", "Hello.").is_none()
        );
    }
}
