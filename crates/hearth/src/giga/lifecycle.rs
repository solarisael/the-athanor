use crate::error::DomainError;

use super::source::{GigaSourceRange, giga_nonempty, giga_strings};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaRisk {
    Low,
    Medium,
    High,
}
impl GigaRisk {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(DomainError::UnknownGigaValue {
                field: "risk".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaEventType {
    ConversationWindow,
    TaskStarted,
    TaskCompleted,
    SubagentDispatched,
    SubagentCompleted,
    TodoTransition,
    ToolOutcome,
    ManualReprocess,
}
impl GigaEventType {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "conversation_window" => Ok(Self::ConversationWindow),
            "task_started" => Ok(Self::TaskStarted),
            "task_completed" => Ok(Self::TaskCompleted),
            "subagent_dispatched" => Ok(Self::SubagentDispatched),
            "subagent_completed" => Ok(Self::SubagentCompleted),
            "todo_transition" => Ok(Self::TodoTransition),
            "tool_outcome" => Ok(Self::ToolOutcome),
            "manual_reprocess" => Ok(Self::ManualReprocess),
            other => Err(DomainError::UnknownGigaValue {
                field: "event_type".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConversationWindow => "conversation_window",
            Self::TaskStarted => "task_started",
            Self::TaskCompleted => "task_completed",
            Self::SubagentDispatched => "subagent_dispatched",
            Self::SubagentCompleted => "subagent_completed",
            Self::TodoTransition => "todo_transition",
            Self::ToolOutcome => "tool_outcome",
            Self::ManualReprocess => "manual_reprocess",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaLifecycle {
    event_type: GigaEventType,
    fields: Vec<(String, String)>,
    proof_contract: Vec<String>,
    source_range: Option<GigaSourceRange>,
    risk: Option<GigaRisk>,
}

impl GigaLifecycle {
    fn fields(values: &[(&str, String)]) -> Result<Vec<(String, String)>, DomainError> {
        values
            .iter()
            .map(|(name, value)| Ok(((*name).into(), giga_nonempty(name, value.clone())?)))
            .collect()
    }
    pub fn conversation_window() -> Self {
        Self {
            event_type: GigaEventType::ConversationWindow,
            fields: Vec::new(),
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        }
    }
    pub fn task_started(
        task_reference: String,
        worker_id: String,
        worker_role: String,
        phase: String,
        project_key: String,
        task_kind: String,
        risk: GigaRisk,
        target: String,
        change: String,
        proof_contract: Vec<String>,
    ) -> Result<Self, DomainError> {
        if proof_contract.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "proof_contract".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            event_type: GigaEventType::TaskStarted,
            fields: Self::fields(&[
                ("task_reference", task_reference),
                ("worker_id", worker_id),
                ("worker_role", worker_role),
                ("phase", phase),
                ("project_key", project_key),
                ("task_kind", task_kind),
                ("target", target),
                ("change", change),
            ])?,
            proof_contract: giga_strings("proof_contract", proof_contract)?,
            source_range: None,
            risk: Some(risk),
        })
    }
    pub fn task_completed(
        task_reference: String,
        outcome: String,
        verification_result: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::TaskCompleted,
            fields: Self::fields(&[
                ("task_reference", task_reference),
                ("outcome", outcome),
                ("verification_result", verification_result),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn subagent_dispatched(
        subagent_reference: String,
        parent_task: String,
        role: String,
        target: String,
        change: String,
        acceptance: Vec<String>,
    ) -> Result<Self, DomainError> {
        if acceptance.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "acceptance".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            event_type: GigaEventType::SubagentDispatched,
            fields: Self::fields(&[
                ("subagent_reference", subagent_reference),
                ("parent_task", parent_task),
                ("role", role),
                ("target", target),
                ("change", change),
            ])?,
            proof_contract: giga_strings("acceptance", acceptance)?,
            source_range: None,
            risk: None,
        })
    }
    pub fn subagent_completed(
        subagent_reference: String,
        parent_task: String,
        outcome: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::SubagentCompleted,
            fields: Self::fields(&[
                ("subagent_reference", subagent_reference),
                ("parent_task", parent_task),
                ("outcome", outcome),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn todo_transition(
        todo_reference: String,
        previous_state: String,
        new_state: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::TodoTransition,
            fields: Self::fields(&[
                ("todo_reference", todo_reference),
                ("previous_state", previous_state),
                ("new_state", new_state),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn tool_outcome(
        tool_name: String,
        status: String,
        sanitized_outcome: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::ToolOutcome,
            fields: Self::fields(&[
                ("tool_name", tool_name),
                ("status", status),
                ("sanitized_outcome", sanitized_outcome),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn manual_reprocess(
        source_range: GigaSourceRange,
        reason: String,
        operator_identity: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::ManualReprocess,
            fields: Self::fields(&[("reason", reason), ("operator_identity", operator_identity)])?,
            proof_contract: Vec::new(),
            source_range: Some(source_range),
            risk: None,
        })
    }
    pub const fn event_type(&self) -> GigaEventType {
        self.event_type
    }
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
    pub fn proof_contract(&self) -> &[String] {
        &self.proof_contract
    }
    pub fn source_range(&self) -> Option<&GigaSourceRange> {
        self.source_range.as_ref()
    }
    pub const fn risk(&self) -> Option<GigaRisk> {
        self.risk
    }
}
