use std::collections::{HashMap, VecDeque};
use std::fmt;

use sha2::{Digest, Sha256};
use summoning::presence::{
    PRESENCE_MAX_REPAIR_RULES, PresenceAuthentication, PresenceCapability, PresenceCloseMaterial,
    PresenceCloseRequest, PresenceContract, PresenceFrame, PresenceLedger, PresenceOpenRequest,
    PresenceReceipt, PresenceResult, PresenceSettleRequest, PresenceTurnRequest, close_presence,
    compile_presence, open_presence, settle_presence,
};

/// Answered requests retained for replay. Past the bound the oldest keys drop
/// and a request under a forgotten key executes again.
pub const PRESENCE_MAX_REPLAY_ENTRIES: usize = 64;

/// How many settlement receipts one session retains.
pub const PRESENCE_MAX_SESSION_RECEIPTS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceOperation {
    Open,
    Compile,
    Settle,
    Close,
}

impl PresenceOperation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Compile => "compile",
            Self::Settle => "settle",
            Self::Close => "close",
        }
    }
}

impl fmt::Display for PresenceOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Host-owned Presence lifecycle: live frame, session ledger, active contract,
/// answered requests.
#[derive(Default)]
pub struct PresenceRuntime {
    sessions: HashMap<String, PresenceSession>,
    replay: ReplayLedger,
}

struct PresenceSession {
    frame: PresenceFrame,
    /// The authoritative ledger. Nothing on the wire may author it.
    ledger: PresenceLedger,
    /// One contract, not a map. A turn contract expires with its turn; a map
    /// kept stale ones settleable after they stopped being true.
    active_contract: Option<PresenceContract>,
    receipts: VecDeque<PresenceReceipt>,
}

/// One answered request: operation, what was asked, what came back.
struct ReplayEntry {
    session: String,
    key: String,
    operation: PresenceOperation,
    request_digest: String,
    outcome: PresenceResult,
}

#[derive(Default)]
struct ReplayLedger {
    entries: VecDeque<ReplayEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresenceRuntimeError {
    MissingFrame,
    ReplayOperationConflict {
        key: String,
        recorded: PresenceOperation,
        attempted: PresenceOperation,
    },
    ReplayBodyConflict {
        key: String,
        operation: PresenceOperation,
    },
    InactiveContract(String),
    Domain(String),
}

impl fmt::Display for PresenceRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFrame => f.write_str("Presence has no open frame for this session"),
            Self::ReplayOperationConflict {
                key,
                recorded,
                attempted,
            } => write!(
                f,
                "Presence idempotency key {key} already answered a {recorded} \
                 and cannot be reused for a {attempted}"
            ),
            Self::ReplayBodyConflict { key, operation } => write!(
                f,
                "Presence idempotency key {key} already answered a different {operation} body"
            ),
            Self::InactiveContract(contract_id) => {
                write!(f, "Presence contract {contract_id} is not active")
            }
            Self::Domain(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for PresenceRuntimeError {}

impl PresenceRuntime {
    /// Open the one live frame for an authenticated session.
    ///
    /// Replay is checked before the lifecycle: an exact retry of the original
    /// key and body returns the original frame whatever happened since. When
    /// the session already holds a live frame, any further open — a new key,
    /// or the original key carrying fresh materials — is answered with the
    /// live frame instead of refused. A client that reopens is a client that
    /// lost its own context (compaction, restart); the Host owns the ledger,
    /// so the door answers with the truth it holds. The caller's claimed
    /// binding is still checked against the authenticated one first, so an
    /// impostor adopts nothing.
    pub fn open(
        &mut self,
        authentication: &PresenceAuthentication,
        idempotency_key: &str,
        request: PresenceOpenRequest,
    ) -> Result<PresenceFrame, PresenceRuntimeError> {
        let session = authentication.binding.session.clone();
        let request_digest = digest_request(&(authentication, &request))?;
        let mut record_adoption = false;
        let mut recorded_conflict = None;
        match self.replay.recall(
            &session,
            idempotency_key,
            PresenceOperation::Open,
            &request_digest,
            opened,
        ) {
            Ok(Some(frame)) => return Ok(frame),
            Ok(None) => record_adoption = true,
            Err(conflict @ PresenceRuntimeError::ReplayOperationConflict { .. }) => {
                return Err(conflict);
            }
            // A reused open key with a different body is exactly what a
            // reopening client looks like; hold the conflict until the live
            // frame has had its say.
            Err(body_conflict) => recorded_conflict = Some(body_conflict),
        }
        if let Some(live) = self.sessions.get(&session) {
            if request.binding != authentication.binding {
                return Err(PresenceRuntimeError::Domain(
                    "invalid Presence binding: does not match the authenticated room state".into(),
                ));
            }
            // The live frame answers only the binding it was opened for; a
            // Host whose authenticated binding moved must close and reopen.
            if live.frame.binding != authentication.binding {
                return Err(recorded_conflict.unwrap_or_else(|| {
                    PresenceRuntimeError::Domain(
                        "Presence frame is bound to a different authenticated binding; \
                         close it before opening anew"
                            .into(),
                    )
                }));
            }
            let frame = live.frame.clone();
            if record_adoption {
                self.replay.record(ReplayEntry {
                    session,
                    key: idempotency_key.to_owned(),
                    operation: PresenceOperation::Open,
                    request_digest,
                    outcome: PresenceResult::Open(frame.clone()),
                });
            }
            return Ok(frame);
        }
        if let Some(conflict) = recorded_conflict {
            return Err(conflict);
        }
        let frame = open_presence(authentication.clone(), request).map_err(domain)?;
        self.sessions.insert(
            session.clone(),
            PresenceSession {
                ledger: PresenceLedger {
                    frame_version: frame.version,
                    contract_version: 0,
                    ..PresenceLedger::default()
                },
                frame: frame.clone(),
                active_contract: None,
                receipts: VecDeque::new(),
            },
        );
        self.replay.record(ReplayEntry {
            session,
            key: idempotency_key.to_owned(),
            operation: PresenceOperation::Open,
            request_digest,
            outcome: PresenceResult::Open(frame.clone()),
        });
        Ok(frame)
    }

    /// Compile the turn contract against the Host's ledger, advancing its
    /// contract counter on a fresh compile.
    ///
    /// A replayed compile returns its recorded contract without reinstalling
    /// it, so an expired contract does not become settleable again.
    pub fn compile(
        &mut self,
        session: &str,
        idempotency_key: &str,
        request: PresenceTurnRequest,
    ) -> Result<PresenceContract, PresenceRuntimeError> {
        let request_digest = digest_request(&request)?;
        if let Some(contract) = self.replay.recall(
            session,
            idempotency_key,
            PresenceOperation::Compile,
            &request_digest,
            compiled,
        )? {
            return Ok(contract);
        }
        let state = self
            .sessions
            .get_mut(session)
            .ok_or(PresenceRuntimeError::MissingFrame)?;
        let mut ledger = state.ledger.clone();
        ledger.contract_version = ledger.contract_version.saturating_add(1);
        let contract = compile_presence(&state.frame, &ledger, request).map_err(domain)?;
        state.ledger = ledger;
        state.active_contract = Some(contract.clone());
        self.replay.record(ReplayEntry {
            session: session.to_owned(),
            key: idempotency_key.to_owned(),
            operation: PresenceOperation::Compile,
            request_digest,
            outcome: PresenceResult::Compile(contract.clone()),
        });
        Ok(contract)
    }

    /// Settle the active contract and fold what it taught into the ledger.
    pub fn settle(
        &mut self,
        session: &str,
        idempotency_key: &str,
        request: PresenceSettleRequest,
    ) -> Result<PresenceReceipt, PresenceRuntimeError> {
        let request_digest = digest_request(&request)?;
        if let Some(receipt) = self.replay.recall(
            session,
            idempotency_key,
            PresenceOperation::Settle,
            &request_digest,
            settled,
        )? {
            return Ok(receipt);
        }
        let state = self
            .sessions
            .get_mut(session)
            .ok_or(PresenceRuntimeError::MissingFrame)?;
        let contract = state
            .active_contract
            .as_ref()
            .filter(|contract| contract.contract_id == request.contract_id)
            .ok_or_else(|| PresenceRuntimeError::InactiveContract(request.contract_id.clone()))?;
        let receipt = settle_presence(contract, request).map_err(domain)?;
        absorb_receipt(&mut state.ledger, &receipt);
        state.receipts.push_back(receipt.clone());
        while state.receipts.len() > PRESENCE_MAX_SESSION_RECEIPTS {
            state.receipts.pop_front();
        }
        self.replay.record(ReplayEntry {
            session: session.to_owned(),
            key: idempotency_key.to_owned(),
            operation: PresenceOperation::Settle,
            request_digest,
            outcome: PresenceResult::Settle(receipt.clone()),
        });
        Ok(receipt)
    }

    /// Seal the close material against the Host's ledger and retire the
    /// session.
    ///
    /// The request is digested as it arrived, before a blank frame ID is
    /// filled in from live state. That is what lets the same close replay
    /// after the session is gone.
    pub fn close(
        &mut self,
        session: &str,
        idempotency_key: &str,
        mut request: PresenceCloseRequest,
    ) -> Result<PresenceCloseMaterial, PresenceRuntimeError> {
        let request_digest = digest_request(&request)?;
        if let Some(material) = self.replay.recall(
            session,
            idempotency_key,
            PresenceOperation::Close,
            &request_digest,
            closed,
        )? {
            return Ok(material);
        }
        let material = {
            let state = self
                .sessions
                .get(session)
                .ok_or(PresenceRuntimeError::MissingFrame)?;
            if request.frame_id.trim().is_empty() {
                request.frame_id = state.frame.frame_id.clone();
            }
            close_presence(&state.frame, &state.ledger, request).map_err(domain)?
        };
        self.sessions.remove(session);
        self.replay.record(ReplayEntry {
            session: session.to_owned(),
            key: idempotency_key.to_owned(),
            operation: PresenceOperation::Close,
            request_digest,
            outcome: PresenceResult::Close(material.clone()),
        });
        Ok(material)
    }
}

impl ReplayLedger {
    fn recall<T>(
        &self,
        session: &str,
        key: &str,
        operation: PresenceOperation,
        request_digest: &str,
        project: fn(&PresenceResult) -> Option<T>,
    ) -> Result<Option<T>, PresenceRuntimeError> {
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.session == session && entry.key == key)
        else {
            return Ok(None);
        };
        if entry.operation != operation {
            return Err(PresenceRuntimeError::ReplayOperationConflict {
                key: key.to_owned(),
                recorded: entry.operation,
                attempted: operation,
            });
        }
        if entry.request_digest != request_digest {
            return Err(PresenceRuntimeError::ReplayBodyConflict {
                key: key.to_owned(),
                operation,
            });
        }
        project(&entry.outcome).map(Some).ok_or_else(|| {
            PresenceRuntimeError::Domain(format!(
                "Presence replay for {key} stored an outcome that is not a {operation}"
            ))
        })
    }

    fn record(&mut self, entry: ReplayEntry) {
        self.entries.push_back(entry);
        while self.entries.len() > PRESENCE_MAX_REPLAY_ENTRIES {
            self.entries.pop_front();
        }
    }
}

/// The capabilities the Host can prove for itself, and only those.
///
/// Answers from configuration the Host already resolved, never from a caller
/// assertion. Room state is unconditional because the Host refuses to start
/// without parsing it. GIGA and Omega have no Host-side probe, so they are not
/// listed.
pub fn host_capabilities(akasha: bool, receipts: bool) -> Vec<PresenceCapability> {
    let mut capabilities = vec![PresenceCapability::RoomState];
    if akasha {
        capabilities.push(PresenceCapability::Akasha);
    }
    if receipts {
        capabilities.push(PresenceCapability::Receipts);
    }
    capabilities
}

/// Fold a settlement into the session's ledger.
///
/// Only violated directive IDs travel, bounded and newest-first, so a long
/// session does not grow the packet every later turn pays for.
fn absorb_receipt(ledger: &mut PresenceLedger, receipt: &PresenceReceipt) {
    for violation in &receipt.violations {
        if ledger
            .repair_rule_ids
            .iter()
            .any(|id| id == &violation.directive_id)
        {
            continue;
        }
        ledger.repair_rule_ids.push(violation.directive_id.clone());
    }
    while ledger.repair_rule_ids.len() > PRESENCE_MAX_REPAIR_RULES {
        ledger.repair_rule_ids.remove(0);
    }
}

fn digest_request(value: &impl serde::Serialize) -> Result<String, PresenceRuntimeError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        PresenceRuntimeError::Domain(format!("Presence request does not serialize: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn opened(result: &PresenceResult) -> Option<PresenceFrame> {
    match result {
        PresenceResult::Open(frame) => Some(frame.clone()),
        _ => None,
    }
}

fn compiled(result: &PresenceResult) -> Option<PresenceContract> {
    match result {
        PresenceResult::Compile(contract) => Some(contract.clone()),
        _ => None,
    }
}

fn settled(result: &PresenceResult) -> Option<PresenceReceipt> {
    match result {
        PresenceResult::Settle(receipt) => Some(receipt.clone()),
        _ => None,
    }
}

fn closed(result: &PresenceResult) -> Option<PresenceCloseMaterial> {
    match result {
        PresenceResult::Close(material) => Some(material.clone()),
        _ => None,
    }
}

fn domain(error: impl ToString) -> PresenceRuntimeError {
    PresenceRuntimeError::Domain(error.to_string())
}
