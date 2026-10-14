//! Durable receipts for portable controls across a Host restart.
//!
//! Failure contract: command receipts, restart records and retired instance
//! ids are written atomically to `<state_dir>/portable-sessions.json` after
//! every mutating request. A command's identity is journaled (`pending`)
//! before its native request runs, and its receipt (`queued`) before any poll
//! may see it; an applied restart is journaled (`awaitingExit`) before its
//! driver exists. A Host that starts again restores the journal, marks every
//! unsettled command `outcomeUnknown` and every unsettled restart `failed`
//! ("Host restarted; outcome unknown"), and replays nothing. No instance is
//! restored: `connected` comes only from a fresh live attach. A reused
//! commandId answers its recorded receipt or `command_conflict`, never a second
//! execution. The native restart intent row is left untouched here; it settles
//! by its own stage deadline through the request door's sweep. Secrets (claim
//! token, successor proof) are never written.

use super::*;
use atomicwrites::{AtomicFile, OverwriteBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;

const FILE: &str = "portable-sessions.json";
const INTERRUPTED: &str = "Host restarted; outcome unknown";

pub(super) fn digest(body: &Value) -> String {
    format!("{:x}", Sha256::digest(body.to_string().as_bytes()))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRow {
    command_id: Uuid,
    fingerprint: String,
    kind: String,
    target: Uuid,
    conversation_id: String,
    issuer: Option<Uuid>,
    issued_at: DateTime<Utc>,
    deadline_at: DateTime<Utc>,
    state: String,
    detail: Option<String>,
    restart_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestartRow {
    restart_id: Uuid,
    intent_id: String,
    target: Uuid,
    conversation_id: String,
    workspace: String,
    session_file: String,
    reason: String,
    launcher: String,
    stage: String,
    detail: Option<String>,
    fresh: bool,
    successor: Option<Uuid>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    format: u32,
    commands: Vec<CommandRow>,
    restarts: Vec<RestartRow>,
    retired: Vec<Uuid>,
}

fn command_state(text: &str) -> CommandState {
    match text {
        "pending" => CommandState::Pending,
        "queued" => CommandState::Queued,
        "delivered" => CommandState::Delivered,
        "applied" => CommandState::Applied,
        "refused" => CommandState::Refused,
        "failed" => CommandState::Failed,
        "expired" => CommandState::Expired,
        _ => CommandState::OutcomeUnknown,
    }
}

fn stage(text: &str) -> RestartStage {
    match text {
        "verified" => RestartStage::Verified,
        "handedToKeeper" => RestartStage::HandedToKeeper,
        _ => RestartStage::Failed,
    }
}

pub(super) fn save(state_dir: &Path, registry: &Registry) -> Result<(), String> {
    let ledger = Ledger {
        format: 1,
        commands: registry
            .command_order
            .iter()
            .filter_map(|id| registry.commands.get(id).map(|command| (id, command)))
            .map(|(id, command)| CommandRow {
                command_id: *id,
                fingerprint: command.fingerprint.clone(),
                kind: command.kind.clone(),
                target: command.target,
                conversation_id: command.conversation_id.clone(),
                issuer: command.issuer,
                issued_at: command.issued_at,
                deadline_at: command.deadline_at,
                state: command.state.as_str().into(),
                detail: command.detail.clone(),
                restart_id: command.restart_id,
            })
            .collect(),
        restarts: registry
            .restarts
            .iter()
            .map(|(id, record)| RestartRow {
                restart_id: *id,
                intent_id: record.intent_id.clone(),
                target: record.target,
                conversation_id: record.conversation_id.clone(),
                workspace: record.workspace.clone(),
                session_file: record.session_file.clone(),
                reason: record.reason.clone(),
                launcher: record.launcher.as_str().into(),
                stage: record.stage.as_str().into(),
                detail: record.detail.clone(),
                fresh: record.fresh,
                successor: record.successor,
                updated_at: record.updated_at,
            })
            .collect(),
        retired: registry.retired_order.iter().copied().collect(),
    };
    let bytes = serde_json::to_vec(&ledger).map_err(|error| error.to_string())?;
    AtomicFile::new(state_dir.join(FILE), OverwriteBehavior::AllowOverwrite)
        .write(|file| file.write_all(&bytes))
        .map_err(|error| format!("portable session ledger could not be written: {error}"))
}

impl Registry {
    /// A missing ledger is a first start; an unreadable one refuses Host start
    /// rather than forgetting which commandIds already ran.
    pub(crate) fn load(state_dir: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(state_dir.join(FILE)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("portable session ledger is unreadable: {error}")),
        };
        let ledger: Ledger = serde_json::from_slice(&bytes)
            .map_err(|error| format!("portable session ledger is corrupt: {error}"))?;
        let mut registry = Self::default();
        for row in ledger.commands {
            let mut state = command_state(&row.state);
            let mut detail = row.detail;
            if !state.terminal() {
                state = CommandState::OutcomeUnknown;
                detail = Some(INTERRUPTED.into());
            }
            registry.command_order.push_back(row.command_id);
            registry.commands.insert(
                row.command_id,
                Command {
                    fingerprint: row.fingerprint,
                    kind: row.kind,
                    payload: Value::Null,
                    target: row.target,
                    issuer: row.issuer,
                    issued_at: row.issued_at,
                    deadline_at: row.deadline_at,
                    state,
                    detail,
                    restart_id: row.restart_id,
                    conversation_id: row.conversation_id,
                },
            );
        }
        for row in ledger.restarts {
            let reached = stage(&row.stage);
            let detail = if reached == RestartStage::Failed && row.stage != "failed" {
                Some(INTERRUPTED.into())
            } else {
                row.detail
            };
            registry.restarts.insert(
                row.restart_id,
                RestartRecord {
                    intent_id: row.intent_id,
                    target: row.target,
                    conversation_id: row.conversation_id,
                    workspace: row.workspace,
                    session_file: row.session_file,
                    reason: row.reason,
                    launcher: if row.launcher == "keeper" {
                        LauncherKind::Keeper
                    } else {
                        LauncherKind::Tern
                    },
                    stage: reached,
                    detail,
                    terminal: None,
                    launch: None,
                    claim_token: None,
                    successor_proof: None,
                    expected_pane: None,
                    relaunch_by: None,
                    successor: row.successor,
                    fresh: row.fresh,
                    reported: true,
                    updated_at: row.updated_at,
                },
            );
        }
        for id in ledger.retired {
            if registry.retired.insert(id) {
                registry.retired_order.push_back(id);
            }
        }
        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restarted_host_never_replays_and_restores_no_connection() {
        let dir = std::env::temp_dir().join(format!("athanor-ledger-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = Registry::default();
        let (command_id, target, retired) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        registry.commands.insert(
            command_id,
            Command {
                fingerprint: digest(&json!({"a": 1})),
                kind: "deliver".into(),
                payload: json!({"text": "hi"}),
                target,
                issuer: None,
                issued_at: Utc::now(),
                deadline_at: Utc::now(),
                state: CommandState::Delivered,
                detail: None,
                restart_id: None,
                conversation_id: "c1".into(),
            },
        );
        registry.command_order.push_back(command_id);
        // A restart whose identity was journaled before its native request,
        // and one armed (awaitingExit) whose driver died with the Host.
        let (pending_id, armed_id, restart_id) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        for (id, state, restart) in [
            (pending_id, CommandState::Pending, None),
            (armed_id, CommandState::Applied, Some(restart_id)),
        ] {
            registry.commands.insert(
                id,
                Command {
                    fingerprint: digest(&json!({"restart": id})),
                    kind: "restart".into(),
                    payload: json!({"reason": "r"}),
                    target,
                    issuer: None,
                    issued_at: Utc::now(),
                    deadline_at: Utc::now(),
                    state,
                    detail: None,
                    restart_id: restart,
                    conversation_id: "c1".into(),
                },
            );
            registry.command_order.push_back(id);
        }
        registry.restarts.insert(
            restart_id,
            RestartRecord {
                intent_id: "00000000-0000-0000-0000-000000000001".into(),
                target,
                conversation_id: "c1".into(),
                workspace: "C:/w".into(),
                session_file: "C:/s.jsonl".into(),
                reason: "r".into(),
                launcher: LauncherKind::Tern,
                stage: RestartStage::AwaitingExit,
                detail: None,
                terminal: None,
                launch: None,
                claim_token: Some("secret".into()),
                successor_proof: Some("secret".into()),
                expected_pane: None,
                relaunch_by: None,
                successor: None,
                fresh: false,
                reported: false,
                updated_at: Utc::now(),
            },
        );
        registry.retire(retired, "detached");
        save(&dir, &registry).unwrap();
        let written = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(
            !written.contains("secret"),
            "claim token and proof never reach disk"
        );
        let loaded = Registry::load(&dir).unwrap();
        let command = &loaded.commands[&command_id];
        assert_eq!(command.state, CommandState::OutcomeUnknown);
        assert_eq!(command.fingerprint, digest(&json!({"a": 1})));
        assert_eq!(
            loaded.commands[&pending_id].state,
            CommandState::OutcomeUnknown,
            "a journaled identity whose request never answered is never asked again"
        );
        assert_eq!(
            loaded.commands[&armed_id].state,
            CommandState::Applied,
            "a committed applied receipt keeps answering applied"
        );
        let restart = &loaded.restarts[&restart_id];
        assert_eq!(
            restart.stage,
            RestartStage::Failed,
            "an armed restart never drives again"
        );
        assert_eq!(restart.detail.as_deref(), Some(INTERRUPTED));
        assert!(restart.claim_token.is_none() && restart.successor_proof.is_none());
        assert!(
            loaded.instances.is_empty(),
            "connected is never restored from disk"
        );
        assert!(loaded.retired.contains(&retired));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
