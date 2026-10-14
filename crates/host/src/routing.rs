use serde::{Deserialize, Serialize};
use std::collections::HashSet;

mod dispatch;
mod spellbook;
#[cfg(test)]
mod tests;

use dispatch::dispatch_with_familiar;
pub use dispatch::{DispatchReceipt, DispatchRequest, DispatchStatus, DispatcherReceipt, dispatch};
pub use spellbook::{Familiar, FamiliarStatus, SpellbookPage, validate_spellbook};

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

/// Spellbook location. The directory, the accepted filenames, and their
/// precedence are House rules; the adapter only performs the read the Host
/// asks for.
pub const SPELLBOOK_DIRECTORY: &str = "familiars";
pub const SPELLBOOK_FILENAMES: [&str; 2] = ["spellbook.json", "litters.json"];

/// One attempted spellbook read, reported back by whoever owns the filesystem.
pub enum SpellbookRead {
    /// No file at this candidate path.
    Missing,
    /// The file exists but could not be read.
    Unreadable(String),
    /// The file was read but is not valid JSON for a spellbook page.
    Malformed(String),
    Parsed(SpellbookPage),
}

/// The file one layer of the effective spellbook came from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellbookSource {
    pub layer: &'static str,
    pub path: String,
    pub alias: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSpellbook {
    pub sources: Vec<SpellbookSource>,
    pub status: FamiliarStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellbookIdentity {
    pub collective: String,
    pub collective_aliases: Vec<String>,
    pub spellbook_aliases: Vec<String>,
}

pub fn spellbook_candidates(directory: &str) -> Vec<String> {
    let separator = if directory.contains('\\') && !directory.contains('/') {
        '\\'
    } else {
        '/'
    };
    let base = directory.trim_end_matches(['/', '\\']);
    SPELLBOOK_FILENAMES
        .iter()
        .map(|filename| format!("{base}{separator}{SPELLBOOK_DIRECTORY}{separator}{filename}"))
        .collect()
}

// Rooms share a parent; `house` is the reserved House directory.
// A parentless room has no shared page.
fn house_dir(room_dir: &str) -> Option<String> {
    let room = room_dir.trim_end_matches(['/', '\\']);
    let separator = room.rfind(['/', '\\'])?;
    Some(format!("{}house", &room[..=separator]))
}

/// The first spellbook file found in one directory, parsed or refused.
fn read_page(
    layer: &'static str,
    directory: &str,
    read: &mut impl FnMut(&str) -> SpellbookRead,
) -> Option<(SpellbookSource, Result<SpellbookPage, String>)> {
    spellbook_candidates(directory)
        .into_iter()
        .enumerate()
        .find_map(|(index, path)| {
            let page = match read(&path) {
                SpellbookRead::Missing => return None,
                SpellbookRead::Unreadable(reason) => Err(format!(
                    "Could not read familiar spellbook '{path}': {reason}"
                )),
                SpellbookRead::Malformed(reason) => Err(format!(
                    "Familiar spellbook '{path}' is not valid JSON: {reason}"
                )),
                SpellbookRead::Parsed(page) => {
                    let mut ids = HashSet::new();
                    let duplicate = page
                        .familiars
                        .as_deref()
                        .unwrap_or_default()
                        .iter()
                        .find(|familiar| !ids.insert(familiar.id.trim()));
                    match duplicate {
                        Some(familiar) => Err(format!(
                            "Familiar spellbook '{path}' repeats id '{}'.",
                            familiar.id.trim()
                        )),
                        None => Ok(page),
                    }
                }
            };
            let source = SpellbookSource {
                layer,
                path,
                alias: index > 0,
            };
            Some((source, page))
        })
}

fn unloaded(sources: Vec<SpellbookSource>, errors: Vec<String>) -> LoadedSpellbook {
    LoadedSpellbook {
        sources,
        status: FamiliarStatus {
            ok: false,
            errors,
            spellbook: None,
        },
    }
}

// Refuse either malformed layer before overlay can hide its invalid entries.
pub fn load_spellbook(
    room_dir: &str,
    mut read: impl FnMut(&str) -> SpellbookRead,
) -> LoadedSpellbook {
    let layers = [
        ("house", house_dir(room_dir)),
        ("room", Some(room_dir.to_owned())),
    ];
    let mut sources = Vec::new();
    let mut book = SpellbookPage::default();
    for (layer, directory) in &layers {
        let Some((source, page)) = directory
            .as_deref()
            .and_then(|directory| read_page(*layer, directory, &mut read))
        else {
            continue;
        };
        sources.push(source);
        match page {
            Ok(page) => book = page.over(book),
            Err(refusal) => return unloaded(sources, vec![refusal]),
        }
    }

    if sources.is_empty() {
        let tried = layers
            .iter()
            .filter_map(|(_, directory)| directory.as_deref())
            .flat_map(spellbook_candidates)
            .collect::<Vec<_>>();
        return unloaded(
            sources,
            vec![format!(
                "No familiar spellbook found. Tried: {}.",
                tried.join(", ")
            )],
        );
    }
    LoadedSpellbook {
        sources,
        status: validate_spellbook(book),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarStatusReceipt {
    #[serde(flatten)]
    pub status: FamiliarStatus,
    pub sources: Vec<SpellbookSource>,
}

pub fn familiar_status(loaded: LoadedSpellbook) -> FamiliarStatusReceipt {
    FamiliarStatusReceipt {
        status: loaded.status,
        sources: loaded.sources,
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
    pub sources: Vec<SpellbookSource>,
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
        sources: Vec::new(),
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
            sources: Vec::new(),
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
            sources: loaded.sources,
            spellbook: None,
        };
    }

    let receipt = familiar_dispatch(loaded.status, &familiar, request);
    let matched = receipt.familiar.is_some();
    HouseDispatchReceipt {
        dispatch: receipt.dispatch,
        selector,
        familiar: receipt.familiar,
        sources: loaded.sources,
        spellbook: matched.then_some(identity).flatten(),
    }
}
