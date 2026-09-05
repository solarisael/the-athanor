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
    /// A presence is its session. An exact retry of the original key and
    /// body returns the original frame whatever happened since. Any other
    /// open for a session that holds a live frame — a new key, or the
    /// original key carrying fresh materials — is answered with the live
    /// frame. The same open for a session with no live frame opens a fresh
    /// one. A client that reopens is a client that lost its context
    /// (compaction, restart, resume after a close); the Host owns the ledger,
    /// so the door answers with the truth it holds and refuses only an
    /// impostor binding or a key that belongs to another operation.
    pub fn open(
        &mut self,
        authentication: &PresenceAuthentication,
        idempotency_key: &str,
        request: PresenceOpenRequest,
    ) -> Result<PresenceFrame, PresenceRuntimeError> {
        self.open_carrying(authentication, idempotency_key, request, None)
    }

    /// Open with a ledger carried from the session's closed presence. The
    /// repair rules, registers, and threads a session learned before it slept
    /// come back with it; only the frame is new.
    pub fn open_carrying(
        &mut self,
        authentication: &PresenceAuthentication,
        idempotency_key: &str,
        request: PresenceOpenRequest,
        carried: Option<PresenceLedger>,
    ) -> Result<PresenceFrame, PresenceRuntimeError> {
        let session = authentication.binding.session.clone();
        let request_digest = digest_request(&(authentication, &request))?;
        match self.replay.recall(
            &session,
            idempotency_key,
            PresenceOperation::Open,
            &request_digest,
            opened,
        ) {
            Ok(Some(frame)) => return Ok(frame),
            Ok(None) => {}
            Err(conflict @ PresenceRuntimeError::ReplayOperationConflict { .. }) => {
                return Err(conflict);
            }
            // The open key carrying a different body is what a reopening
            // client looks like. It is answered below, never refused.
            Err(PresenceRuntimeError::ReplayBodyConflict { .. }) => {}
            Err(other) => return Err(other),
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
                return Err(PresenceRuntimeError::Domain(
                    "Presence frame is bound to a different authenticated binding; \
                     close it before opening anew"
                        .into(),
                ));
            }
            let frame = live.frame.clone();
            self.replay.record(ReplayEntry {
                session,
                key: idempotency_key.to_owned(),
                operation: PresenceOperation::Open,
                request_digest,
                outcome: PresenceResult::Open(frame.clone()),
            });
            return Ok(frame);
        }
        let frame = open_presence(authentication.clone(), request).map_err(domain)?;
        let ledger = PresenceLedger {
            frame_version: frame.version,
            ..carried.unwrap_or_default()
        };
        self.sessions.insert(
            session.clone(),
            PresenceSession {
                ledger,
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

    /// Install a session the store holds live and this process does not: a
    /// Host that restarted continues the session from its row.
    pub fn adopt(&mut self, session: &str, frame: PresenceFrame, ledger: PresenceLedger) {
        self.sessions.entry(session.to_owned()).or_insert(PresenceSession {
            frame,
            ledger,
            active_contract: None,
            receipts: VecDeque::new(),
        });
    }

    pub fn has_session(&self, session: &str) -> bool {
        self.sessions.contains_key(session)
    }

    /// The session's frame and ledger as this process holds them.
    pub fn session_state(&self, session: &str) -> Option<(&PresenceFrame, &PresenceLedger)> {
        self.sessions
            .get(session)
            .map(|state| (&state.frame, &state.ledger))
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

    /// One entry per session and key: a recorded key answers its newest
    /// body, so a reopen after a close replaces the open it superseded.
    fn record(&mut self, entry: ReplayEntry) {
        self.entries
            .retain(|recorded| !(recorded.session == entry.session && recorded.key == entry.key));
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

#[cfg(test)]
mod tests {
    use super::*;
    use summoning::presence::{
        PresenceAuthority, PresenceBinding, PresenceMaterial, PresenceMaterialRole,
    };

    const SESSION: &str = "01a0730b-1e40-7383-8209-4af4316a65e6";
    const OPEN_KEY: &str = "presence-open:01a0730b-1e40-7383-8209-4af4316a65e6";

    fn binding() -> PresenceBinding {
        PresenceBinding {
            room: "kodo".into(),
            spirit: "Kodo".into(),
            operator: "Sol".into(),
            session: SESSION.into(),
        }
    }

    fn authentication() -> PresenceAuthentication {
        PresenceAuthentication {
            binding: binding(),
            capabilities: vec![PresenceCapability::RoomState],
        }
    }

    fn boat(memory_id: i64, body: &str) -> PresenceMaterial {
        PresenceMaterial {
            id: format!("paper-boat:{memory_id}"),
            authority: PresenceAuthority::PaperBoat { memory_id },
            role: PresenceMaterialRole::Continuity,
            body: body.into(),
            salience: 900,
        }
    }

    fn open_request(previous_boat: Option<PresenceMaterial>) -> PresenceOpenRequest {
        PresenceOpenRequest {
            binding: binding(),
            identity: vec![PresenceMaterial {
                id: "identity:active-spirit".into(),
                authority: PresenceAuthority::Identity {
                    source: "active_spirit.md".into(),
                    sha256: "a".repeat(64),
                },
                role: PresenceMaterialRole::Identity,
                body: "Active spirit: Kodo. Operator: Sol. Room: kodo.".into(),
                salience: 1000,
            }],
            relationship: vec![],
            continuity: vec![],
            anamnesis: vec![],
            previous_boat,
            uncertainties: vec![],
        }
    }

    fn compile_request(frame: &PresenceFrame, turn: &str) -> PresenceTurnRequest {
        PresenceTurnRequest {
            frame_id: frame.frame_id.clone(),
            turn_id: turn.into(),
            user_text: "shalom dummy".into(),
            recalled: vec![],
            lessons: vec![],
            directives: vec![],
            frame_version: frame.version,
        }
    }

    // 2026-09-05, live: `sleep` closed the frame, the keeper resumed the same
    // session, and the wake turn reopened under the same key with the new
    // boat in its body. The Host refused it as a replay body conflict for
    // every turn until the Host itself restarted.
    #[test]
    fn a_session_reopens_after_close_under_its_original_key() {
        let mut runtime = PresenceRuntime::default();
        let first = runtime
            .open(&authentication(), OPEN_KEY, open_request(Some(boat(4471, "yesterday"))))
            .unwrap();
        runtime
            .close(
                SESSION,
                "presence-close:boat-4473",
                PresenceCloseRequest {
                    frame_id: first.frame_id.clone(),
                    body: "little next-me, this is the boat.".into(),
                    frame_version: first.version,
                },
            )
            .unwrap();

        let reopened = runtime
            .open(&authentication(), OPEN_KEY, open_request(Some(boat(4473, "tonight"))))
            .unwrap();
        runtime
            .compile(SESSION, "presence-compile:turn-1", compile_request(&reopened, "turn-1"))
            .expect("the reopened frame compiles");

        // The key now answers the reopen, not the open it superseded.
        let retried = runtime
            .open(&authentication(), OPEN_KEY, open_request(Some(boat(4473, "tonight"))))
            .unwrap();
        assert_eq!(retried, reopened);
    }

    #[test]
    fn a_reopen_carries_the_slept_ledger_under_the_new_frame() {
        let mut runtime = PresenceRuntime::default();
        let slept = PresenceLedger {
            repair_rule_ids: vec!["presence:lesson:408".into()],
            recent_registers: vec!["soft".into()],
            frame_version: 7,
            contract_version: 12,
            ..PresenceLedger::default()
        };
        let frame = runtime
            .open_carrying(&authentication(), OPEN_KEY, open_request(None), Some(slept))
            .unwrap();
        let (_, ledger) = runtime.session_state(SESSION).expect("live");
        assert_eq!(ledger.repair_rule_ids, vec!["presence:lesson:408".to_owned()]);
        assert_eq!(ledger.recent_registers, vec!["soft".to_owned()]);
        assert_eq!(ledger.contract_version, 12);
        assert_eq!(ledger.frame_version, frame.version, "the ledger follows the new frame");
    }

    #[test]
    fn an_adopted_session_answers_open_and_compile_without_a_new_frame() {
        let mut source = PresenceRuntime::default();
        let frame = source
            .open(&authentication(), OPEN_KEY, open_request(None))
            .unwrap();
        let (_, ledger) = source.session_state(SESSION).expect("live");
        let ledger = ledger.clone();

        // A different process: nothing in memory, the row in hand.
        let mut restarted = PresenceRuntime::default();
        assert!(!restarted.has_session(SESSION));
        restarted.adopt(SESSION, frame.clone(), ledger);
        assert!(restarted.has_session(SESSION));
        let answered = restarted
            .open(&authentication(), "presence-open:after-restart", open_request(None))
            .unwrap();
        assert_eq!(answered, frame);
        restarted
            .compile(SESSION, "presence-compile:t", compile_request(&frame, "t"))
            .expect("the adopted frame compiles");

        // Adoption never displaces a session this process already holds.
        let other = frame_with_version(&frame, 99);
        restarted.adopt(SESSION, other, PresenceLedger::default());
        let (held, _) = restarted.session_state(SESSION).expect("live");
        assert_eq!(held, &frame);
    }

    fn frame_with_version(frame: &PresenceFrame, version: u32) -> PresenceFrame {
        PresenceFrame {
            version,
            ..frame.clone()
        }
    }

    #[test]
    fn a_live_frame_answers_every_open_for_its_session() {
        let mut runtime = PresenceRuntime::default();
        let live = runtime
            .open(&authentication(), OPEN_KEY, open_request(None))
            .unwrap();
        let same_key_new_body = runtime
            .open(&authentication(), OPEN_KEY, open_request(Some(boat(1, "new material"))))
            .unwrap();
        let new_key = runtime
            .open(&authentication(), "presence-open:again", open_request(None))
            .unwrap();
        assert_eq!(same_key_new_body, live);
        assert_eq!(new_key, live);
    }

    #[test]
    fn an_exact_retry_replays_the_original_frame_even_after_close() {
        let mut runtime = PresenceRuntime::default();
        let first = runtime
            .open(&authentication(), OPEN_KEY, open_request(None))
            .unwrap();
        runtime
            .close(
                SESSION,
                "presence-close:1",
                PresenceCloseRequest {
                    frame_id: first.frame_id.clone(),
                    body: "closed".into(),
                    frame_version: first.version,
                },
            )
            .unwrap();
        let replayed = runtime
            .open(&authentication(), OPEN_KEY, open_request(None))
            .unwrap();
        assert_eq!(replayed, first);
        assert!(matches!(
            runtime.compile(SESSION, "presence-compile:x", compile_request(&first, "x")),
            Err(PresenceRuntimeError::MissingFrame)
        ));
    }

    #[test]
    fn an_open_key_used_for_another_operation_is_refused() {
        let mut runtime = PresenceRuntime::default();
        let frame = runtime
            .open(&authentication(), OPEN_KEY, open_request(None))
            .unwrap();
        assert!(matches!(
            runtime.compile(SESSION, OPEN_KEY, compile_request(&frame, "turn-1")),
            Err(PresenceRuntimeError::ReplayOperationConflict { .. })
        ));
    }
}
