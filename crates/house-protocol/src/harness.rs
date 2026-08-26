//! Local control wire between the Athanor owner and its GUI child.
//!
//! The token and loopback socket keep process authority out of the per-room Host.
//! Every harness uses this vocabulary; OMP-specific restart fields stay elsewhere.

use serde::{Deserialize, Serialize};

pub const HARNESS_CONTROL_FORMAT: u8 = 1;
const MAX_IDENTIFIER: usize = 128;
const MAX_DETAIL: usize = 512;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HarnessControlRequest {
    pub format: u8,
    pub request_id: String,
    pub token: String,
    pub command: HarnessCommand,
}

impl HarnessControlRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != HARNESS_CONTROL_FORMAT {
            return Err("unsupported harness control format".into());
        }
        bounded(&self.request_id, "requestId", MAX_IDENTIFIER)?;
        bounded(&self.token, "token", MAX_IDENTIFIER)?;
        self.command.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum HarnessCommand {
    List {},
    Start { harness_id: String },
    Stop { harness_id: String },
    Restart { harness_id: String },
}

impl HarnessCommand {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::List {} => Ok(()),
            Self::Start { harness_id }
            | Self::Stop { harness_id }
            | Self::Restart { harness_id } => bounded(harness_id, "harnessId", MAX_IDENTIFIER),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessLifecycle {
    Stopped,
    Running,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HarnessStatus {
    pub harness_id: String,
    pub label: String,
    pub lifecycle: HarnessLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HarnessControlResponse {
    pub format: u8,
    pub request_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub harnesses: Vec<HarnessStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl HarnessControlResponse {
    pub fn success(request_id: String, harnesses: Vec<HarnessStatus>) -> Self {
        Self {
            format: HARNESS_CONTROL_FORMAT,
            request_id,
            ok: true,
            harnesses,
            error: None,
        }
    }

    pub fn refusal(request_id: String, error: impl Into<String>) -> Self {
        Self {
            format: HARNESS_CONTROL_FORMAT,
            request_id,
            ok: false,
            harnesses: Vec::new(),
            error: Some(error.into()),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.format != HARNESS_CONTROL_FORMAT {
            return Err("unsupported harness control format".into());
        }
        bounded(&self.request_id, "requestId", MAX_IDENTIFIER)?;
        if self.ok == self.error.is_some() {
            return Err("a harness response must carry exactly one outcome".into());
        }
        for harness in &self.harnesses {
            bounded(&harness.harness_id, "harnessId", MAX_IDENTIFIER)?;
            bounded(&harness.label, "label", MAX_IDENTIFIER)?;
            if let Some(detail) = &harness.detail {
                bounded(detail, "detail", MAX_DETAIL)?;
            }
        }
        Ok(())
    }
}

fn bounded(value: &str, field: &str, ceiling: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > ceiling {
        return Err(format!("{field} must contain 1 to {ceiling} characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_requests_refuse_unknown_fields_and_foreign_formats() {
        let unknown =
            r#"{"format":1,"requestId":"r1","token":"t","command":{"method":"list","extra":true}}"#;
        assert!(serde_json::from_str::<HarnessControlRequest>(unknown).is_err());
        let foreign = HarnessControlRequest {
            format: 2,
            request_id: "r2".into(),
            token: "t".into(),
            command: HarnessCommand::List {},
        };
        assert_eq!(
            foreign.validate().unwrap_err(),
            "unsupported harness control format"
        );
    }

    #[test]
    fn control_responses_require_exactly_one_outcome() {
        let missing = HarnessControlResponse {
            format: HARNESS_CONTROL_FORMAT,
            request_id: "r1".into(),
            ok: false,
            harnesses: Vec::new(),
            error: None,
        };
        assert!(missing.validate().is_err());
        let doubled = HarnessControlResponse {
            ok: true,
            error: Some("impossible".into()),
            ..missing
        };
        assert!(doubled.validate().is_err());
    }
}
