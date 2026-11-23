//! One failure shape on the Host wire (issues #3 and #56, slice A1).
//!
//! The code at the failure point declares the class. Nothing infers a class
//! from message text: the class decides what the agent does next, and `code`
//! keeps the exact word a caller can act on (`revision_conflict`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::panic::Location;

/// The closed list of failure classes Sol approved on 2026-11-23 (#3).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The call is wrong: unknown key, bad value, missing field. Fix the call.
    InputRefused,
    /// A grant, consent, or capability is missing. Do not retry.
    NotPermitted,
    /// The state under the call is not what it assumed. Read, then decide.
    Conflict,
    /// A write was sent and no result came back. Read before any retry.
    OutcomeUnknown,
    /// The connect step to the Host or a provider failed or was too slow.
    NoConnection,
    /// The provider answered 429 or 529. Wait for the stated time.
    ProviderBusy,
    /// The provider failed repeatedly. The House skips it for a while.
    ProviderDown,
    /// A limit ran out on a live connection.
    Deadline,
    /// The turn budget ran out; the result is partial.
    BudgetExhausted,
    /// One layer sent a shape the next layer does not accept.
    InvalidResponse,
    /// The install or environment is missing something.
    Configuration,
    /// An invariant broke. A bug.
    Internal,
}

impl FailureClass {
    /// True when the same call can succeed after the stated wait, without a change.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::NoConnection | Self::ProviderBusy | Self::ProviderDown
        )
    }
}

/// One boundary the failure crossed. Slice A1 records the hop where the
/// failure was built; slice A2 adds the chain.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FailureHop {
    pub layer: String,
    pub file: String,
    pub line: u32,
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub class: FailureClass,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub hops: Vec<FailureHop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl Failure {
    /// Builds a failure and records the caller's file and line as its first hop.
    /// The layer is the crate (`crates/<layer>/...`); the operation defaults to
    /// the code until [`Failure::op`] names it.
    #[track_caller]
    pub fn new(class: FailureClass, code: impl Into<String>, message: impl Into<String>) -> Self {
        let location = Location::caller();
        let code = code.into();
        Self {
            class,
            hops: vec![FailureHop {
                layer: layer_of(location.file()).to_owned(),
                file: location.file().replace('\\', "/"),
                line: location.line(),
                op: code.clone(),
                observed: None,
            }],
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Names the operation of the hop this failure was built at.
    pub fn op(mut self, op: impl Into<String>) -> Self {
        if let Some(hop) = self.hops.last_mut() {
            hop.op = op.into();
        }
        self
    }

    /// Records the one value that matters at the hop this failure was built at.
    pub fn observed(mut self, observed: impl Into<String>) -> Self {
        if let Some(hop) = self.hops.last_mut() {
            hop.observed = Some(observed.into());
        }
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn retryable(&self) -> bool {
        self.class.retryable()
    }
}

fn layer_of(file: &str) -> &str {
    let mut parts = file.split(['/', '\\']);
    match (parts.next(), parts.next()) {
        (Some("crates"), Some(layer)) => layer,
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_records_where_it_was_built() {
        let line = line!() + 1;
        let failure = Failure::new(FailureClass::Conflict, "revision_conflict", "changed")
            .op("resource_write");

        assert_eq!(failure.hops.len(), 1);
        assert_eq!(failure.hops[0].line, line);
        assert!(failure.hops[0].file.ends_with("failure.rs"));
        assert_eq!(failure.hops[0].op, "resource_write");
    }

    #[test]
    fn the_wire_shape_is_snake_case_and_refuses_unknown_fields() {
        let failure = Failure::new(FailureClass::InputRefused, "invalid_params", "bad");
        let wire = serde_json::to_value(&failure).unwrap();
        assert_eq!(wire["class"], "input_refused");

        let mut extra = wire.clone();
        extra["reason"] = Value::String("old".into());
        assert!(serde_json::from_value::<Failure>(extra).is_err());
    }
}
