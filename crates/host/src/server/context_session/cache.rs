use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BlockKind {
    RoomContext,
    RoutingMode,
    WakeContext,
    AnamnesisWake,
    KeywordDirective,
    HallwayBell,
    RecallContext,
    PresenceContext,
}

impl BlockKind {
    pub(super) fn singleton(self) -> bool {
        matches!(
            self,
            Self::RoomContext | Self::RoutingMode | Self::WakeContext | Self::AnamnesisWake
        )
    }

    pub(super) fn dynamic(self) -> bool {
        matches!(self, Self::RecallContext | Self::PresenceContext)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Block {
    pub kind: BlockKind,
    pub content: String,
    #[serde(default)]
    pub details: Value,
    pub timestamp: u64,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct TurnBlocks {
    pub turn_id: String,
    pub blocks: Vec<Block>,
}

pub(super) const WORK_DECAY_TURNS: u64 = 6;
pub(super) const CONTEXT_VERSION: u8 = 2;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct WorkEvidence {
    #[serde(default)]
    pub epoch: String,
    pub revision: u64,
    pub turn: u64,
    pub marked_turn: Option<u64>,
}

impl WorkEvidence {
    pub fn observe(&mut self, epoch: &str, revision: u64, turn: u64) -> bool {
        if self.epoch != epoch {
            *self = Self {
                epoch: epoch.into(),
                ..Self::default()
            };
        }
        if revision > 0 && revision != self.revision {
            self.revision = revision;
            self.marked_turn = Some(self.turn);
        }
        self.turn = self.turn.max(turn);
        self.marked_turn
            .is_some_and(|marked| self.turn.saturating_sub(marked) < WORK_DECAY_TURNS)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct CacheInvalidation {
    pub reason: String,
    pub prior_generation: String,
    pub prior_content_sha256: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct RecallSubject {
    pub turn: protocol::RecallContextTurn,
    pub project: Option<String>,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct DynamicRetirement {
    recall: usize,
    presence: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct SavedSession {
    pub version: u8,
    pub turns: Vec<TurnBlocks>,
    // Per-kind frontiers retire active material without changing sent blocks or retiring reused Recall.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub retired_dynamic_through: BTreeMap<String, DynamicRetirement>,
    #[serde(default)]
    pub woken: bool,
    #[serde(default)]
    pub work: WorkEvidence,
    #[serde(default)]
    pub identity_epoch: Option<String>,
    #[serde(default)]
    pub generation: String,
    #[serde(default)]
    pub last_invalidation: Option<CacheInvalidation>,
    #[serde(default)]
    pub recall_subject: Option<RecallSubject>,
}

impl Default for SavedSession {
    fn default() -> Self {
        Self {
            version: CONTEXT_VERSION,
            turns: Vec::new(),
            retired_dynamic_through: BTreeMap::new(),
            woken: false,
            work: WorkEvidence::default(),
            identity_epoch: None,
            generation: String::new(),
            last_invalidation: None,
            recall_subject: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct LegacyMemo {
    pub version: u8,
    pub turns: Vec<TurnBlocks>,
}

#[derive(Default)]
pub(in super::super) struct ContextSession {
    pub(super) saved: Option<SavedSession>,
    pub(super) plans: BTreeMap<String, super::lessons::LessonPlan>,
    pub(super) sieves: super::lessons::SieveCache,
    pub(super) woken: bool,
}

#[derive(Default)]
pub(in super::super) struct ContextSessions {
    sessions: HashMap<String, Arc<Mutex<ContextSession>>>,
    recent: VecDeque<String>,
    active: HashMap<String, std::sync::Weak<Mutex<ContextSession>>>,
    pub coverage: HashMap<String, Value>,
    pub telemetry_lock: Arc<std::sync::Mutex<()>>,
}

impl ContextSessions {
    pub(super) fn get(&mut self, session: &str) -> Arc<Mutex<ContextSession>> {
        self.active.retain(|_, session| session.strong_count() > 0);
        let entry = self
            .active
            .get(session)
            .and_then(std::sync::Weak::upgrade)
            .unwrap_or_default();
        self.active.insert(session.into(), Arc::downgrade(&entry));
        self.sessions.insert(session.into(), entry.clone());
        self.recent.retain(|key| key != session);
        self.recent.push_back(session.into());
        // ponytail: 128 resident sessions, visible turns retained on disk; upgrade when retention GC receives an explicit contract.
        if self.recent.len() > 128 {
            if let Some(oldest) = self.recent.pop_front() {
                self.sessions.remove(&oldest);
            }
        }
        entry
    }
}

pub(super) fn file(state: &AppState, session: &str) -> PathBuf {
    let key = format!(
        "{}\0{}\0{}",
        state.config.house_id, state.config.room, session
    );
    let digest = format!("{:x}", Sha256::digest(key.as_bytes()));
    state
        .config
        .state_dir
        .join("context-turns")
        .join(format!("{digest}.json"))
}

pub(super) fn load(
    state: &AppState,
    session: &str,
    legacy: Option<LegacyMemo>,
    visible: &[String],
) -> Result<SavedSession, String> {
    let path = file(state, session);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let saved: SavedSession = serde_json::from_slice(&bytes)
                .map_err(|error| format!("invalid native turn cache: {error}"))?;
            if !matches!(saved.version, 1 | CONTEXT_VERSION) {
                return Err("unsupported native turn cache".into());
            }
            Ok(saved)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => adopt(legacy, visible),
        Err(error) => Err(format!("cannot read native turn cache: {error}")),
    }
}

fn adopt(legacy: Option<LegacyMemo>, visible: &[String]) -> Result<SavedSession, String> {
    let mut saved = SavedSession::default();
    if let Some(legacy) = legacy {
        if legacy.version != 1 {
            return Err("unsupported legacy turn cache".into());
        }
        saved.version = 1;
        let mut seen = std::collections::HashSet::new();
        for turn in legacy.turns {
            if !visible.contains(&turn.turn_id) || !seen.insert(turn.turn_id.clone()) {
                return Err("legacy turn cache has an invisible or repeated turn".into());
            }
            for block in &turn.blocks {
                if !block.details.is_null() && !block.details.is_object() {
                    return Err("legacy context block has invalid details".into());
                }
            }
            saved.turns.push(turn);
        }
    }
    Ok(saved)
}

pub(super) fn commit(state: &AppState, session: &str, saved: &SavedSession) -> Result<(), String> {
    crate::store::atomic_json_write(&file(state, session), saved)
}

fn latest_entry(saved: &SavedSession, kind: BlockKind) -> Option<(&TurnBlocks, &Block)> {
    saved.turns.iter().rev().find_map(|turn| {
        let frontier = if kind.dynamic() {
            saved
                .retired_dynamic_through
                .get(&turn.turn_id)
                .map_or(0, |frontier| match kind {
                    BlockKind::RecallContext => frontier.recall,
                    BlockKind::PresenceContext => frontier.presence,
                    _ => 0,
                })
        } else {
            0
        };

        turn.blocks
            .iter()
            .skip(frontier)
            .rev()
            .find(|block| block.kind == kind)
            .map(|block| (turn, block))
    })
}

pub(super) fn latest(saved: &SavedSession, kind: BlockKind) -> Option<&Block> {
    latest_entry(saved, kind).map(|(_, block)| block)
}

// Only the latest active block of each kind can supply current authority.
pub(super) fn latest_for_turn<'a>(
    saved: &'a SavedSession,
    turn_id: &str,
    kind: BlockKind,
) -> Option<&'a Block> {
    latest_entry(saved, kind)
        .filter(|(turn, _)| turn.turn_id == turn_id)
        .map(|(_, block)| block)
}

pub(super) fn has_kind(saved: &SavedSession, kind: BlockKind) -> bool {
    latest(saved, kind).is_some()
}

pub(super) fn retire_recall(saved: &mut SavedSession) -> bool {
    retire_dynamic(saved, true, true)
}

fn retire_dynamic(saved: &mut SavedSession, recall: bool, presence: bool) -> bool {
    if !recall && !presence {
        return false;
    }

    let mut changed = false;
    for turn in &saved.turns {
        let mut frontier = saved
            .retired_dynamic_through
            .get(&turn.turn_id)
            .copied()
            .unwrap_or_default();
        let recall_active = recall
            && turn
                .blocks
                .iter()
                .skip(frontier.recall)
                .any(|block| block.kind == BlockKind::RecallContext);
        let presence_active = presence
            && turn
                .blocks
                .iter()
                .skip(frontier.presence)
                .any(|block| block.kind == BlockKind::PresenceContext);

        if !recall_active && !presence_active {
            continue;
        }

        if recall_active {
            frontier.recall = turn.blocks.len();
        }
        if presence_active {
            frontier.presence = turn.blocks.len();
        }
        if let Some(current) = saved.retired_dynamic_through.get_mut(&turn.turn_id) {
            *current = frontier;
        } else {
            saved
                .retired_dynamic_through
                .insert(turn.turn_id.clone(), frontier);
        }
        changed = true;
    }

    changed
}

pub(super) fn prune(saved: &mut SavedSession, visible: &[String]) -> bool {
    let turns = saved.turns.len();
    let frontiers = saved.retired_dynamic_through.len();
    saved.turns.retain(|turn| visible.contains(&turn.turn_id));
    saved
        .retired_dynamic_through
        .retain(|turn_id, _| saved.turns.iter().any(|turn| &turn.turn_id == turn_id));

    saved.turns.len() != turns || saved.retired_dynamic_through.len() != frontiers
}

pub(super) fn merge(saved: &mut SavedSession, turn_id: &str, blocks: Vec<Block>) {
    let blocks: Vec<_> = blocks
        .into_iter()
        .filter(|block| !block.kind.singleton() || !has_kind(saved, block.kind))
        .collect();
    let recall = blocks
        .iter()
        .any(|block| block.kind == BlockKind::RecallContext);
    let presence = blocks
        .iter()
        .any(|block| block.kind == BlockKind::PresenceContext);
    retire_dynamic(saved, recall, presence);

    if let Some(turn) = saved.turns.iter_mut().find(|turn| turn.turn_id == turn_id) {
        turn.blocks.extend(blocks);
    } else {
        saved.turns.push(TurnBlocks {
            turn_id: turn_id.into(),
            blocks,
        });
    }
}
