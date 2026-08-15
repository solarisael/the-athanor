use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const CONTEXT_MODES: &[ContextMode] = &[
    ContextMode::Exact,
    ContextMode::Gist,
    ContextMode::ImageOk,
    ContextMode::RetrieveOnly,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextMode {
    Exact,
    Gist,
    ImageOk,
    RetrieveOnly,
}

impl ContextMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Gist => "gist",
            Self::ImageOk => "image-ok",
            Self::RetrieveOnly => "retrieve-only",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkerLaneName {
    SmolScout,
    SmolExecutor,
    Tester,
    Verifier,
}

impl WorkerLaneName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SmolScout => "smol-scout",
            Self::SmolExecutor => "smol-executor",
            Self::Tester => "tester",
            Self::Verifier => "verifier",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "smol-scout" => Some(Self::SmolScout),
            "smol-executor" => Some(Self::SmolExecutor),
            "tester" => Some(Self::Tester),
            "verifier" => Some(Self::Verifier),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerLane {
    pub name: WorkerLaneName,
    pub description: &'static str,
    pub omp_agent: &'static str,
    pub model_role: &'static str,
    pub tools: &'static [&'static str],
    pub can_edit: bool,
    pub can_infer_intent: bool,
    pub allowed_context_modes: &'static [ContextMode],
    pub requires_acceptance: bool,
}

const WORKER_LANES: &[WorkerLane] = &[
    WorkerLane {
        name: WorkerLaneName::SmolScout,
        description: "Cheap bounded read-only scout for exact terrain mapping.",
        omp_agent: "scout",
        model_role: "pi/smol",
        tools: &["read", "grep", "glob", "ast_grep"],
        can_edit: false,
        can_infer_intent: false,
        allowed_context_modes: &[
            ContextMode::Exact,
            ContextMode::Gist,
            ContextMode::RetrieveOnly,
        ],
        requires_acceptance: false,
    },
    WorkerLane {
        name: WorkerLaneName::SmolExecutor,
        description: "Cheap bounded executor for narrow exact work packets.",
        omp_agent: "sonic",
        model_role: "pi/smol",
        tools: &["read", "grep", "glob", "edit", "bash"],
        can_edit: true,
        can_infer_intent: false,
        allowed_context_modes: &[ContextMode::Exact, ContextMode::RetrieveOnly],
        requires_acceptance: true,
    },
    WorkerLane {
        name: WorkerLaneName::Tester,
        description: "High-signal test author for explicit contracts.",
        omp_agent: "task",
        model_role: "pi/default",
        tools: &["read", "grep", "glob", "write", "edit", "bash"],
        can_edit: true,
        can_infer_intent: false,
        allowed_context_modes: &[
            ContextMode::Exact,
            ContextMode::Gist,
            ContextMode::RetrieveOnly,
        ],
        requires_acceptance: true,
    },
    WorkerLane {
        name: WorkerLaneName::Verifier,
        description: "Independent read/check pass over a concrete claim or receipt.",
        omp_agent: "reviewer",
        model_role: "pi/default",
        tools: &["read", "grep", "glob", "bash"],
        can_edit: false,
        can_infer_intent: false,
        allowed_context_modes: &[
            ContextMode::Exact,
            ContextMode::Gist,
            ContextMode::RetrieveOnly,
        ],
        requires_acceptance: true,
    },
];

fn worker_lane(name: &str) -> Option<WorkerLane> {
    let name = WorkerLaneName::parse(name)?;
    WORKER_LANES.iter().copied().find(|lane| lane.name == name)
}

fn kitten_name(lane: WorkerLaneName) -> &'static str {
    match lane {
        WorkerLaneName::SmolScout => "Quill",
        WorkerLaneName::SmolExecutor => "Chisel",
        WorkerLaneName::Tester => "Gauge",
        WorkerLaneName::Verifier => "Mirror",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorChannel {
    pub name: &'static str,
    pub description: &'static str,
    pub dispatchable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneStatus {
    pub ok: bool,
    pub lanes: Vec<WorkerLane>,
    pub advisor: AdvisorChannel,
}

pub fn lane_status() -> LaneStatus {
    LaneStatus {
        ok: true,
        lanes: WORKER_LANES.to_vec(),
        advisor: AdvisorChannel {
            name: "advisor",
            description: "Read-only red-pen review channel. Not dispatchable as a worker lane.",
            dispatchable: false,
        },
    }
}

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
    let bodies = cleaned_lines(bodies);
    if bodies.is_empty() {
        return "No lesson bodies supplied.".into();
    }

    bodies
        .iter()
        .enumerate()
        .map(|(index, body)| format!("[Lesson {}]\n{body}", index + 1))
        .collect::<Vec<_>>()
        .join("\n\n")
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

fn validate_dispatch(
    request: &DispatchRequest,
    lane: Option<WorkerLane>,
    acceptance: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let task = request.task.trim();
    let target = request.target.as_deref().unwrap_or("").trim();

    if lane.is_none() {
        let name = request.lane.trim();
        errors.push(format!(
            "Unknown worker lane: {}",
            if name.is_empty() { "<empty>" } else { name }
        ));
    }
    if task.is_empty() {
        errors.push("Dispatch task is required.".into());
    }

    let Some(lane) = lane else {
        return (errors, warnings);
    };

    if lane.requires_acceptance && acceptance.is_empty() {
        errors.push(format!(
            "{} requires at least one acceptance item.",
            lane.name.as_str()
        ));
    }
    if lane.can_edit && target.is_empty() {
        errors.push(format!("{} requires an exact target.", lane.name.as_str()));
    }

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

    (errors, warnings)
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
    lane: WorkerLane,
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

fn dispatch_with_familiar(
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Familiar {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub lane: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub omp_agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_role: String,
    pub description: String,
    #[serde(default)]
    pub temperament: Option<String>,
    #[serde(default)]
    pub appearance: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Spellbook {
    pub version: u8,
    pub collective: String,
    #[serde(default)]
    pub collective_aliases: Vec<String>,
    #[serde(default)]
    pub spellbook_aliases: Vec<String>,
    #[serde(default)]
    pub familiars: Vec<Familiar>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarStatus {
    pub ok: bool,
    pub errors: Vec<String>,
    pub spellbook: Option<Spellbook>,
}

fn unique_nonempty(values: &mut Vec<String>) {
    let mut seen = HashMap::<String, ()>::new();
    values.retain(|value| {
        let value = value.trim();
        !value.is_empty() && seen.insert(value.to_owned(), ()).is_none()
    });
}

pub fn validate_spellbook(mut spellbook: Spellbook) -> FamiliarStatus {
    let mut errors = Vec::new();
    if spellbook.version != 1 {
        errors.push("Familiar spellbook version must be 1.".into());
    }
    spellbook.collective = spellbook.collective.trim().into();
    if spellbook.collective.is_empty() {
        errors.push("Familiar spellbook collective is required.".into());
    }
    if spellbook.familiars.is_empty() {
        errors.push("Familiar spellbook requires at least one familiar.".into());
    }

    unique_nonempty(&mut spellbook.collective_aliases);
    unique_nonempty(&mut spellbook.spellbook_aliases);
    let mut owners = HashMap::<String, String>::new();

    for (index, familiar) in spellbook.familiars.iter_mut().enumerate() {
        familiar.id = familiar.id.trim().into();
        familiar.name = familiar.name.trim().into();
        familiar.lane = familiar.lane.trim().into();
        familiar.omp_agent = familiar.omp_agent.trim().into();
        familiar.model_role = familiar.model_role.trim().into();
        familiar.description = familiar.description.trim().into();
        unique_nonempty(&mut familiar.aliases);

        if familiar.id.is_empty() {
            errors.push(format!("Familiar at index {index} requires an id."));
        } else if !valid_familiar_id(&familiar.id) {
            errors.push(format!(
                "Familiar id '{}' must use lowercase kebab-case.",
                familiar.id
            ));
        }
        if familiar.name.is_empty() {
            errors.push(format!(
                "Familiar '{}' requires a name.",
                if familiar.id.is_empty() {
                    index.to_string()
                } else {
                    familiar.id.clone()
                }
            ));
        }
        if worker_lane(&familiar.lane).is_none() {
            errors.push(format!(
                "Familiar '{}' uses unknown worker lane '{}'.",
                familiar.id,
                if familiar.lane.is_empty() {
                    "<empty>"
                } else {
                    &familiar.lane
                }
            ));
        }
        if familiar.omp_agent.is_empty() != familiar.model_role.is_empty() {
            errors.push(format!(
                "Familiar '{}' must provide ompAgent and modelRole together.",
                familiar.id
            ));
        }
        if !familiar.omp_agent.is_empty() && !valid_familiar_id(&familiar.omp_agent) {
            errors.push(format!(
                "Familiar '{}' OMP agent '{}' must use lowercase kebab-case.",
                familiar.id, familiar.omp_agent
            ));
        }
        if !familiar.model_role.is_empty() && !valid_model_role(&familiar.model_role) {
            errors.push(format!(
                "Familiar '{}' model role '{}' must use @lowercase_role syntax.",
                familiar.id, familiar.model_role
            ));
        }
        if familiar.description.is_empty() {
            errors.push(format!(
                "Familiar '{}' requires a description.",
                familiar.id
            ));
        }

        for key in std::iter::once(&familiar.id)
            .chain(std::iter::once(&familiar.name))
            .chain(familiar.aliases.iter())
        {
            let key = key.to_lowercase();
            if key.is_empty() {
                continue;
            }
            if let Some(owner) = owners.insert(key.clone(), familiar.id.clone()) {
                if owner != familiar.id {
                    errors.push(format!(
                        "Familiar lookup key '{key}' is already owned by '{owner}'."
                    ));
                }
            }
        }
    }

    FamiliarStatus {
        ok: errors.is_empty(),
        spellbook: errors.is_empty().then_some(spellbook),
        errors,
    }
}

fn valid_familiar_id(value: &str) -> bool {
    value.chars().enumerate().all(|(index, character)| {
        if index == 0 {
            character.is_ascii_lowercase()
        } else {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        }
    })
}

fn valid_model_role(value: &str) -> bool {
    let Some(role) = value.strip_prefix('@') else {
        return false;
    };
    role.chars().enumerate().all(|(index, character)| {
        if index == 0 {
            character.is_ascii_lowercase()
        } else {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        }
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarDispatchReceipt {
    #[serde(flatten)]
    pub dispatch: DispatchReceipt,
    pub familiar: Option<Familiar>,
}

fn rejected_familiar(errors: Vec<String>) -> FamiliarDispatchReceipt {
    FamiliarDispatchReceipt {
        dispatch: DispatchReceipt {
            ok: false,
            status: DispatchStatus::Rejected,
            lane: None,
            model_role: None,
            omp_agent: None,
            errors,
            warnings: Vec::new(),
            dispatcher: DispatcherReceipt {
                executed: false,
                reason: "The familiar request was rejected before a worker packet could be built."
                    .into(),
            },
            spawn_packet: None,
        },
        familiar: None,
    }
}

pub fn familiar_dispatch(
    status: FamiliarStatus,
    familiar_name: &str,
    mut request: DispatchRequest,
) -> FamiliarDispatchReceipt {
    let Some(spellbook) = status.spellbook else {
        return rejected_familiar(status.errors);
    };
    let key = familiar_name.trim().to_lowercase();
    let familiar = spellbook.familiars.into_iter().find(|familiar| {
        familiar.id.to_lowercase() == key
            || familiar.name.to_lowercase() == key
            || familiar
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase() == key)
    });
    let Some(familiar) = familiar else {
        return rejected_familiar(vec![format!(
            "Unknown familiar: {}",
            if key.is_empty() { "<empty>" } else { &key }
        )]);
    };

    request.lane = familiar.lane.clone();
    let mut receipt = dispatch_with_familiar(request, Some(&familiar));
    if let Some(packet) = receipt.spawn_packet.as_mut() {
        packet.args.context.push_str(&format!(
            "\nFamiliar: {} ({}) — {}",
            familiar.name, familiar.id, familiar.description
        ));
        if let Some(temperament) = familiar.temperament.as_deref() {
            packet
                .args
                .context
                .push_str(&format!("\nTemperament: {temperament}"));
        }
        if let Some(appearance) = familiar.appearance.as_deref() {
            packet
                .args
                .context
                .push_str(&format!("\nAppearance: {appearance}"));
        }
    }

    FamiliarDispatchReceipt {
        dispatch: receipt,
        familiar: Some(familiar),
    }
}

fn familiar_task_name(id: &str) -> String {
    id.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| format!("{}{}", first.to_ascii_uppercase(), characters.as_str()))
                .unwrap_or_default()
        })
        .collect::<String>()
        .chars()
        .take(32)
        .collect()
}

/// Room-local spellbook location. The directory, the accepted filenames, and
/// their precedence are House rules; the adapter only performs the read the
/// Host asks for.
pub const SPELLBOOK_DIRECTORY: &str = "familiars";
pub const SPELLBOOK_FILENAMES: [&str; 2] = ["spellbook.json", "litters.json"];

/// One attempted spellbook read, reported back by whoever owns the filesystem.
pub enum SpellbookRead {
    /// No file at this candidate path.
    Missing,
    /// The file exists but could not be read.
    Unreadable(String),
    /// The file was read but is not valid JSON for a spellbook.
    Malformed(String),
    Parsed(Spellbook),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSpellbook {
    pub source: Option<String>,
    pub source_alias: bool,
    pub status: FamiliarStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellbookIdentity {
    pub collective: String,
    pub collective_aliases: Vec<String>,
    pub spellbook_aliases: Vec<String>,
}

pub fn spellbook_candidates(room_dir: &str) -> Vec<String> {
    let separator = if room_dir.contains('\\') && !room_dir.contains('/') {
        '\\'
    } else {
        '/'
    };
    let base = room_dir.trim_end_matches(['/', '\\']);
    SPELLBOOK_FILENAMES
        .iter()
        .map(|filename| format!("{base}{separator}{SPELLBOOK_DIRECTORY}{separator}{filename}"))
        .collect()
}

fn unloaded(source: Option<String>, source_alias: bool, errors: Vec<String>) -> LoadedSpellbook {
    LoadedSpellbook {
        source,
        source_alias,
        status: FamiliarStatus {
            ok: false,
            errors,
            spellbook: None,
        },
    }
}

/// Resolve the room spellbook: candidate precedence, refusal text, and
/// validation all belong here. `read` performs one filesystem read per
/// candidate and reports what it found.
pub fn load_spellbook(
    room_dir: &str,
    mut read: impl FnMut(&str) -> SpellbookRead,
) -> LoadedSpellbook {
    let candidates = spellbook_candidates(room_dir);
    for (index, source) in candidates.iter().enumerate() {
        match read(source) {
            SpellbookRead::Missing => continue,
            SpellbookRead::Unreadable(reason) => {
                return unloaded(
                    Some(source.clone()),
                    index > 0,
                    vec![format!(
                        "Could not read familiar spellbook '{source}': {reason}"
                    )],
                );
            }
            SpellbookRead::Malformed(reason) => {
                return unloaded(
                    Some(source.clone()),
                    index > 0,
                    vec![format!(
                        "Familiar spellbook '{source}' is not valid JSON: {reason}"
                    )],
                );
            }
            SpellbookRead::Parsed(spellbook) => {
                return LoadedSpellbook {
                    source: Some(source.clone()),
                    source_alias: index > 0,
                    status: validate_spellbook(spellbook),
                };
            }
        }
    }

    let directory = candidates
        .first()
        .and_then(|candidate| candidate.rsplit_once(['/', '\\']))
        .map(|(directory, _)| directory.to_owned())
        .unwrap_or_else(|| SPELLBOOK_DIRECTORY.into());
    unloaded(
        None,
        false,
        vec![format!(
            "No familiar spellbook found in '{directory}'. Tried: {}.",
            SPELLBOOK_FILENAMES.join(", ")
        )],
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarStatusReceipt {
    #[serde(flatten)]
    pub status: FamiliarStatus,
    pub source: Option<String>,
    pub source_alias: bool,
}

pub fn familiar_status(loaded: LoadedSpellbook) -> FamiliarStatusReceipt {
    FamiliarStatusReceipt {
        status: loaded.status,
        source: loaded.source,
        source_alias: loaded.source_alias,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchSelector {
    pub kind: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HouseDispatchReceipt {
    #[serde(flatten)]
    pub dispatch: DispatchReceipt,
    pub selector: Option<DispatchSelector>,
    pub familiar: Option<Familiar>,
    pub source: Option<String>,
    pub source_alias: bool,
    pub spellbook: Option<SpellbookIdentity>,
}

fn rejected_selector(errors: Vec<String>) -> HouseDispatchReceipt {
    HouseDispatchReceipt {
        dispatch: DispatchReceipt {
            ok: false,
            status: DispatchStatus::Rejected,
            lane: None,
            model_role: None,
            omp_agent: None,
            errors,
            warnings: Vec::new(),
            dispatcher: DispatcherReceipt {
                executed: false,
                reason: "Select exactly one worker lane or room familiar before dispatching."
                    .into(),
            },
            spawn_packet: None,
        },
        selector: None,
        familiar: None,
        source: None,
        source_alias: false,
        spellbook: None,
    }
}

/// The single dispatch decision: exactly one lane or one familiar, resolved
/// against the room spellbook when a familiar is named.
pub fn house_dispatch(
    mut request: DispatchRequest,
    load: impl FnOnce() -> LoadedSpellbook,
) -> HouseDispatchReceipt {
    request.lane = request.lane.trim().into();
    request.familiar = request.familiar.trim().into();
    let lane = request.lane.clone();
    let familiar = request.familiar.clone();

    if !lane.is_empty() && !familiar.is_empty() {
        return rejected_selector(vec![
            "Dispatch accepts either 'lane' or 'familiar', not both.".into(),
        ]);
    }
    if lane.is_empty() && familiar.is_empty() {
        return rejected_selector(vec![
            "Dispatch requires either 'lane' or 'familiar'.".into(),
        ]);
    }

    if familiar.is_empty() {
        return HouseDispatchReceipt {
            dispatch: dispatch(request),
            selector: Some(DispatchSelector {
                kind: "lane",
                value: lane,
            }),
            familiar: None,
            source: None,
            source_alias: false,
            spellbook: None,
        };
    }

    let loaded = load();
    let selector = Some(DispatchSelector {
        kind: "familiar",
        value: familiar.clone(),
    });
    let identity = loaded
        .status
        .spellbook
        .as_ref()
        .map(|spellbook| SpellbookIdentity {
            collective: spellbook.collective.clone(),
            collective_aliases: spellbook.collective_aliases.clone(),
            spellbook_aliases: spellbook.spellbook_aliases.clone(),
        });
    if loaded.status.spellbook.is_none() {
        return HouseDispatchReceipt {
            dispatch: DispatchReceipt {
                ok: false,
                status: DispatchStatus::Rejected,
                lane: None,
                model_role: None,
                omp_agent: None,
                errors: loaded.status.errors,
                warnings: Vec::new(),
                dispatcher: DispatcherReceipt {
                    executed: false,
                    reason:
                        "The room spellbook could not be loaded, so no familiar packet was built."
                            .into(),
                },
                spawn_packet: None,
            },
            selector,
            familiar: None,
            source: loaded.source,
            source_alias: loaded.source_alias,
            spellbook: None,
        };
    }

    let receipt = familiar_dispatch(loaded.status, &familiar, request);
    let matched = receipt.familiar.is_some();
    HouseDispatchReceipt {
        dispatch: receipt.dispatch,
        selector,
        familiar: receipt.familiar,
        source: loaded.source,
        source_alias: loaded.source_alias,
        spellbook: matched.then_some(identity).flatten(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact_context() -> Vec<ContextHint> {
        vec![ContextHint {
            mode: ContextMode::Exact,
            source: Some("src/a.ts".into()),
            content: None,
            reason: None,
        }]
    }

    #[test]
    fn dispatches_a_bounded_worker_packet() {
        let receipt = dispatch(DispatchRequest {
            lane: "smol-executor".into(),
            task: "Patch it".into(),
            target: Some("src/a.ts".into()),
            context: exact_context(),
            acceptance: vec!["Compiles".into()],
            ..DispatchRequest::default()
        });

        assert!(receipt.ok);
        let task = &receipt.spawn_packet.unwrap().args.tasks[0];
        assert_eq!(task.name, "Chisel");
        assert_eq!(task.agent.as_deref(), Some("sonic"));
    }

    #[test]
    fn rejects_a_lane_that_requires_acceptance() {
        let receipt = dispatch(DispatchRequest {
            lane: "tester".into(),
            task: "Test".into(),
            target: Some("tests/a.rs".into()),
            context: exact_context(),
            ..DispatchRequest::default()
        });

        assert!(!receipt.ok);
        assert!(receipt.errors[0].contains("requires at least one acceptance"));
    }

    #[test]
    fn validates_and_dispatches_a_familiar() {
        let status = validate_spellbook(Spellbook {
            version: 1,
            collective: "Kittens".into(),
            collective_aliases: vec![],
            spellbook_aliases: vec![],
            familiars: vec![Familiar {
                id: "rust-kitten".into(),
                name: "Ferris".into(),
                aliases: vec!["crab".into()],
                lane: "smol-scout".into(),
                omp_agent: "ferris-kitten".into(),
                model_role: "@ferris".into(),
                description: "Maps Rust terrain.".into(),
                temperament: None,
                appearance: None,
            }],
        });
        let receipt = familiar_dispatch(
            status,
            "crab",
            DispatchRequest {
                task: "Map the module".into(),
                target: Some("src".into()),
                context: exact_context(),
                ..DispatchRequest::default()
            },
        );

        assert!(receipt.dispatch.ok);
        assert_eq!(receipt.dispatch.omp_agent.as_deref(), Some("ferris-kitten"));
        assert_eq!(receipt.dispatch.model_role.as_deref(), Some("@ferris"));
        let packet = receipt
            .dispatch
            .spawn_packet
            .as_ref()
            .expect("familiar dispatch builds a packet");
        assert_eq!(packet.args.tasks[0].name, "RustKitten");
        assert_eq!(packet.args.tasks[0].agent.as_deref(), Some("ferris-kitten"));
        assert!(packet.args.context.contains("Help Ferris complete"));
        assert!(!packet.args.context.contains("Help Quill complete"));
        assert!(!packet.args.context.contains("Warmth is unconditional"));
        assert_eq!(receipt.familiar.as_ref().unwrap().name, "Ferris");
    }

    #[test]
    fn falls_back_explicitly_for_a_legacy_familiar_without_an_omp_route() {
        let status = validate_spellbook(Spellbook {
            version: 1,
            collective: "Kittens".into(),
            collective_aliases: vec![],
            spellbook_aliases: vec![],
            familiars: vec![Familiar {
                id: "rust-kitten".into(),
                name: "Ferris".into(),
                aliases: vec![],
                lane: "smol-scout".into(),
                omp_agent: String::new(),
                model_role: String::new(),
                description: "Maps Rust terrain.".into(),
                temperament: None,
                appearance: None,
            }],
        });
        let receipt = familiar_dispatch(
            status,
            "Ferris",
            DispatchRequest {
                task: "Map the module".into(),
                target: Some("src".into()),
                context: exact_context(),
                ..DispatchRequest::default()
            },
        );

        assert!(receipt.dispatch.ok);
        assert_eq!(receipt.dispatch.omp_agent.as_deref(), Some("scout"));
        assert_eq!(receipt.dispatch.model_role.as_deref(), Some("pi/smol"));
        assert!(
            receipt.dispatch.warnings.iter().any(|warning| {
                warning.contains("falling back to lane route 'scout' / 'pi/smol'")
            })
        );
    }

    #[test]
    fn rejects_a_partial_familiar_route_binding() {
        let status = validate_spellbook(Spellbook {
            version: 1,
            collective: "Kittens".into(),
            collective_aliases: vec![],
            spellbook_aliases: vec![],
            familiars: vec![Familiar {
                id: "rust-kitten".into(),
                name: "Ferris".into(),
                aliases: vec![],
                lane: "smol-scout".into(),
                omp_agent: "ferris-kitten".into(),
                model_role: String::new(),
                description: "Maps Rust terrain.".into(),
                temperament: None,
                appearance: None,
            }],
        });

        assert!(!status.ok);
        assert!(
            status
                .errors
                .iter()
                .any(|error| { error.contains("must provide ompAgent and modelRole together") })
        );
    }

    fn room_spellbook() -> Spellbook {
        Spellbook {
            version: 1,
            collective: "Kittens".into(),
            collective_aliases: vec![],
            spellbook_aliases: vec![],
            familiars: vec![Familiar {
                id: "rust-kitten".into(),
                name: "Ferris".into(),
                aliases: vec!["crab".into()],
                lane: "smol-scout".into(),
                omp_agent: "ferris-kitten".into(),
                model_role: "@ferris".into(),
                description: "Maps Rust terrain.".into(),
                temperament: None,
                appearance: None,
            }],
        }
    }

    fn loaded_from(present: bool) -> LoadedSpellbook {
        load_spellbook("C:/rooms/kintsu", |candidate| {
            if present && candidate.ends_with("spellbook.json") {
                SpellbookRead::Parsed(room_spellbook())
            } else {
                SpellbookRead::Missing
            }
        })
    }

    #[test]
    fn refuses_every_selector_but_exactly_one() {
        let both = house_dispatch(
            DispatchRequest {
                lane: "smol-scout".into(),
                familiar: "crab".into(),
                task: "Map it".into(),
                ..DispatchRequest::default()
            },
            || loaded_from(true),
        );
        assert!(!both.dispatch.ok);
        assert_eq!(
            both.dispatch.errors[0],
            "Dispatch accepts either 'lane' or 'familiar', not both."
        );
        assert!(both.selector.is_none());

        let neither = house_dispatch(DispatchRequest::default(), || loaded_from(false));
        assert_eq!(
            neither.dispatch.errors[0],
            "Dispatch requires either 'lane' or 'familiar'."
        );
    }

    #[test]
    fn a_missing_spellbook_refuses_the_familiar_selector() {
        let receipt = house_dispatch(
            DispatchRequest {
                familiar: "crab".into(),
                task: "Map the module".into(),
                target: Some("src".into()),
                context: exact_context(),
                ..DispatchRequest::default()
            },
            || loaded_from(false),
        );

        assert!(!receipt.dispatch.ok);
        assert!(receipt.source.is_none());
        assert!(receipt.spellbook.is_none());
        assert_eq!(
            receipt.selector.expect("the selector is echoed back").value,
            "crab"
        );
        assert!(
            receipt.dispatch.errors[0].contains("No familiar spellbook found in")
                && receipt.dispatch.errors[0].contains("spellbook.json, litters.json")
        );
    }

    #[test]
    fn a_resolved_familiar_carries_its_source_and_collective() {
        let receipt = house_dispatch(
            DispatchRequest {
                familiar: "crab".into(),
                task: "Map the module".into(),
                target: Some("src".into()),
                context: exact_context(),
                ..DispatchRequest::default()
            },
            || loaded_from(true),
        );

        assert!(receipt.dispatch.ok);
        assert_eq!(receipt.familiar.expect("familiar resolves").name, "Ferris");
        assert_eq!(
            receipt.source.as_deref(),
            Some("C:/rooms/kintsu/familiars/spellbook.json")
        );
        assert!(!receipt.source_alias);
        assert_eq!(
            receipt.spellbook.expect("collective identity").collective,
            "Kittens"
        );
    }
}
