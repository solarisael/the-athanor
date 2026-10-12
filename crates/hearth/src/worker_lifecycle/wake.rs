use super::WAKE_LIMIT_CHARS;
use super::model::{EntryRecord, InstanceState, OperationRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeKind {
    Opening,
    Resume,
    Successor,
}

impl WakeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Resume => "resume",
            Self::Successor => "successor",
        }
    }
}

/// Everything an `open` wake needs except entry bodies. The store loads the
/// entries named by [`WakePlan::sequences`] from the same book, then renders.
#[derive(Clone, Debug, PartialEq)]
pub struct WakePlan {
    pub kind: WakeKind,
    pub swarm_id: String,
    pub book_id: String,
    pub instance_id: String,
    pub ordinal: i64,
    pub agent_name: String,
    pub state: InstanceState,
    pub handoff_requests: i64,
    pub last_sequence: i64,
    pub own_entries: i64,
    pub last_own_sequence: Option<i64>,
    pub predecessor_instance_id: Option<String>,
    pub checkpoint_sequence: Option<i64>,
    pub handoff_sequence: Option<i64>,
    pub repair_sequence: Option<i64>,
    pub pending: Vec<OperationRecord>,
}

impl WakePlan {
    pub fn sequences(&self) -> Vec<i64> {
        [
            self.checkpoint_sequence,
            self.handoff_sequence,
            self.repair_sequence,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

fn report_text(entries: &[EntryRecord], sequence: Option<i64>) -> Option<String> {
    let sequence = sequence?;
    let entry = entries.iter().find(|entry| entry.sequence == sequence)?;
    let report = entry.body.get("report").unwrap_or(&Value::Null);
    Some(match report {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).expect("JSON value serializes"),
    })
}

fn pending_text(out: &mut String, pending: &[OperationRecord]) {
    if pending.is_empty() {
        out.push_str("Unresolved operations: none recorded.\n");
        return;
    }
    out.push_str(
        "Unresolved operations (started, never recorded as ended; their side effects may or may not have happened, verify before repeating):\n",
    );
    for operation in pending {
        let _ = writeln!(
            out,
            "- {} (instance {}, started at sequence {})",
            operation.operation_id, operation.instance_id, operation.started_sequence
        );
    }
}

/// Bound the wake to [`WAKE_LIMIT_CHARS`] characters, marking any clip.
pub(super) fn clip(text: String) -> String {
    if text.chars().count() <= WAKE_LIMIT_CHARS {
        return text;
    }
    const MARKER: &str =
        "\n[wake clipped at 12000 characters; read the book with worker_notebook for the rest]";
    let keep = WAKE_LIMIT_CHARS - MARKER.chars().count();
    let mut clipped: String = text.chars().take(keep).collect();
    clipped.push_str(MARKER);
    clipped
}

pub(super) fn render(plan: &WakePlan, entries: &[EntryRecord]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Worker notebook {}: book {}, instance {} (#{}, agent {}), state {}.",
        plan.kind.as_str(),
        plan.book_id,
        plan.instance_id,
        plan.ordinal,
        plan.agent_name,
        plan.state.as_str()
    );
    match plan.kind {
        WakeKind::Opening => {
            out.push_str(
                "Your assignment is the task prompt above; this book is its working record. Record notes with worker_notebook append and read earlier entries with worker_notebook read.\n",
            );
            if plan.last_sequence > 0 {
                let _ = writeln!(
                    out,
                    "The book already holds {} entries (parent briefing or evidence); read them from sequence 0.",
                    plan.last_sequence
                );
            }
        }
        WakeKind::Resume => {
            out.push_str(
                "This is the same session resuming its own instance. Continue from your checkpoint; do not repeat completed work.\n",
            );
            match report_text(entries, plan.checkpoint_sequence) {
                Some(report) => {
                    let _ = writeln!(
                        out,
                        "Latest checkpoint (sequence {}):\n{report}",
                        plan.checkpoint_sequence.unwrap_or_default()
                    );
                }
                None => out.push_str("No checkpoint recorded yet.\n"),
            }
            let _ = writeln!(
                out,
                "Your progress: {} entries written, last at sequence {}. The book holds {} entries.",
                plan.own_entries,
                plan.last_own_sequence
                    .map_or_else(|| "none".to_owned(), |sequence| sequence.to_string()),
                plan.last_sequence
            );
            if plan.state == InstanceState::HandoffOnly {
                let _ = writeln!(
                    out,
                    "Handoff-only mode is latched: no investigation, edits, or spawns. Yield your handoff now ({} final model requests used of 2).",
                    plan.handoff_requests
                );
            } else if plan.state == InstanceState::Wrapping {
                out.push_str("Context passed 40%: wrap up and prepare your handoff.\n");
            }
        }
        WakeKind::Successor => {
            let _ = writeln!(
                out,
                "You continue predecessor instance {} in this book. Its handoff follows; do not redo completed work.",
                plan.predecessor_instance_id.as_deref().unwrap_or("unknown")
            );
            match report_text(entries, plan.handoff_sequence) {
                Some(report) => {
                    let _ = writeln!(
                        out,
                        "Predecessor handoff (sequence {}):\n{report}",
                        plan.handoff_sequence.unwrap_or_default()
                    );
                }
                None => out.push_str("Predecessor handoff report: none recorded.\n"),
            }
            if let Some(report) = report_text(entries, plan.repair_sequence) {
                let _ = writeln!(
                    out,
                    "The predecessor's handoff was incomplete; the parent supplied this repair (sequence {}):\n{report}",
                    plan.repair_sequence.unwrap_or_default()
                );
            }
            let _ = writeln!(
                out,
                "The book holds {} entries; read specific pages with worker_notebook read rather than replaying it whole.",
                plan.last_sequence
            );
        }
    }
    pending_text(&mut out, &plan.pending);
    clip(out)
}
