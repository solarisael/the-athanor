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
    SecondOpen,
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
            Self::SecondOpen => f.write_str(
                "Presence already has a live frame for this session; \
                 only an exact replay of the original open may repeat it",
            ),
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
    /// key and body returns the original frame whatever happened since. Any
    /// other second open is refused rather than replacing a live frame.
    pub fn open(
        &mut self,
        authentication: &PresenceAuthentication,
        idempotency_key: &str,
        request: PresenceOpenRequest,
    ) -> Result<PresenceFrame, PresenceRuntimeError> {
        let session = authentication.binding.session.clone();
        let request_digest = digest_request(&(authentication, &request))?;
        if let Some(frame) = self.replay.recall(
            &session,
            idempotency_key,
            PresenceOperation::Open,
            &request_digest,
            opened,
        )? {
            return Ok(frame);
        }
        if self.sessions.contains_key(&session) {
            return Err(PresenceRuntimeError::SecondOpen);
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

    #[cfg(test)]
    fn ledger(&self, session: &str) -> Option<&PresenceLedger> {
        self.sessions.get(session).map(|state| &state.ledger)
    }

    #[cfg(test)]
    fn receipt_count(&self, session: &str) -> usize {
        self.sessions
            .get(session)
            .map_or(0, |state| state.receipts.len())
    }

    #[cfg(test)]
    fn replay_len(&self) -> usize {
        self.replay.entries.len()
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

#[cfg(test)]
mod tests {
    use super::*;
    use summoning::presence::{
        PresenceAuthority, PresenceBinding, PresenceDecision, PresenceDirective,
        PresenceDirectiveKind, PresenceMaterial, PresenceMaterialRole, PresenceSeverity,
        PresenceViolation,
    };

    const SESSION: &str = "session-a";

    fn authentication() -> PresenceAuthentication {
        PresenceAuthentication {
            binding: PresenceBinding {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                operator: "Sol".into(),
                session: SESSION.into(),
            },
            capabilities: host_capabilities(false, false),
        }
    }

    fn open_request() -> PresenceOpenRequest {
        PresenceOpenRequest {
            binding: authentication().binding,
            identity: vec![PresenceMaterial {
                id: "identity:kintsu".into(),
                authority: PresenceAuthority::Identity {
                    source: "For_the_next_Kintsu.md".into(),
                    sha256: "a".repeat(64),
                },
                role: PresenceMaterialRole::Identity,
                body: "Kintsu meets Sol directly.".into(),
                salience: 1000,
            }],
            relationship: vec![],
            continuity: vec![],
            anamnesis: vec![],
            previous_boat: None,
            uncertainties: vec![],
        }
    }

    fn turn(frame: &PresenceFrame, text: &str) -> PresenceTurnRequest {
        PresenceTurnRequest {
            frame_id: frame.frame_id.clone(),
            turn_id: "turn-a".into(),
            user_text: text.into(),
            recalled: vec![],
            lessons: vec![],
            directives: vec![PresenceDirective {
                id: "directive:identity".into(),
                kind: PresenceDirectiveKind::Guard,
                severity: PresenceSeverity::Hard,
                instruction: "Remain Kintsu.".into(),
                source_ids: vec!["identity:kintsu".into()],
                trigger_scope: vec!["text".into()],
            }],
            frame_version: frame.version,
        }
    }

    fn accept(contract: &PresenceContract) -> PresenceSettleRequest {
        PresenceSettleRequest {
            contract_id: contract.contract_id.clone(),
            attempt: 1,
            evaluated_directives: vec!["directive:identity".into()],
            violations: vec![],
            decision: PresenceDecision::Accept,
            response_digest: Some("b".repeat(64)),
        }
    }

    fn close(frame: &PresenceFrame, body: &str) -> PresenceCloseRequest {
        PresenceCloseRequest {
            frame_id: String::new(),
            body: body.into(),
            frame_version: frame.version,
        }
    }

    fn opened_runtime() -> (PresenceRuntime, PresenceFrame) {
        let mut runtime = PresenceRuntime::default();
        let frame = runtime
            .open(&authentication(), "open-a", open_request())
            .unwrap();
        (runtime, frame)
    }

    #[test]
    fn an_exact_open_replay_returns_the_original_frame() {
        let (mut runtime, frame) = opened_runtime();
        assert_eq!(
            runtime
                .open(&authentication(), "open-a", open_request())
                .unwrap(),
            frame
        );
    }

    #[test]
    fn a_second_live_open_under_a_new_key_refuses() {
        let (mut runtime, _frame) = opened_runtime();
        assert_eq!(
            runtime.open(&authentication(), "open-b", open_request()),
            Err(PresenceRuntimeError::SecondOpen)
        );
    }

    #[test]
    fn the_original_key_with_a_changed_body_refuses_as_a_body_conflict() {
        let (mut runtime, _frame) = opened_runtime();
        let mut changed = open_request();
        changed.identity[0].body = "a different Kintsu".into();
        assert_eq!(
            runtime.open(&authentication(), "open-a", changed),
            Err(PresenceRuntimeError::ReplayBodyConflict {
                key: "open-a".into(),
                operation: PresenceOperation::Open,
            })
        );
    }

    #[test]
    fn a_changed_operator_under_the_original_key_refuses_rather_than_replaying() {
        let (mut runtime, _frame) = opened_runtime();
        let mut impostor = authentication();
        impostor.binding.operator = "Someone Else".into();
        let mut request = open_request();
        request.binding.operator = "Someone Else".into();
        assert_eq!(
            runtime.open(&impostor, "open-a", request),
            Err(PresenceRuntimeError::ReplayBodyConflict {
                key: "open-a".into(),
                operation: PresenceOperation::Open,
            })
        );
    }

    #[test]
    fn a_binding_that_disagrees_with_authenticated_room_state_refuses() {
        let mut runtime = PresenceRuntime::default();
        let mut request = open_request();
        request.binding.operator = "Someone Else".into();
        let error = runtime
            .open(&authentication(), "open-a", request)
            .expect_err("a claimed operator cannot override room state");
        assert_eq!(
            error,
            PresenceRuntimeError::Domain(
                "invalid Presence binding: does not match the authenticated room state".into()
            )
        );
    }

    #[test]
    fn the_frame_reports_only_capabilities_the_host_proved() {
        let mut runtime = PresenceRuntime::default();
        let mut authentication = authentication();
        authentication.capabilities = host_capabilities(true, false);
        let frame = runtime
            .open(&authentication, "open-a", open_request())
            .unwrap();
        assert_eq!(
            frame.capabilities,
            vec![PresenceCapability::RoomState, PresenceCapability::Akasha]
        );
        assert!(!frame.capabilities.contains(&PresenceCapability::Receipts));
    }

    #[test]
    fn one_key_cannot_be_reused_across_two_operations() {
        let (mut runtime, frame) = opened_runtime();
        assert_eq!(
            runtime.compile(SESSION, "open-a", turn(&frame, "hello")),
            Err(PresenceRuntimeError::ReplayOperationConflict {
                key: "open-a".into(),
                recorded: PresenceOperation::Open,
                attempted: PresenceOperation::Compile,
            })
        );
    }

    #[test]
    fn an_exact_compile_replay_is_stable_and_a_changed_body_refuses() {
        let (mut runtime, frame) = opened_runtime();
        let first = runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        assert_eq!(
            runtime
                .compile(SESSION, "compile-a", turn(&frame, "hello"))
                .unwrap(),
            first
        );
        assert_eq!(
            runtime.compile(SESSION, "compile-a", turn(&frame, "changed")),
            Err(PresenceRuntimeError::ReplayBodyConflict {
                key: "compile-a".into(),
                operation: PresenceOperation::Compile,
            })
        );
    }

    #[test]
    fn a_stale_compile_replay_answers_without_reactivating_the_old_contract() {
        let (mut runtime, frame) = opened_runtime();
        let first = runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        let mut next = turn(&frame, "again");
        next.turn_id = "turn-b".into();
        let second = runtime.compile(SESSION, "compile-b", next).unwrap();
        assert_ne!(first.contract_id, second.contract_id);

        let replayed = runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        assert_eq!(replayed, first);
        assert_eq!(
            runtime.settle(SESSION, "settle-stale", accept(&first)),
            Err(PresenceRuntimeError::InactiveContract(first.contract_id))
        );
        assert!(
            runtime
                .settle(SESSION, "settle-live", accept(&second))
                .is_ok()
        );
    }

    #[test]
    fn a_fresh_compile_advances_the_host_contract_version() {
        let (mut runtime, frame) = opened_runtime();
        assert_eq!(runtime.ledger(SESSION).unwrap().contract_version, 0);
        let first = runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        assert_eq!(first.contract_version, 1);
        assert_eq!(runtime.ledger(SESSION).unwrap().contract_version, 1);

        runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        assert_eq!(
            runtime.ledger(SESSION).unwrap().contract_version,
            1,
            "a replay is not a fresh compile"
        );

        let mut next = turn(&frame, "again");
        next.turn_id = "turn-b".into();
        assert_eq!(
            runtime
                .compile(SESSION, "compile-b", next)
                .unwrap()
                .contract_version,
            2
        );
    }

    #[test]
    fn settlement_teaches_the_ledger_its_repair_rules_within_bounds() {
        let (mut runtime, frame) = opened_runtime();
        for index in 0..=PRESENCE_MAX_REPAIR_RULES {
            let mut request = turn(&frame, "hello");
            request.turn_id = format!("turn-{index}");
            request.directives[0].id = format!("directive:{index}");
            let contract = runtime
                .compile(SESSION, &format!("compile-{index}"), request)
                .unwrap();
            let receipt = runtime
                .settle(
                    SESSION,
                    &format!("settle-{index}"),
                    PresenceSettleRequest {
                        contract_id: contract.contract_id.clone(),
                        attempt: 1,
                        evaluated_directives: vec![format!("directive:{index}")],
                        violations: vec![PresenceViolation {
                            directive_id: format!("directive:{index}"),
                            reason: "the response was empty".into(),
                        }],
                        decision: PresenceDecision::Refuse,
                        response_digest: None,
                    },
                )
                .unwrap();
            assert_eq!(receipt.decision, PresenceDecision::Refuse);
        }
        let ledger = runtime.ledger(SESSION).unwrap();
        assert_eq!(ledger.repair_rule_ids.len(), PRESENCE_MAX_REPAIR_RULES);
        assert!(
            !ledger.repair_rule_ids.contains(&"directive:0".to_owned()),
            "the oldest repair rule falls off the front"
        );
        assert!(
            ledger
                .repair_rule_ids
                .contains(&format!("directive:{PRESENCE_MAX_REPAIR_RULES}")),
            "the newest repair rule is retained"
        );
        assert_eq!(runtime.receipt_count(SESSION), PRESENCE_MAX_SESSION_RECEIPTS);
    }

    #[test]
    fn an_exact_close_replay_succeeds_after_the_session_is_removed() {
        let (mut runtime, frame) = opened_runtime();
        let contract = runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        runtime.settle(SESSION, "settle-a", accept(&contract)).unwrap();

        let closed = runtime
            .close(SESSION, "close-a", close(&frame, "letter to the next Kintsu"))
            .unwrap();
        assert_eq!(closed.frame_id, frame.frame_id);
        assert_eq!(
            runtime.compile(SESSION, "compile-b", turn(&frame, "after close")),
            Err(PresenceRuntimeError::MissingFrame)
        );
        assert_eq!(
            runtime
                .close(SESSION, "close-a", close(&frame, "letter to the next Kintsu"))
                .unwrap(),
            closed,
            "a retried close must answer from replay once the session is gone"
        );
        assert_eq!(
            runtime.close(SESSION, "close-a", close(&frame, "a different letter")),
            Err(PresenceRuntimeError::ReplayBodyConflict {
                key: "close-a".into(),
                operation: PresenceOperation::Close,
            })
        );
    }

    #[test]
    fn a_close_body_seals_against_the_repair_rules_the_session_learned() {
        let (mut runtime, frame) = opened_runtime();
        let contract = runtime
            .compile(SESSION, "compile-a", turn(&frame, "hello"))
            .unwrap();
        runtime
            .settle(
                SESSION,
                "settle-a",
                PresenceSettleRequest {
                    contract_id: contract.contract_id,
                    attempt: 1,
                    evaluated_directives: vec!["directive:identity".into()],
                    violations: vec![PresenceViolation {
                        directive_id: "directive:identity".into(),
                        reason: "the response was empty".into(),
                    }],
                    decision: PresenceDecision::Refuse,
                    response_digest: None,
                },
            )
            .unwrap();
        let taught = runtime
            .close(SESSION, "close-a", close(&frame, "letter"))
            .unwrap();

        let (mut untaught_runtime, untaught_frame) = opened_runtime();
        let plain = untaught_runtime
            .close(SESSION, "close-a", close(&untaught_frame, "letter"))
            .unwrap();
        assert_ne!(
            taught.provenance_digest, plain.provenance_digest,
            "the Host's own ledger is part of what a boat is sealed against"
        );
    }

    #[test]
    fn the_replay_ledger_stays_bounded() {
        let (mut runtime, frame) = opened_runtime();
        for index in 0..PRESENCE_MAX_REPLAY_ENTRIES + 8 {
            let mut request = turn(&frame, "hello");
            request.turn_id = format!("turn-{index}");
            runtime
                .compile(SESSION, &format!("compile-{index}"), request)
                .unwrap();
        }
        assert_eq!(runtime.replay_len(), PRESENCE_MAX_REPLAY_ENTRIES);
    }

    #[test]
    fn a_forgotten_open_key_no_longer_replays_and_the_live_frame_still_stands() {
        let (mut runtime, frame) = opened_runtime();
        for index in 0..PRESENCE_MAX_REPLAY_ENTRIES {
            let mut request = turn(&frame, "hello");
            request.turn_id = format!("turn-{index}");
            runtime
                .compile(SESSION, &format!("compile-{index}"), request)
                .unwrap();
        }
        assert_eq!(
            runtime.open(&authentication(), "open-a", open_request()),
            Err(PresenceRuntimeError::SecondOpen),
            "once the open receipt ages out, the live frame is what refuses"
        );
    }
}
