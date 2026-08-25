use serde_json::{Value, json};
use std::io::{BufRead, Write};

fn main() {
    let transcript = std::env::var("FAKE_SUBSTRATE_TRANSCRIPT").expect("transcript path");
    let mode = std::env::var("FAKE_SUBSTRATE_MODE").unwrap_or_else(|_| "relaunch-once".to_string());
    let stage_deadlines = json!({
        "requestedTtlSecs": 300,
        "exitingSecs": 60,
        "relaunchingSecs": 120,
        "relaunchAttemptLimit": 2,
    });
    let status_deadlines = json!({
        "expiresAt": "2026-08-25T00:05:00Z",
        "exitingDeadlineAt": "2026-08-25T00:01:00Z",
    });
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.expect("request line");
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).expect("request json");
        let answered_statuses = count_statuses(&transcript);
        append(&transcript, &line);
        let id = request["id"].as_str().unwrap_or("0").to_string();
        let method = request["method"].as_str().unwrap_or("").to_string();
        let response = match (method.as_str(), mode.as_str()) {
            ("restart_status", "storm") => refusal(
                &id,
                "restart_storm",
                "more than 3 restarts reached exiting for this workspace this hour",
            ),
            ("restart_status", _) if answered_statuses == 0 => result(
                &id,
                json!({
                    "workspace": request["params"]["workspace"].clone(),
                    "intent": {
                        "intentId": "intent-1",
                        "state": "requested",
                        "mode": "resume",
                        "sessionId": "session-1",
                        "deadlines": status_deadlines,
                    },
                }),
            ),
            ("restart_status", _) => result(
                &id,
                json!({
                    "workspace": request["params"]["workspace"].clone(),
                    "intent": null,
                }),
            ),
            ("restart_claim", _) => {
                if mode == "relaunch-broken" {
                    let program = std::env::var("FAKE_OMP_PROGRAM").expect("omp program path");
                    std::fs::remove_file(&program).expect("remove the omp program");
                }
                result(
                    &id,
                    json!({"claimToken": "claim-token-1", "claimEpoch": 1, "stageDeadlines": stage_deadlines}),
                )
            }
            ("restart_transition", _) => {
                // the state vocabulary has no compound: a failed relaunch reaches `failed`
                // and the stage it failed in lives on the intent row, not on the wire
                let to = request["params"]["to"].as_str().unwrap_or("relaunching");
                result(&id, json!({"state": to}))
            }
            _ => refusal(&id, "unknown_method", &format!("unknown method {method}")),
        };
        writeln!(stdout, "{response}").expect("response line");
        stdout.flush().expect("response flush");
    }
}

fn count_statuses(transcript: &str) -> usize {
    std::fs::read_to_string(transcript)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"restart_status\""))
        .count()
}

fn append(transcript: &str, line: &str) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(transcript)
        .expect("transcript file");
    writeln!(file, "{line}").expect("transcript line");
}

fn result(id: &str, result: Value) -> String {
    json!({"protocol": 1, "id": id, "result": result}).to_string()
}

fn refusal(id: &str, code: &str, message: &str) -> String {
    json!({"protocol": 1, "id": id, "error": {"code": code, "message": message, "retryable": false}})
        .to_string()
}
