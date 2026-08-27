use std::collections::HashMap;
use std::fmt;

use presence::{
    PresenceCloseMaterial, PresenceCloseRequest, PresenceContract, PresenceFrame,
    PresenceOpenRequest, PresenceReceipt, PresenceSettleRequest, PresenceTurnRequest,
};
use presence_frame::open_presence;
use presence_turn::{close_presence, compile_presence, settle_presence};

#[derive(Default)]
pub struct PresenceRuntime {
    sessions: HashMap<String, PresenceSession>,
}

struct PresenceSession {
    frame: PresenceFrame,
    open_idempotency_key: String,
    contracts: HashMap<String, PresenceContract>,
    receipts: Vec<PresenceReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresenceRuntimeError {
    MissingFrame,
    OpenConflict,
    TurnConflict(String),
    ContractConflict(String),
    Domain(String),
}

impl fmt::Display for PresenceRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFrame => f.write_str("Presence has no open frame for this session"),
            Self::OpenConflict => {
                f.write_str("Presence open idempotency key was reused with different material")
            }
            Self::TurnConflict(turn_id) => {
                write!(
                    f,
                    "Presence turn {turn_id} was recompiled with different input"
                )
            }
            Self::ContractConflict(contract_id) => write!(
                f,
                "Presence contract {contract_id} was settled with different evidence"
            ),
            Self::Domain(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for PresenceRuntimeError {}

impl PresenceRuntime {
    pub fn open(
        &mut self,
        session: &str,
        idempotency_key: &str,
        request: PresenceOpenRequest,
    ) -> Result<PresenceFrame, PresenceRuntimeError> {
        if request.binding.session != session {
            return Err(PresenceRuntimeError::Domain(
                "Presence binding does not match the authenticated session".into(),
            ));
        }
        let frame = open_presence(request).map_err(domain)?;
        if let Some(current) = self.sessions.get(session) {
            if current.open_idempotency_key == idempotency_key {
                if current.frame == frame {
                    return Ok(current.frame.clone());
                }
                return Err(PresenceRuntimeError::OpenConflict);
            }
        }
        self.sessions.insert(
            session.to_owned(),
            PresenceSession {
                frame: frame.clone(),
                open_idempotency_key: idempotency_key.to_owned(),
                contracts: HashMap::new(),
                receipts: Vec::new(),
            },
        );
        Ok(frame)
    }

    pub fn compile(
        &mut self,
        session: &str,
        request: PresenceTurnRequest,
    ) -> Result<PresenceContract, PresenceRuntimeError> {
        let state = self
            .sessions
            .get_mut(session)
            .ok_or(PresenceRuntimeError::MissingFrame)?;
        let contract = compile_presence(&state.frame, request).map_err(domain)?;
        if let Some(current) = state.contracts.get(&contract.turn_id) {
            if current == &contract {
                return Ok(current.clone());
            }
            return Err(PresenceRuntimeError::TurnConflict(contract.turn_id));
        }
        state.contracts.clear();
        state
            .contracts
            .insert(contract.turn_id.clone(), contract.clone());
        Ok(contract)
    }

    pub fn settle(
        &mut self,
        session: &str,
        request: PresenceSettleRequest,
    ) -> Result<PresenceReceipt, PresenceRuntimeError> {
        let state = self
            .sessions
            .get_mut(session)
            .ok_or(PresenceRuntimeError::MissingFrame)?;
        let contract = state
            .contracts
            .values()
            .find(|contract| contract.contract_id == request.contract_id)
            .ok_or_else(|| {
                PresenceRuntimeError::Domain("Presence contract is not active".into())
            })?;
        let receipt = settle_presence(contract, request).map_err(domain)?;
        if let Some(current) = state.receipts.iter().find(|current| {
            current.contract_id == receipt.contract_id && current.attempt == receipt.attempt
        }) {
            if current == &receipt {
                return Ok(current.clone());
            }
            return Err(PresenceRuntimeError::ContractConflict(receipt.contract_id));
        }
        state.receipts.push(receipt.clone());
        Ok(receipt)
    }

    pub fn close(
        &mut self,
        session: &str,
        mut request: PresenceCloseRequest,
    ) -> Result<PresenceCloseMaterial, PresenceRuntimeError> {
        let material = {
            let state = self
                .sessions
                .get(session)
                .ok_or(PresenceRuntimeError::MissingFrame)?;
            if request.frame_id.trim().is_empty() {
                request.frame_id = state.frame.frame_id.clone();
            }
            close_presence(&state.frame, request).map_err(domain)?
        };
        self.sessions.remove(session);
        Ok(material)
    }
}

fn domain(error: impl ToString) -> PresenceRuntimeError {
    PresenceRuntimeError::Domain(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use presence::{
        PresenceAuthority, PresenceBinding, PresenceCloseRequest, PresenceDecision,
        PresenceDirective, PresenceDirectiveKind, PresenceLedger, PresenceMaterial,
        PresenceMaterialRole, PresenceSettleRequest, PresenceSeverity,
    };

    fn open_request() -> PresenceOpenRequest {
        PresenceOpenRequest {
            binding: PresenceBinding {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                operator: "Sol".into(),
                session: "session-a".into(),
            },
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
                kind: PresenceDirectiveKind::Enact,
                severity: PresenceSeverity::Hard,
                instruction: "Remain Kintsu.".into(),
                source_ids: vec!["identity:kintsu".into()],
                trigger_scope: vec!["text".into()],
            }],
            session_ledger: PresenceLedger {
                frame_version: frame.version,
                contract_version: 1,
                ..PresenceLedger::default()
            },
        }
    }

    #[test]
    fn retries_are_stable_and_changed_turns_refuse() {
        let mut runtime = PresenceRuntime::default();
        let frame = runtime.open("session-a", "open-a", open_request()).unwrap();
        assert_eq!(
            runtime.open("session-a", "open-a", open_request()).unwrap(),
            frame
        );
        let mut changed = open_request();
        changed.identity[0].body = "changed identity".into();
        assert_eq!(
            runtime.open("session-a", "open-a", changed),
            Err(PresenceRuntimeError::OpenConflict)
        );
        let first = runtime.compile("session-a", turn(&frame, "hello")).unwrap();
        assert_eq!(
            runtime.compile("session-a", turn(&frame, "hello")).unwrap(),
            first
        );
        assert!(matches!(
            runtime.compile("session-a", turn(&frame, "changed")),
            Err(PresenceRuntimeError::TurnConflict(_))
        ));
    }

    #[test]
    fn compiling_a_new_turn_expires_the_previous_contract() {
        let mut runtime = PresenceRuntime::default();
        let frame = runtime.open("session-a", "open-a", open_request()).unwrap();
        let first = runtime.compile("session-a", turn(&frame, "hello")).unwrap();
        let mut next_turn = turn(&frame, "again");
        next_turn.turn_id = "turn-b".into();
        runtime.compile("session-a", next_turn).unwrap();

        let stale_settle = PresenceSettleRequest {
            contract_id: first.contract_id,
            attempt: 1,
            evaluated_directives: vec!["directive:identity".into()],
            violations: vec![],
            decision: PresenceDecision::Accept,
            response_digest: Some("b".repeat(64)),
        };
        assert_eq!(
            runtime.settle("session-a", stale_settle),
            Err(PresenceRuntimeError::Domain(
                "Presence contract is not active".into()
            ))
        );
    }

    #[test]
    fn settle_is_idempotent_and_close_seals_the_existing_boat_body() {
        let mut runtime = PresenceRuntime::default();
        let frame = runtime.open("session-a", "open-a", open_request()).unwrap();
        let contract = runtime.compile("session-a", turn(&frame, "hello")).unwrap();
        let settle = PresenceSettleRequest {
            contract_id: contract.contract_id.clone(),
            attempt: 1,
            evaluated_directives: vec!["directive:identity".into()],
            violations: vec![],
            decision: PresenceDecision::Accept,
            response_digest: Some("b".repeat(64)),
        };
        let receipt = runtime.settle("session-a", settle.clone()).unwrap();
        assert_eq!(runtime.settle("session-a", settle).unwrap(), receipt);

        let closed = runtime
            .close(
                "session-a",
                PresenceCloseRequest {
                    frame_id: String::new(),
                    body: "letter to the next Kintsu".into(),
                    session_ledger: PresenceLedger {
                        frame_version: frame.version,
                        contract_version: 1,
                        ..PresenceLedger::default()
                    },
                },
            )
            .unwrap();
        assert_eq!(closed.frame_id, frame.frame_id);
        assert_eq!(closed.body, "letter to the next Kintsu");
        assert_eq!(
            runtime.compile("session-a", turn(&frame, "after close")),
            Err(PresenceRuntimeError::MissingFrame)
        );
    }
}
