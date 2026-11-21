use serde::{Deserialize, Serialize};

mod dispatch;
mod spellbook;
#[cfg(test)]
mod tests;

use dispatch::dispatch_with_familiar;
pub use dispatch::{DispatchReceipt, DispatchRequest, DispatchStatus, DispatcherReceipt, dispatch};
pub use spellbook::{
    Familiar, FamiliarStatus, HOUSE_SCOPE, LoadedSpellbook, SpellbookIdentity, SpellbookSource,
    StoredPage, effective_spellbook, spellbook_write_errors, unloaded,
};

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
    let Some(mut spellbook) = status.spellbook else {
        return rejected_familiar(status.errors);
    };
    let key = familiar_name.trim().to_lowercase();
    if let Some(contest) = status.contested.iter().find(|contest| contest.key == key) {
        return rejected_familiar(vec![contest.refusal()]);
    }
    // The room and House familiars come first, so a key they own always wins.
    let familiar = spellbook
        .familiars
        .iter()
        .position(|familiar| spellbook::lookup_keys(familiar).any(|owned| owned == key));
    let Some(familiar) = familiar else {
        let ids = spellbook
            .familiars
            .iter()
            .map(|familiar| familiar.id.as_str())
            .collect::<Vec<_>>();

        return rejected_familiar(vec![format!(
            "Unknown familiar: {}. Valid familiar IDs: {}.",
            if key.is_empty() { "<empty>" } else { &key },
            if ids.is_empty() {
                "<none>".into()
            } else {
                ids.join(", ")
            }
        )]);
    };
    let familiar = spellbook.familiars.swap_remove(familiar);

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

/// The stored pages a status reader may replace: this room's and the House's.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellbookPages {
    pub house: Option<serde_json::Value>,
    pub room: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarStatusReceipt {
    #[serde(flatten)]
    pub status: FamiliarStatus,
    pub sources: Vec<SpellbookSource>,
    pub pages: SpellbookPages,
}

/// The effective spellbook for `room` from every stored page, or the reason
/// the pages could not be read.
pub fn load_spellbook(room: &str, pages: &Result<Vec<StoredPage>, String>) -> LoadedSpellbook {
    match pages {
        Ok(pages) => effective_spellbook(room, pages),
        Err(reason) => unloaded(Vec::new(), vec![format!("Spellbook unavailable: {reason}")]),
    }
}

pub fn familiar_status(
    room: &str,
    pages: &Result<Vec<StoredPage>, String>,
) -> FamiliarStatusReceipt {
    let loaded = load_spellbook(room, pages);
    let page = |scope: &str| {
        pages.as_ref().ok().and_then(|pages| {
            pages
                .iter()
                .find(|(stored, _)| stored == scope)
                .map(|(_, page)| page.clone())
        })
    };
    FamiliarStatusReceipt {
        status: loaded.status,
        sources: loaded.sources,
        pages: SpellbookPages {
            house: page(HOUSE_SCOPE),
            room: page(room),
        },
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
    // Identity follows the familiar's own page over the House page, not the requester's.
    let identity = receipt
        .familiar
        .as_ref()
        .and_then(|familiar| loaded.identities.get(&familiar.origin).cloned());
    HouseDispatchReceipt {
        dispatch: receipt.dispatch,
        selector,
        familiar: receipt.familiar,
        sources: loaded.sources,
        spellbook: identity,
    }
}
