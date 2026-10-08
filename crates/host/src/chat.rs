//! The room conversation ring the chat projection serves.
//!
//! One Host serves one room, so this is a single bounded log. The Host stamps
//! sequence and time; a say is idempotent on its say id and a turn on its
//! turn id, so surface retries and doorman re-reports never produce twin
//! lines. The bounded projection survives Host restarts; the complete durable
//! conversation record stays with the shell conversation log.
//!
//! Beside the ring sit the drafts: the spirit side of says still being
//! answered. A draft is replaced whole on every report and retired by the
//! settled turn, whose final content is authoritative.

use protocol::{ChatAuthor, ChatDraft, ChatMessage, ChatOutcome, ChatStep};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub const CHAT_MAX_ENTRIES: usize = 256;
pub const CHAT_MAX_TEXT_CHARS: usize = 32_768;
// One doorman answers one say at a time, so one open draft is the normal
// count; the bounds only cap a misbehaving adapter, never a real conversation.
const CHAT_MAX_DRAFTS: usize = 8;
const CHAT_MAX_STEPS: usize = 64;
const CHAT_MAX_THINKING_BLOCKS: usize = 64;
const CHAT_MAX_THINKING_CHARS: usize = 32_768;

#[derive(Default)]
pub struct ChatLog {
    entries: VecDeque<ChatMessage>,
    drafts: Vec<ChatDraft>,
    next_sequence: u64,
    draft_generation: u64,
    path: Option<PathBuf>,
}

// Final turns retire one draft generation; streaming updates never rewrite the history ring.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatCheckpoint<Messages, DraftIds> {
    entries: Messages,
    draft_ids: DraftIds,
    next_sequence: u64,
    draft_generation: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftCheckpoint<Drafts> {
    generation: u64,
    drafts: Drafts,
}

fn read_checkpoint<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("cannot read chat projection {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(format!(
            "cannot read chat projection {}: {error}",
            path.display()
        )),
    }
}

impl ChatLog {
    pub fn open(path: &Path) -> Result<Self, String> {
        let checkpoint: ChatCheckpoint<VecDeque<ChatMessage>, Vec<String>> = read_checkpoint(path)?;
        let mut draft_checkpoint: DraftCheckpoint<Vec<ChatDraft>> =
            read_checkpoint(&path.with_file_name("chat-drafts.json"))?;
        if draft_checkpoint.generation < checkpoint.draft_generation {
            return Err("chat draft checkpoint is older than the conversation".into());
        }
        if draft_checkpoint.generation == checkpoint.draft_generation {
            if checkpoint.draft_ids.iter().any(|id| {
                !draft_checkpoint
                    .drafts
                    .iter()
                    .any(|draft| &draft.turn_id == id)
            }) {
                return Err("chat draft checkpoint is missing an active draft".into());
            }
            draft_checkpoint
                .drafts
                .retain(|draft| checkpoint.draft_ids.contains(&draft.turn_id));
        }
        let log = Self {
            entries: checkpoint.entries,
            drafts: draft_checkpoint.drafts,
            next_sequence: checkpoint.next_sequence,
            draft_generation: draft_checkpoint.generation,
            path: Some(path.to_owned()),
        };
        if log.entries.len() > CHAT_MAX_ENTRIES
            || log.drafts.len() > CHAT_MAX_DRAFTS
            || log
                .entries
                .back()
                .is_some_and(|entry| entry.sequence >= log.next_sequence)
        {
            return Err(format!("invalid chat projection {}", path.display()));
        }
        Ok(log)
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let checkpoint = ChatCheckpoint {
            entries: &self.entries,
            draft_ids: self
                .drafts
                .iter()
                .map(|draft| draft.turn_id.as_str())
                .collect::<Vec<_>>(),
            next_sequence: self.next_sequence,
            draft_generation: self.draft_generation,
        };
        crate::store::atomic_json_write(path, &checkpoint)
    }

    fn persist_drafts(&mut self) -> Result<(), String> {
        let generation = self
            .draft_generation
            .checked_add(1)
            .ok_or_else(|| "chat draft generation exhausted".to_owned())?;
        if let Some(path) = &self.path {
            let checkpoint = DraftCheckpoint {
                generation,
                drafts: &self.drafts,
            };
            crate::store::atomic_json_write(&path.with_file_name("chat-drafts.json"), &checkpoint)?;
        }
        self.draft_generation = generation;
        Ok(())
    }

    /// Append one operator line. `None` means the say id already answered —
    /// a retry, not a new message.
    pub fn say(
        &mut self,
        author_name: &str,
        text: &str,
        say_id: &str,
        at: String,
    ) -> Result<Option<ChatMessage>, String> {
        self.append(
            ChatAuthor::Operator,
            author_name,
            text,
            say_id,
            vec![],
            vec![],
            ChatOutcome::Complete,
            at,
        )
    }

    /// Append one spirit line for a settled turn. `None` means this turn id
    /// already reported its spirit side. The turn's draft retires with it;
    /// the line carries the steps the turn itself brings.
    pub fn turn(
        &mut self,
        author_name: &str,
        text: &str,
        turn_id: &str,
        steps: Vec<ChatStep>,
        thinking: Vec<String>,
        outcome: ChatOutcome,
        at: String,
    ) -> Result<Option<ChatMessage>, String> {
        let retired = self.retire_draft(turn_id);
        let result = self.append(
            ChatAuthor::Spirit,
            author_name,
            text,
            turn_id,
            bounded_steps(steps),
            thinking,
            outcome,
            at,
        );
        if result.is_err() || matches!(&result, Ok(None)) {
            if let Some((index, draft)) = retired {
                self.drafts.insert(index, draft);
            }
        }
        result
    }

    /// Replace the draft for one turn. `None` means the turn already settled,
    /// so a late report changes nothing.
    pub fn draft(
        &mut self,
        author_name: &str,
        text: &str,
        turn_id: &str,
        steps: Vec<ChatStep>,
        thinking: Vec<String>,
        at: String,
    ) -> Result<Option<ChatDraft>, String> {
        if self
            .entries
            .iter()
            .any(|entry| entry.author == ChatAuthor::Spirit && entry.turn_id == turn_id)
        {
            return Ok(None);
        }
        let retired = self.retire_draft(turn_id);
        let draft = ChatDraft {
            turn_id: turn_id.to_owned(),
            author_name: author_name.to_owned(),
            text: bounded_text(text),
            steps: bounded_steps(steps),
            thinking: bounded_thinking(thinking),
            at,
        };
        self.drafts.push(draft.clone());
        let evicted = if self.drafts.len() > CHAT_MAX_DRAFTS {
            Some(self.drafts.remove(0))
        } else {
            None
        };
        if let Err(error) = self.persist_drafts() {
            self.drafts.pop();
            if let Some(draft) = evicted {
                self.drafts.insert(0, draft);
            }
            if let Some((index, draft)) = retired {
                self.drafts.insert(index, draft);
            }
            return Err(error);
        }
        Ok(Some(draft))
    }

    pub fn snapshot(&self) -> Vec<ChatMessage> {
        self.entries.iter().cloned().collect()
    }

    pub fn drafts(&self) -> Vec<ChatDraft> {
        self.drafts.clone()
    }

    fn retire_draft(&mut self, turn_id: &str) -> Option<(usize, ChatDraft)> {
        let index = self
            .drafts
            .iter()
            .position(|draft| draft.turn_id == turn_id)?;
        Some((index, self.drafts.remove(index)))
    }

    pub(crate) fn summary(&self) -> (usize, Option<&str>) {
        (
            self.entries.len(),
            self.entries.back().map(|message| message.at.as_str()),
        )
    }

    fn append(
        &mut self,
        author: ChatAuthor,
        author_name: &str,
        text: &str,
        turn_id: &str,
        steps: Vec<ChatStep>,
        thinking: Vec<String>,
        outcome: ChatOutcome,
        at: String,
    ) -> Result<Option<ChatMessage>, String> {
        if self
            .entries
            .iter()
            .any(|entry| entry.author == author && entry.turn_id == turn_id)
        {
            return Ok(None);
        }
        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| "chat sequence exhausted".to_owned())?;
        let message = ChatMessage {
            sequence: self.next_sequence,
            author,
            author_name: author_name.to_owned(),
            text: bounded_text(text),
            at,
            turn_id: turn_id.to_owned(),
            steps,
            thinking: bounded_thinking(thinking),
            outcome,
        };
        self.next_sequence = next_sequence;
        self.entries.push_back(message.clone());
        let evicted = if self.entries.len() > CHAT_MAX_ENTRIES {
            self.entries.pop_front()
        } else {
            None
        };
        if let Err(error) = self.persist() {
            self.entries.pop_back();
            if let Some(entry) = evicted {
                self.entries.push_front(entry);
            }
            self.next_sequence -= 1;
            return Err(error);
        }
        Ok(Some(message))
    }
}

fn bounded_text(text: &str) -> String {
    if text.chars().count() <= CHAT_MAX_TEXT_CHARS {
        return text.to_owned();
    }
    text.chars().take(CHAT_MAX_TEXT_CHARS).collect()
}

fn bounded_steps(mut steps: Vec<ChatStep>) -> Vec<ChatStep> {
    steps.truncate(CHAT_MAX_STEPS);
    steps
}

fn bounded_thinking(mut blocks: Vec<String>) -> Vec<String> {
    blocks.truncate(CHAT_MAX_THINKING_BLOCKS);
    let mut remaining = CHAT_MAX_THINKING_CHARS;
    blocks.retain_mut(|block| {
        if remaining == 0 {
            return false;
        }

        let mut chars = block.char_indices();
        let mut kept = 0;
        for _ in 0..remaining {
            if chars.next().is_none() {
                break;
            }
            kept += 1;
        }
        let end = chars.next().map(|(index, _)| index);
        if let Some(end) = end {
            block.truncate(end);
        }
        remaining -= kept;
        !block.is_empty()
    });
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> String {
        "2026-08-28T12:00:00Z".to_owned()
    }

    fn step(id: &str) -> ChatStep {
        ChatStep {
            tool_call_id: id.to_owned(),
            tool: "read".to_owned(),
            summary: "the map".to_owned(),
            status: protocol::ChatStepStatus::Ok,
            started_at: now(),
            elapsed_ms: Some(12),
        }
    }

    #[test]
    fn a_spirit_line_takes_the_sequence_after_the_operator_line_before_it() {
        let mut log = ChatLog::default();
        let say = log
            .say("Sol", "hello dragon", "say-1", now())
            .unwrap()
            .unwrap();
        let turn = log
            .turn(
                "Kodo",
                "thump thump",
                "turn-1",
                vec![],
                vec![],
                ChatOutcome::Complete,
                now(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(turn.sequence, say.sequence + 1);
    }

    #[test]
    fn a_draft_is_replaced_whole_and_retired_by_its_turn() {
        let mut log = ChatLog::default();
        log.say("Sol", "hello", "say-1", now()).unwrap();
        log.draft("Kodo", "thu", "say-1", vec![], vec![], now())
            .unwrap();
        let draft = log
            .draft("Kodo", "thump", "say-1", vec![step("t-1")], vec![], now())
            .unwrap()
            .unwrap();
        assert_eq!(log.drafts(), vec![draft]);
        let turn = log
            .turn(
                "Kodo",
                "thump thump",
                "say-1",
                vec![step("t-1")],
                vec![],
                ChatOutcome::Complete,
                now(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(turn.steps, vec![step("t-1")]);
        assert!(log.drafts().is_empty());
        assert!(
            log.draft("Kodo", "late", "say-1", vec![], vec![], now())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_repeated_say_id_adds_no_second_line() {
        let mut log = ChatLog::default();
        log.say("Sol", "hello", "say-1", now())
            .expect("the first say enters the ring");
        log.say("Sol", "hello", "say-1", now()).unwrap();
        assert_eq!(log.snapshot().len(), 1);
    }

    #[test]
    fn one_id_carries_both_the_operator_and_the_spirit_line() {
        let mut log = ChatLog::default();
        log.say("Sol", "hello", "shared-id", now())
            .expect("the operator side enters the ring");
        log.turn(
            "Kodo",
            "answer",
            "shared-id",
            vec![],
            vec![],
            ChatOutcome::Complete,
            now(),
        )
        .expect("the spirit side is not the operator side");
        assert_eq!(log.snapshot().len(), 2);
    }

    #[test]
    fn the_ring_stays_bounded_and_keeps_the_newest_lines() {
        let mut log = ChatLog::default();
        for index in 0..CHAT_MAX_ENTRIES + 8 {
            log.say("Sol", "line", &format!("say-{index}"), now())
                .unwrap();
        }
        let snapshot = log.snapshot();
        assert_eq!(snapshot.len(), CHAT_MAX_ENTRIES);
        assert_eq!(
            snapshot.last().unwrap().turn_id,
            format!("say-{}", CHAT_MAX_ENTRIES + 7)
        );
    }

    #[test]
    fn oversize_text_is_cut_to_the_bound() {
        let mut log = ChatLog::default();
        let text = "x".repeat(CHAT_MAX_TEXT_CHARS + 5);
        let message = log.say("Sol", &text, "say-big", now()).unwrap().unwrap();
        assert_eq!(message.text.chars().count(), CHAT_MAX_TEXT_CHARS);
    }
}
