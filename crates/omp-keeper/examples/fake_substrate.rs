//! A fake substrate that answers the restart wire for the keeper's smoke tests.
//!
//! The concern: script one substrate without a database and without drifting
//! from the real door. Two rules keep it honest:
//!
//! 1. Every answer is a real `house_protocol::restart` struct, serialized. The
//!    fixture cannot invent a field the door lacks, cannot omit one it has, and
//!    cannot spell a state its own way.
//! 2. Every request is parsed back into the real params struct and `validate()`d
//!    exactly as the substrate's door does, so a request the real House would
//!    refuse is refused here too.
//!
//! Those rules are the answer to a review finding: the previous fixture wrote
//! JSON by hand and answered `intent-1` and `claim-token-1` — shapes
//! `RestartClaimParams::validate` and `RestartTransitionParams::validate`
//! reject outright. Fixture green used to be able to mean wire red.
//!
//! State lives in a sidecar file beside the transcript because the keeper opens
//! more than one substrate session per run and each session is a fresh process.

use chrono::{DateTime, TimeDelta, Utc};
use house_protocol::PROTOCOL_VERSION;
use house_protocol::restart::{
    RestartClaimParams, RestartClaimReceipt, RestartMode, RestartStageDeadlines, RestartState,
    RestartStatusDeadlines, RestartStatusIntent, RestartStatusParams, RestartStatusReceipt,
    RestartTransitionParams, RestartTransitionReceipt, RestartTransitionTarget,
};
use omp_keeper::protocol::{
    METHOD_RESTART_CLAIM, METHOD_RESTART_STATUS, METHOD_RESTART_TRANSITION, STORM_REFUSAL_CODE,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// A canonical lowercase UUID, because `uuid_shaped` refuses anything else.
const INTENT_ID: &str = "3f6b9c2a-7d41-4e58-9a0b-1c8e5d2f4a67";
/// 64 lowercase hex, because `hex_token` refuses anything else.
const CLAIM_TOKEN: &str = "9f2c7a1e4b8d60359f2c7a1e4b8d60359f2c7a1e4b8d60359f2c7a1e4b8d6035";
const SESSION_ID: &str = "session-1";

// The contract's stage numbers, mirroring house-substrate's restart consts.
const REQUESTED_TTL_SECS: i64 = 300;
const EXITING_DEADLINE_SECS: i64 = 60;
const RELAUNCHING_DEADLINE_SECS: i64 = 120;
const RELAUNCH_ATTEMPT_LIMIT: i32 = 2;
/// `unverified` runs the real relaunching stage on a short published window so
/// the smoke costs seconds. The keeper obeys whatever the House publishes, so a
/// small number here exercises the same code path as 120.
const UNVERIFIED_RELAUNCHING_SECS: i64 = 2;
/// `exiting-overrun` publishes a deadline the keeper is already late for, which
/// is what a keeper that started after the adapter armed actually meets.
const OVERRUN_SECS: i64 = 5;

/// What one run of the fixture has already answered. The keeper's sessions are
/// separate processes, so this crosses them on disk.
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Script {
    statuses: u32,
    relaunch_transitions: u32,
    /// When the last relaunching transition landed. The substrate mints
    /// `relaunching_deadline_at = NOW() + relaunching_secs` on every relaunching
    /// transition, so each retry gets its own window; this anchors the same way.
    relaunching_at: Option<String>,
}

fn main() {
    let transcript = std::env::var("FAKE_SUBSTRATE_TRANSCRIPT").expect("transcript path");
    let mode = std::env::var("FAKE_SUBSTRATE_MODE").unwrap_or_else(|_| "full-loop".to_string());
    let script_path = format!("{transcript}.script.json");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.expect("request line");
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).expect("request json");
        append(&transcript, &line);
        let id = request["id"].as_str().unwrap_or("0").to_string();
        let method = request["method"].as_str().unwrap_or("").to_string();
        let params = request["params"].clone();
        let mut script = read_script(&script_path);
        let response = match refuse_bad_shape(&method, &params) {
            Some(reason) => refusal(&id, "invalid_params", &reason),
            None => answer(&id, &method, &mode, &params, &mut script),
        };
        write_script(&script_path, &script);
        writeln!(stdout, "{response}").expect("response line");
        stdout.flush().expect("response flush");
    }
}

/// The door's own strictness, borrowed. `deny_unknown_fields` catches an extra
/// field, a missing field fails to parse, and `validate()` catches a malformed
/// id or token.
fn refuse_bad_shape(method: &str, params: &Value) -> Option<String> {
    let checked = match method {
        METHOD_RESTART_STATUS => parse::<RestartStatusParams>(params).and_then(|p| p.validate()),
        METHOD_RESTART_CLAIM => parse::<RestartClaimParams>(params).and_then(|p| p.validate()),
        METHOD_RESTART_TRANSITION => {
            parse::<RestartTransitionParams>(params).and_then(|p| p.validate())
        }
        _ => Ok(()),
    };
    // The real door answers `invalid_params` with static text; the fixture says
    // the reason instead, so a drift names itself in the test output.
    checked.err()
}

fn parse<T: serde::de::DeserializeOwned>(params: &Value) -> Result<T, String> {
    serde_json::from_value(params.clone()).map_err(|error| error.to_string())
}

fn answer(id: &str, method: &str, mode: &str, params: &Value, script: &mut Script) -> String {
    match method {
        METHOD_RESTART_STATUS => {
            if mode == "storm" {
                return storm_refusal(id);
            }
            // The substrate walking off mid-watch, which is what a crashed or
            // restarted House looks like to the keeper: the verify poll after the
            // window read gets no answer at all, ever.
            if mode == "substrate-dies-mid-watch" && script.statuses >= 2 {
                std::process::exit(0);
            }
            // The retry's window read is refused, so the keeper must keep the
            // deadline the House published for the first attempt.
            if mode == "window-read-refused" && script.relaunch_transitions >= 2 {
                return refusal(
                    id,
                    "stale_lease",
                    "the lease is expired, superseded, stale, or invalid",
                );
            }
            let index = script.statuses;
            script.statuses += 1;
            let receipt = RestartStatusReceipt {
                workspace: params["workspace"].as_str().unwrap_or_default().to_string(),
                intent: pending_intent(mode, index, script),
            };
            result(id, &receipt)
        }
        METHOD_RESTART_CLAIM => {
            if mode == "storm-on-claim" {
                return storm_refusal(id);
            }
            if mode == "relaunch-broken" {
                let program = std::env::var("FAKE_OMP_PROGRAM").expect("omp program path");
                std::fs::remove_file(&program).expect("remove the omp program");
            }
            let receipt = RestartClaimReceipt {
                claim_token: CLAIM_TOKEN.to_string(),
                claim_epoch: 1,
                stage_deadlines: stage_deadlines(mode),
            };
            result(id, &receipt)
        }
        METHOD_RESTART_TRANSITION => {
            let params: RestartTransitionParams =
                parse(params).expect("transition params already validated");
            if params.to == RestartTransitionTarget::Relaunching {
                script.relaunch_transitions += 1;
                script.relaunching_at = Some(Utc::now().to_rfc3339());
            }
            let reached = match params.to {
                RestartTransitionTarget::Exiting => RestartState::Exiting,
                RestartTransitionTarget::Relaunching => RestartState::Relaunching,
                RestartTransitionTarget::Failed => RestartState::Failed,
            };
            result(id, &RestartTransitionReceipt { state: reached })
        }
        _ => refusal(id, "unknown_method", &format!("unknown method {method}")),
    }
}

/// The pending intent each mode publishes for the nth status ask.
///
/// `restart_status` only ever reports the pending states — requested, exiting,
/// claimed, relaunching — so a verified intent reads as absent, never as
/// `verified`. Returning `None` here is how this fixture says "the successor
/// verified", exactly as the real read does.
fn pending_intent(mode: &str, index: u32, script: &Script) -> Option<RestartStatusIntent> {
    let deadlines = stage_deadlines(mode);
    match (mode, index) {
        ("no-intent", _) => None,
        // the House named a deadline the keeper is already past
        ("exiting-overrun", 0) => Some(exiting_intent(-OVERRUN_SECS)),
        ("exiting-overrun", _) => None,
        // the successor never verifies: the intent stays relaunching forever
        ("unverified" | "window-read-refused", 0) => Some(exiting_intent(deadlines.exiting_secs)),
        ("unverified" | "window-read-refused", _) => Some(relaunching_intent(script, &deadlines)),
        (_, 0) => Some(exiting_intent(deadlines.exiting_secs)),
        (_, 1) => Some(relaunching_intent(script, &deadlines)),
        (_, _) => None,
    }
}

fn exiting_intent(exiting_offset_secs: i64) -> RestartStatusIntent {
    RestartStatusIntent {
        intent_id: INTENT_ID.to_string(),
        state: RestartState::Exiting,
        mode: RestartMode::Resume,
        session_id: Some(SESSION_ID.to_string()),
        deadlines: RestartStatusDeadlines {
            expires_at: instant(REQUESTED_TTL_SECS),
            exiting_deadline_at: Some(instant(exiting_offset_secs)),
            relaunching_deadline_at: None,
        },
    }
}

fn relaunching_intent(script: &Script, deadlines: &RestartStageDeadlines) -> RestartStatusIntent {
    let anchor = script
        .relaunching_at
        .as_deref()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    RestartStatusIntent {
        intent_id: INTENT_ID.to_string(),
        state: RestartState::Relaunching,
        mode: RestartMode::Resume,
        session_id: Some(SESSION_ID.to_string()),
        deadlines: RestartStatusDeadlines {
            expires_at: instant(REQUESTED_TTL_SECS),
            exiting_deadline_at: None,
            relaunching_deadline_at: Some(
                (anchor + span(deadlines.relaunching_secs)).to_rfc3339(),
            ),
        },
    }
}

fn stage_deadlines(mode: &str) -> RestartStageDeadlines {
    let relaunching_secs = if mode == "unverified" || mode == "window-read-refused" {
        UNVERIFIED_RELAUNCHING_SECS
    } else {
        RELAUNCHING_DEADLINE_SECS
    };
    RestartStageDeadlines {
        requested_ttl_secs: REQUESTED_TTL_SECS,
        exiting_secs: EXITING_DEADLINE_SECS,
        relaunching_secs,
        relaunch_attempt_limit: RELAUNCH_ATTEMPT_LIMIT,
    }
}

fn span(seconds: i64) -> TimeDelta {
    TimeDelta::try_seconds(seconds).expect("a usable stage length")
}

fn instant(offset_secs: i64) -> String {
    (Utc::now() + span(offset_secs)).to_rfc3339()
}

fn read_script(path: &str) -> Script {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_script(path: &str, script: &Script) {
    std::fs::write(path, serde_json::to_vec(script).expect("script json")).expect("script file");
}

fn append(transcript: &str, line: &str) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(transcript)
        .expect("transcript file");
    writeln!(file, "{line}").expect("transcript line");
}

fn result<T: Serialize>(id: &str, receipt: &T) -> String {
    let body = serde_json::to_value(receipt).expect("receipt serializes");
    json!({"protocol": PROTOCOL_VERSION, "id": id, "result": body}).to_string()
}

fn refusal(id: &str, code: &str, message: &str) -> String {
    json!({"protocol": PROTOCOL_VERSION, "id": id, "error": {"code": code, "message": message, "retryable": false}})
        .to_string()
}

fn storm_refusal(id: &str) -> String {
    // the substrate's own words (house-substrate/src/restart/mod.rs:152-155)
    refusal(
        id,
        STORM_REFUSAL_CODE,
        "too many restarts reached exiting for this workspace inside the storm window",
    )
}
