use serde::{Deserialize, Serialize};

mod dispatch;
mod spellbook;

use dispatch::dispatch_with_familiar;
pub use dispatch::{DispatchReceipt, DispatchRequest, DispatchStatus, DispatcherReceipt, dispatch};
pub use spellbook::{Familiar, FamiliarStatus, Spellbook, validate_spellbook};

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
