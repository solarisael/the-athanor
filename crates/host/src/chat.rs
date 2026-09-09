//! The room conversation ring the chat projection serves.
//!
//! One Host serves one room, so this is a single bounded log. The Host stamps
//! sequence and time; a say is idempotent on its say id and a turn on its
//! turn id, so surface retries and doorman re-reports never produce twin
//! lines. The ring is in-memory: chat is a live surface, not an archive —
//! the durable conversation record stays with the shell conversation log.
//!
//! Beside the ring sit the drafts: the spirit side of says still being
//! answered. A draft is replaced whole on every report and retired by the
//! settled turn, which keeps the draft's tool steps when it brings none.

use protocol::{ChatAuthor, ChatDraft, ChatMessage, ChatStep};
use std::collections::VecDeque;

pub const CHAT_MAX_ENTRIES: usize = 256;
pub const CHAT_MAX_TEXT_CHARS: usize = 32_768;
// One doorman answers one say at a time, so one open draft is the normal
// count; the bounds only cap a misbehaving adapter, never a real conversation.
const CHAT_MAX_DRAFTS: usize = 8;
const CHAT_MAX_STEPS: usize = 64;

#[derive(Default)]
pub struct ChatLog {
    entries: VecDeque<ChatMessage>,
    drafts: Vec<ChatDraft>,
    next_sequence: u64,
}

impl ChatLog {
    /// Append one operator line. `None` means the say id already answered —
    /// a retry, not a new message.
    pub fn say(
        &mut self,
        author_name: &str,
        text: &str,
        say_id: &str,
        at: String,
    ) -> Option<ChatMessage> {
        self.append(ChatAuthor::Operator, author_name, text, say_id, vec![], at)
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
        at: String,
    ) -> Option<ChatMessage> {
        self.retire_draft(turn_id);
        self.append(ChatAuthor::Spirit, author_name, text, turn_id, bounded_steps(steps), at)
    }

    /// Replace the draft for one turn. `None` means the turn already settled,
    /// so a late report changes nothing.
    pub fn draft(
        &mut self,
        author_name: &str,
        text: &str,
        turn_id: &str,
        steps: Vec<ChatStep>,
        at: String,
    ) -> Option<ChatDraft> {
        if self
            .entries
            .iter()
            .any(|entry| entry.author == ChatAuthor::Spirit && entry.turn_id == turn_id)
        {
            return None;
        }
        self.retire_draft(turn_id);
        let draft = ChatDraft {
            turn_id: turn_id.to_owned(),
            author_name: author_name.to_owned(),
            text: bounded_text(text),
            steps: bounded_steps(steps),
            at,
        };
        self.drafts.push(draft.clone());
        while self.drafts.len() > CHAT_MAX_DRAFTS {
            self.drafts.remove(0);
        }
        Some(draft)
    }

    pub fn snapshot(&self) -> Vec<ChatMessage> {
        self.entries.iter().cloned().collect()
    }

    pub fn drafts(&self) -> Vec<ChatDraft> {
        self.drafts.clone()
    }

    fn retire_draft(&mut self, turn_id: &str) {
        self.drafts.retain(|draft| draft.turn_id != turn_id);
    }

    pub(crate) fn summary(&self) -> (usize, Option<&str>) {
        (self.entries.len(), self.entries.back().map(|message| message.at.as_str()))
    }

    fn append(
        &mut self,
        author: ChatAuthor,
        author_name: &str,
        text: &str,
        turn_id: &str,
        steps: Vec<ChatStep>,
        at: String,
    ) -> Option<ChatMessage> {
        if self
            .entries
            .iter()
            .any(|entry| entry.author == author && entry.turn_id == turn_id)
        {
            return None;
        }
        let message = ChatMessage {
            sequence: self.next_sequence,
            author,
            author_name: author_name.to_owned(),
            text: bounded_text(text),
            at,
            turn_id: turn_id.to_owned(),
            steps,
        };
        self.next_sequence += 1;
        self.entries.push_back(message.clone());
        while self.entries.len() > CHAT_MAX_ENTRIES {
            self.entries.pop_front();
        }
        Some(message)
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
        let say = log.say("Sol", "hello dragon", "say-1", now()).unwrap();
        let turn = log.turn("Kodo", "thump thump", "turn-1", vec![], now()).unwrap();
        assert_eq!(turn.sequence, say.sequence + 1);
    }

    #[test]
    fn a_draft_is_replaced_whole_and_retired_by_its_turn() {
        let mut log = ChatLog::default();
        log.say("Sol", "hello", "say-1", now()).unwrap();
        log.draft("Kodo", "thu", "say-1", vec![], now()).unwrap();
        let draft = log.draft("Kodo", "thump", "say-1", vec![step("t-1")], now()).unwrap();
        assert_eq!(log.drafts(), vec![draft]);
        let turn = log.turn("Kodo", "thump thump", "say-1", vec![step("t-1")], now()).unwrap();
        assert_eq!(turn.steps, vec![step("t-1")]);
        assert!(log.drafts().is_empty());
        assert!(log.draft("Kodo", "late", "say-1", vec![], now()).is_none());
    }

    #[test]
    fn a_repeated_say_id_adds_no_second_line() {
        let mut log = ChatLog::default();
        log.say("Sol", "hello", "say-1", now())
            .expect("the first say enters the ring");
        log.say("Sol", "hello", "say-1", now());
        assert_eq!(log.snapshot().len(), 1);
    }

    #[test]
    fn one_id_carries_both_the_operator_and_the_spirit_line() {
        let mut log = ChatLog::default();
        log.say("Sol", "hello", "shared-id", now())
            .expect("the operator side enters the ring");
        log.turn("Kodo", "answer", "shared-id", vec![], now())
            .expect("the spirit side is not the operator side");
        assert_eq!(log.snapshot().len(), 2);
    }

    #[test]
    fn the_ring_stays_bounded_and_keeps_the_newest_lines() {
        let mut log = ChatLog::default();
        for index in 0..CHAT_MAX_ENTRIES + 8 {
            log.say("Sol", "line", &format!("say-{index}"), now());
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
        let message = log.say("Sol", &text, "say-big", now()).unwrap();
        assert_eq!(message.text.chars().count(), CHAT_MAX_TEXT_CHARS);
    }
}
