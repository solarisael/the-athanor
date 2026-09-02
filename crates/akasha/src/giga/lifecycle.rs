use super::error::domain_error;
use crate::AppError;
use hearth::{GigaEvent, GigaEventType, GigaLifecycle, GigaRisk, GigaSourceRange};
use serde_json::{Value, json};

pub(super) fn lifecycle_json(event: &GigaEvent) -> Value {
    let lifecycle = event.lifecycle();
    let mut value = serde_json::Map::new();
    for field in [
        "task_reference",
        "worker_id",
        "worker_role",
        "phase",
        "project_key",
        "task_kind",
        "target",
        "change",
        "outcome",
        "verification_result",
        "subagent_reference",
        "parent_task",
        "role",
        "todo_reference",
        "previous_state",
        "new_state",
        "tool_name",
        "status",
        "sanitized_outcome",
        "reason",
        "operator_identity",
    ] {
        if let Some(field_value) = lifecycle.field(field) {
            value.insert(field.into(), json!(field_value));
        }
    }
    if !lifecycle.proof_contract().is_empty() {
        let field = if event.event_type() == GigaEventType::SubagentDispatched {
            "acceptance"
        } else {
            "proof_contract"
        };
        value.insert(field.into(), json!(lifecycle.proof_contract()));
    }
    if let Some(range) = lifecycle.source_range() {
        value.insert(
            "source_range".into(),
            json!({"start": range.start(), "end": range.end()}),
        );
    }
    if let Some(risk) = lifecycle.risk() {
        value.insert("risk".into(), json!(risk.as_str()));
    }
    Value::Object(value)
}

fn lifecycle_text(value: &Value, field: &str) -> Result<String, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::Invalid(format!("stored GIGA lifecycle is missing {field}")))
}

fn lifecycle_strings(value: &Value, field: &str) -> Result<Vec<String>, AppError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Invalid(format!("stored GIGA lifecycle is missing {field}")))?
        .iter()
        .map(|entry| {
            entry.as_str().map(str::to_owned).ok_or_else(|| {
                AppError::Invalid(format!("stored GIGA lifecycle {field} is invalid"))
            })
        })
        .collect()
}

pub(super) fn lifecycle_from_json(
    event_type: GigaEventType,
    value: &Value,
) -> Result<GigaLifecycle, AppError> {
    match event_type {
        GigaEventType::ConversationWindow => Ok(GigaLifecycle::conversation_window()),
        GigaEventType::TaskStarted => GigaLifecycle::task_started(
            lifecycle_text(value, "task_reference")?,
            lifecycle_text(value, "worker_id")?,
            lifecycle_text(value, "worker_role")?,
            lifecycle_text(value, "phase")?,
            lifecycle_text(value, "project_key")?,
            lifecycle_text(value, "task_kind")?,
            GigaRisk::parse(&lifecycle_text(value, "risk")?).map_err(domain_error)?,
            lifecycle_text(value, "target")?,
            lifecycle_text(value, "change")?,
            lifecycle_strings(value, "proof_contract")?,
        )
        .map_err(domain_error),
        GigaEventType::TaskCompleted => GigaLifecycle::task_completed(
            lifecycle_text(value, "task_reference")?,
            lifecycle_text(value, "outcome")?,
            lifecycle_text(value, "verification_result")?,
        )
        .map_err(domain_error),
        GigaEventType::SubagentDispatched => {
            let acceptance = if value.get("acceptance").is_some() {
                lifecycle_strings(value, "acceptance")?
            } else {
                lifecycle_strings(value, "proof_contract")?
            };
            GigaLifecycle::subagent_dispatched(
                lifecycle_text(value, "subagent_reference")?,
                lifecycle_text(value, "parent_task")?,
                lifecycle_text(value, "role")?,
                lifecycle_text(value, "target")?,
                lifecycle_text(value, "change")?,
                acceptance,
            )
            .map_err(domain_error)
        }
        GigaEventType::SubagentCompleted => GigaLifecycle::subagent_completed(
            lifecycle_text(value, "subagent_reference")?,
            lifecycle_text(value, "parent_task")?,
            lifecycle_text(value, "outcome")?,
        )
        .map_err(domain_error),
        GigaEventType::TodoTransition => GigaLifecycle::todo_transition(
            lifecycle_text(value, "todo_reference")?,
            lifecycle_text(value, "previous_state")?,
            lifecycle_text(value, "new_state")?,
        )
        .map_err(domain_error),
        GigaEventType::ToolOutcome => GigaLifecycle::tool_outcome(
            lifecycle_text(value, "tool_name")?,
            lifecycle_text(value, "status")?,
            lifecycle_text(value, "sanitized_outcome")?,
        )
        .map_err(domain_error),
        GigaEventType::ManualReprocess => {
            let range = value
                .get("source_range")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    AppError::Invalid("stored GIGA lifecycle is missing source_range".into())
                })?;
            let start = range.get("start").and_then(Value::as_u64).ok_or_else(|| {
                AppError::Invalid("stored GIGA lifecycle source_range is invalid".into())
            })?;
            let end = range.get("end").and_then(Value::as_u64).ok_or_else(|| {
                AppError::Invalid("stored GIGA lifecycle source_range is invalid".into())
            })?;
            GigaLifecycle::manual_reprocess(
                GigaSourceRange::new(start, end).map_err(domain_error)?,
                lifecycle_text(value, "reason")?,
                lifecycle_text(value, "operator_identity")?,
            )
            .map_err(domain_error)
        }
    }
}
