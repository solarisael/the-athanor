//! The room conversation ring the chat projection serves.
//!
//! One Host serves one room, so this is a single bounded log. The Host stamps
//! sequence and time; a say is idempotent on its say id and a turn on its
//! turn id, so surface retries and doorman re-reports never produce twin
//! lines. The ring is in-memory: chat is a live surface, not an archive —
//! the durable conversation record stays with the shell conversation log.

use protocol::{ChatAuthor, ChatMessage};
use std::collections::VecDeque;

pub const CHAT_MAX_ENTRIES: usize = 256;
pub const CHAT_MAX_TEXT_CHARS: usize = 32_768;

#[derive(Default)]
pub struct ChatLog {
    entries: VecDeque<ChatMessage>,
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
        self.append(ChatAuthor::Operator, author_name, text, say_id, at)
    }

    /// Append one spirit line for a settled turn. `None` means this turn id
    /// already reported its spirit side.
    pub fn turn(
        &mut self,
        author_name: &str,
        text: &str,
        turn_id: &str,
        at: String,
    ) -> Option<ChatMessage> {
        self.append(ChatAuthor::Spirit, author_name, text, turn_id, at)
    }

    pub fn snapshot(&self) -> Vec<ChatMessage> {
        self.entries.iter().cloned().collect()
    }

    fn append(
        &mut self,
        author: ChatAuthor,
        author_name: &str,
        text: &str,
        turn_id: &str,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> String {
        "2026-08-28T12:00:00Z".to_owned()
    }

    #[test]
    fn says_and_turns_interleave_with_host_stamped_sequences() {
        let mut log = ChatLog::default();
        let say = log.say("Sol", "hello dragon", "say-1", now()).unwrap();
        let turn = log.turn("Kodo", "thump thump", "turn-1", now()).unwrap();
        assert_eq!(say.sequence, 0);
        assert_eq!(turn.sequence, 1);
        assert_eq!(log.snapshot().len(), 2);
    }

    #[test]
    fn a_repeated_say_id_is_a_retry_not_a_twin_line() {
        let mut log = ChatLog::default();
        assert!(log.say("Sol", "hello", "say-1", now()).is_some());
        assert!(log.say("Sol", "hello", "say-1", now()).is_none());
        assert_eq!(log.snapshot().len(), 1);
    }

    #[test]
    fn one_turn_id_carries_both_sides_without_colliding() {
        let mut log = ChatLog::default();
        assert!(log.say("Sol", "hello", "shared-id", now()).is_some());
        assert!(log.turn("Kodo", "answer", "shared-id", now()).is_some());
        assert!(log.turn("Kodo", "again", "shared-id", now()).is_none());
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
        assert_eq!(snapshot.last().unwrap().turn_id, format!("say-{}", CHAT_MAX_ENTRIES + 7));
    }

    #[test]
    fn oversize_text_is_cut_to_the_bound() {
        let mut log = ChatLog::default();
        let text = "x".repeat(CHAT_MAX_TEXT_CHARS + 5);
        let message = log.say("Sol", &text, "say-big", now()).unwrap();
        assert_eq!(message.text.chars().count(), CHAT_MAX_TEXT_CHARS);
    }
}
