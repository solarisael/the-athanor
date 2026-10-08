use protocol::{ChatAuthor, ChatMessage};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum LifecycleRequest {
    ChatNext,
    ChatOutcome {
        say_id: String,
        messages: Vec<TurnObservation>,
    },
    BoatLine {
        context_window: f64,
        compaction_threshold: f64,
    },
    KnockPlan {
        delivered: bool,
        start_settled: bool,
        turn_started: bool,
        turn_ended: bool,
        delivered_elapsed_ms: Option<u64>,
        ended_elapsed_ms: Option<u64>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TurnObservation {
    kind: ObservationKind,
    #[serde(default)]
    say_id: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    thinking: Vec<String>,
    #[serde(default)]
    settled: bool,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum ObservationKind {
    Origin,
    Assistant,
    Other,
}

pub(crate) fn execute(
    request: LifecycleRequest,
    messages: &[ChatMessage],
) -> Result<Value, String> {
    match request {
        LifecycleRequest::ChatNext => {
            // # enough: the ring caps this scan at 256 rows; index answered turns if that bound grows.
            let next = messages
                .iter()
                .filter(|line| {
                    line.author == ChatAuthor::Operator
                        && !messages.iter().any(|answer| {
                            answer.author == ChatAuthor::Spirit && answer.turn_id == line.turn_id
                        })
                })
                .min_by_key(|line| line.sequence);
            Ok(json!({ "next": next }))
        }
        LifecycleRequest::ChatOutcome { say_id, messages } => {
            let Some(origin) = messages.iter().rposition(|message| {
                message.kind == ObservationKind::Origin
                    && message.say_id.as_deref() == Some(say_id.as_str())
            }) else {
                return Ok(json!({ "answer": null }));
            };
            let owned = &messages[origin + 1..];
            let end = owned
                .iter()
                .position(|message| message.kind == ObservationKind::Origin)
                .unwrap_or(owned.len());
            let owned = &owned[..end];
            let Some(index) = owned
                .iter()
                .position(|message| message.kind == ObservationKind::Assistant && message.settled)
            else {
                return Ok(json!({ "answer": null }));
            };
            let answer = &owned[index];
            let outcome = match answer.stop_reason.as_deref() {
                Some("error") => "error",
                Some("aborted") => "aborted",
                _ => "complete",
            };
            let thinking: Vec<&str> = owned[..=index]
                .iter()
                .flat_map(|message| message.thinking.iter().map(String::as_str))
                .collect();
            Ok(
                json!({ "answer": { "text": answer.text, "thinking": thinking, "outcome": outcome } }),
            )
        }
        LifecycleRequest::BoatLine {
            context_window,
            compaction_threshold,
        } => {
            if !context_window.is_finite()
                || context_window <= 0.0
                || !compaction_threshold.is_finite()
            {
                return Err("boat threshold requires finite harness token limits".into());
            }
            Ok(json!({ "tokens": compaction_threshold - (context_window * 0.1).floor() }))
        }
        LifecycleRequest::KnockPlan {
            delivered,
            start_settled,
            turn_started,
            turn_ended,
            delivered_elapsed_ms,
            ended_elapsed_ms,
        } => {
            if !delivered {
                return Ok(json!({ "deliver": true, "settlements": [], "failure": null }));
            }
            let failure = if !start_settled
                && delivered_elapsed_ms.is_some_and(|elapsed| elapsed >= 25_000)
            {
                Some("recipient turn start did not settle within 25000ms")
            } else if !turn_started && delivered_elapsed_ms.is_some_and(|elapsed| elapsed >= 60_000)
            {
                Some("recipient turn did not start within 60000ms")
            } else if turn_ended && ended_elapsed_ms.is_some_and(|elapsed| elapsed >= 25_000) {
                Some("recipient turn completion did not settle within 25000ms")
            } else {
                None
            };
            let settlements: &[&str] = if failure.is_some() {
                &[]
            } else {
                match (start_settled, turn_started && turn_ended) {
                    (false, true) => &["started", "completed"],
                    (false, false) => &["started"],
                    (true, true) => &["completed"],
                    (true, false) => &[],
                }
            };
            Ok(json!({ "deliver": false, "settlements": settlements, "failure": failure }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(request: Value) -> Value {
        execute(serde_json::from_value(request).unwrap(), &[]).unwrap()
    }

    #[test]
    fn next_chat_say_skips_answered_turns_and_uses_sequence_order() {
        let messages: Vec<ChatMessage> = serde_json::from_value(json!([
            {"sequence": 4, "author": "operator", "authorName": "Sol", "text": "later", "at": "", "turnId": "later"},
            {"sequence": 1, "author": "operator", "authorName": "Sol", "text": "answered", "at": "", "turnId": "done"},
            {"sequence": 3, "author": "operator", "authorName": "Sol", "text": "next", "at": "", "turnId": "next"},
            {"sequence": 2, "author": "spirit", "authorName": "Kintsu", "text": "reply", "at": "", "turnId": "done"}
        ])).unwrap();
        let result = execute(LifecycleRequest::ChatNext, &messages).unwrap();
        assert_eq!(result["next"]["turnId"], "next");
    }

    #[test]
    fn chat_uses_first_settled_answer_inside_the_matching_turn() {
        let result = decide(json!({
            "action": "chatOutcome", "sayId": "say-1", "messages": [
                {"kind":"origin", "sayId":"say-1"},
                {"kind":"assistant", "text":"progress", "settled":false, "thinking":["work"]},
                {"kind":"assistant", "text":"answer", "settled":true, "stopReason":"stop"},
                {"kind":"origin", "sayId":"say-2"},
                {"kind":"assistant", "text":"unrelated", "settled":true}
            ]
        }));
        assert_eq!(
            result["answer"],
            json!({"text":"answer", "thinking":["work"], "outcome":"complete"})
        );
    }

    #[test]
    fn knock_deadlines_preserve_acknowledgement_precedence() {
        let result = decide(json!({
            "action":"knockPlan", "delivered":true, "startSettled":false,
            "turnStarted":true, "turnEnded":true,
            "deliveredElapsedMs":25_000, "endedElapsedMs":0
        }));
        assert!(result["failure"].is_string());
        assert_eq!(result["settlements"], json!([]));
        let result = decide(json!({
            "action":"knockPlan", "delivered":true, "startSettled":false,
            "turnStarted":true, "turnEnded":true,
            "deliveredElapsedMs":24_999, "endedElapsedMs":0
        }));
        assert_eq!(result["settlements"], json!(["started", "completed"]));
    }
}
