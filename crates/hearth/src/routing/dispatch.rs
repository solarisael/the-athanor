use super::{
    CONTEXT_MODES, ContextMode, Familiar, RiskLevel, WorkerLane, WorkerLaneName,
    familiar_task_name, kitten_name, worker_lane,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextHint {
    pub mode: ContextMode,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl Default for ContextMode {
    fn default() -> Self {
        Self::Exact
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DispatchRequest {
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub familiar: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub context: Vec<ContextHint>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default)]
    pub lesson_bodies: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DispatchStatus {
    Ready,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatcherReceipt {
    pub executed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnTask {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub task: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnArgs {
    pub context: String,
    pub tasks: Vec<SpawnTask>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnPacket {
    pub tool: &'static str,
    pub args: SpawnArgs,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchReceipt {
    pub ok: bool,
    pub status: DispatchStatus,
    pub lane: Option<WorkerLaneName>,
    pub model_role: Option<String>,
    pub omp_agent: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub dispatcher: DispatcherReceipt,
    pub spawn_packet: Option<SpawnPacket>,
}

fn cleaned_lines(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn acceptance_lines(values: &[String]) -> Vec<String> {
    cleaned_lines(values)
        .iter()
        .flat_map(|entry| entry.lines())
        .map(|line| line.trim().trim_start_matches(['-', '*']).trim())
        .map(|line| line.strip_suffix("// 0%").unwrap_or(line).trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn format_context(hints: &[ContextHint]) -> String {
    if hints.is_empty() {
        return "No extra context supplied. Read exact sources before acting.".into();
    }

    hints
        .iter()
        .enumerate()
        .map(|(index, hint)| {
            let mode = hint.mode.as_str();
            let mut lines = vec![format!("{}. mode={mode}", index + 1)];
            if let Some(source) = hint.source.as_deref().filter(|value| !value.is_empty()) {
                lines.push(format!("   source={source}"));
            }
            if let Some(reason) = hint.reason.as_deref().filter(|value| !value.is_empty()) {
                lines.push(format!("   reason={reason}"));
            }
            if let Some(content) = hint.content.as_deref().filter(|value| !value.is_empty()) {
                lines.push(format!("   content={content}"));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_lesson_bodies(bodies: &[String]) -> String {
    cleaned_lines(bodies)
        .iter()
        .enumerate()
        .map(|(index, body)| format!("[Lesson {}]\n{body}", index + 1))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn lesson_body_errors(bodies: &[String]) -> Vec<String> {
    if cleaned_lines(bodies).is_empty() {
        vec![
            "Dispatch requires at least one complete lesson body; bare IDs and summaries are not delivery."
                .into(),
        ]
    } else {
        Vec::new()
    }
}

fn rejected_receipt(
    lane: Option<WorkerLane>,
    errors: Vec<String>,
    warnings: Vec<String>,
) -> DispatchReceipt {
    DispatchReceipt {
        ok: false,
        status: DispatchStatus::Rejected,
        lane: lane.map(|value| value.name),
        model_role: lane.map(|value| value.model_role.to_owned()),
        omp_agent: lane.map(|value| value.omp_agent.to_owned()),
        errors,
        warnings,
        dispatcher: DispatcherReceipt {
            executed: false,
            reason: "The Athanor validates and packages dispatches; the main model explicitly spawns accepted packets.".into(),
        },
        spawn_packet: None,
    }
}

fn base_dispatch_errors(
    request: &DispatchRequest,
    lane: Option<WorkerLane>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if lane.is_none() {
        let name = request.lane.trim();
        errors.push(format!(
            "Unknown worker lane: {}",
            if name.is_empty() { "<empty>" } else { name }
        ));
    }
    if request.task.trim().is_empty() {
        errors.push("Dispatch task is required.".into());
    }
    errors.extend(lesson_body_errors(&request.lesson_bodies));
    errors
}

fn context_mode_errors(request: &DispatchRequest, lane: WorkerLane) -> Vec<String> {
    let mut errors = Vec::new();
    for hint in &request.context {
        if !CONTEXT_MODES.contains(&hint.mode) {
            errors.push("Unknown context mode.".into());
        } else if !lane.allowed_context_modes.contains(&hint.mode) {
            errors.push(format!(
                "{} does not allow context mode '{}'.",
                lane.name.as_str(),
                hint.mode.as_str()
            ));
        }
    }
    errors
}

fn lane_dispatch_errors(
    request: &DispatchRequest,
    lane: WorkerLane,
    acceptance: &[String],
) -> Vec<String> {
    let mut errors = Vec::new();
    let target = request.target.as_deref().unwrap_or("").trim();
    if lane.requires_acceptance && acceptance.is_empty() {
        errors.push(format!(
            "{} requires at least one acceptance item.",
            lane.name.as_str()
        ));
    }
    if lane.can_edit && target.is_empty() {
        errors.push(format!("{} requires an exact target.", lane.name.as_str()));
    }
    errors.extend(context_mode_errors(request, lane));
    errors
}

fn dispatch_warnings(request: &DispatchRequest, lane: WorkerLane) -> Vec<String> {
    let mut warnings = Vec::new();
    let target = request.target.as_deref().unwrap_or("").trim();
    let has_exact_context = request
        .context
        .iter()
        .any(|hint| matches!(hint.mode, ContextMode::Exact | ContextMode::RetrieveOnly));
    if lane.can_edit && !has_exact_context {
        warnings.push(format!(
            "{} can edit; provide exact or retrieve-only context before executing.",
            lane.name.as_str()
        ));
    }
    if target.is_empty() {
        warnings.push(format!(
            "{} has no explicit target; lineage uses the general domain.",
            lane.name.as_str()
        ));
    }
    warnings
}

fn validate_dispatch(
    request: &DispatchRequest,
    lane: Option<WorkerLane>,
    acceptance: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut errors = base_dispatch_errors(request, lane);
    let Some(lane) = lane else {
        return (errors, Vec::new());
    };
    errors.extend(lane_dispatch_errors(request, lane, acceptance));
    (errors, dispatch_warnings(request, lane))
}

struct DispatchRoute<'a> {
    display_name: &'a str,
    task_name: String,
    omp_agent: &'a str,
    model_role: &'a str,
    is_familiar: bool,
}

fn dispatch_route<'a>(lane: WorkerLane, familiar: Option<&'a Familiar>) -> DispatchRoute<'a> {
    if let Some(familiar) = familiar {
        return DispatchRoute {
            display_name: &familiar.name,
            task_name: familiar_task_name(&familiar.id),
            omp_agent: if familiar.omp_agent.is_empty() {
                lane.omp_agent
            } else {
                &familiar.omp_agent
            },
            model_role: if familiar.model_role.is_empty() {
                lane.model_role
            } else {
                &familiar.model_role
            },
            is_familiar: true,
        };
    }

    let name = kitten_name(lane.name);
    DispatchRoute {
        display_name: name,
        task_name: name.into(),
        omp_agent: lane.omp_agent,
        model_role: lane.model_role,
        is_familiar: false,
    }
}

fn assignment(
    _lane: WorkerLane,
    worker_name: &str,
    task: &str,
    target: &str,
    acceptance: &[String],
    warnings: &mut Vec<String>,
) -> String {
    let target_lines = target
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let quest_target = target_lines.first().copied().unwrap_or("general");
    let frame_target = quest_target.replace('[', "(").replace(']', ")");
    if frame_target != quest_target {
        warnings.push("Quest frame target brackets were rendered as parentheses.".into());
    }

    let objectives = if acceptance.is_empty() {
        "- Return a receipt naming what was checked and what remains unknown. // 0%".into()
    } else {
        acceptance
            .iter()
            .map(|line| format!("- {line} // 0%"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut lines = vec![
        "# Target".into(),
        quest_target.into(),
        format!("[Quest Received] [{worker_name}] [TARGET: {frame_target}]"),
    ];
    if target_lines.len() > 1 {
        lines.push(String::new());
        lines.extend(target_lines[1..].iter().map(|line| (*line).to_owned()));
    }
    lines.extend([
        String::new(),
        "# Change".into(),
        "The House opens one bounded door for you:".into(),
        task.into(),
        "Keep your paws inside the named boundary. Read exact sources first and follow the written path; never invent a missing step.".into(),
        "If the map and terrain disagree, halt at the seam and tell us what you found. Questions, limits, disagreement, and refusal are valid yields.".into(),
        String::new(),
        "# Acceptance".into(),
        "**OBJECTIVES**".into(),
        objectives,
        "[Touch nothing else.]".into(),
        "[What will you do?]".into(),
    ]);
    lines.join("\n")
}

fn shared_context(
    request: &DispatchRequest,
    lane: WorkerLane,
    route: &DispatchRoute<'_>,
) -> String {
    let peer_contract = if route.is_familiar {
        "Treat this familiar as a capable peer. Authority remains bounded by the written quest."
    } else {
        "Treat this worker as a capable peer. Authority remains bounded by the written quest."
    };
    [
        "# Goal".into(),
        format!(
            "Help {} complete one exact quest whose result matters to the House.",
            route.display_name
        ),
        "# Constraints".into(),
        format!("Lane: {}", lane.name.as_str()),
        format!("Configured agent: {}", route.omp_agent),
        format!(
            "Model role: {}; the agent definition selects the runtime model.",
            route.model_role
        ),
        format!("Risk: {}", request.risk.as_str()),
        peer_contract.into(),
        "Do not infer operator intent beyond the quest. A halt with evidence is a successful result.".into(),
        "# Contract".into(),
        format!("{}: {}", lane.name.as_str(), lane.description),
        String::new(),
        "Context fragments:".into(),
        format_context(&request.context),
        String::new(),
        "[Codex — supplied lessons ride free and do not expand quest scope]".into(),
        format_lesson_bodies(&request.lesson_bodies),
        String::new(),
        "Return evidence, uncertainties, and exact changed or checked artifacts. An honest empty result is valid.".into(),
    ]
    .join("\n")
}

pub(super) fn dispatch_with_familiar(
    request: DispatchRequest,
    familiar: Option<&Familiar>,
) -> DispatchReceipt {
    let lane = worker_lane(&request.lane);
    let acceptance = acceptance_lines(&request.acceptance);
    let (errors, mut warnings) = validate_dispatch(&request, lane, &acceptance);

    let Some(lane) = lane else {
        return rejected_receipt(None, errors, warnings);
    };
    let route = dispatch_route(lane, familiar);
    if let Some(familiar) = familiar {
        if familiar.omp_agent.is_empty() || familiar.model_role.is_empty() {
            warnings.push(format!(
                "Familiar '{}' has no exact OMP agent and model-role binding; falling back to lane route '{}' / '{}'.",
                familiar.name, lane.omp_agent, lane.model_role
            ));
        }
    }
    if !errors.is_empty() {
        let mut receipt = rejected_receipt(Some(lane), errors, warnings);
        receipt.model_role = Some(route.model_role.into());
        receipt.omp_agent = Some(route.omp_agent.into());
        return receipt;
    }

    let task = assignment(
        lane,
        route.display_name,
        request.task.trim(),
        request.target.as_deref().unwrap_or("").trim(),
        &acceptance,
        &mut warnings,
    );
    let agent =
        (route.is_familiar || route.omp_agent != "task").then(|| route.omp_agent.to_owned());

    DispatchReceipt {
        ok: true,
        status: DispatchStatus::Ready,
        lane: Some(lane.name),
        model_role: Some(route.model_role.into()),
        omp_agent: Some(route.omp_agent.into()),
        errors,
        warnings,
        dispatcher: DispatcherReceipt {
            executed: false,
            reason: "Pass spawnPacket.args directly to the OMP task tool. Spawning remains an explicit main-model action.".into(),
        },
        spawn_packet: Some(SpawnPacket {
            tool: "task",
            args: SpawnArgs {
                context: shared_context(&request, lane, &route),
                tasks: vec![SpawnTask {
                    name: route.task_name,
                    agent,
                    task,
                }],
            },
        }),
    }
}

pub fn dispatch(request: DispatchRequest) -> DispatchReceipt {
    dispatch_with_familiar(request, None)
}
