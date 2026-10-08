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
pub(super) struct SavedSession {
    pub version: u8,
    pub turns: Vec<TurnBlocks>,
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
}

impl Default for SavedSession {
    fn default() -> Self {
        Self {
            version: 1,
            turns: Vec::new(),
            woken: false,
            work: WorkEvidence::default(),
            identity_epoch: None,
            generation: String::new(),
            last_invalidation: None,
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
            if saved.version != 1 {
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

pub(super) fn has_kind(saved: &SavedSession, kind: BlockKind) -> bool {
    saved
        .turns
        .iter()
        .any(|turn| turn.blocks.iter().any(|block| block.kind == kind))
}

pub(super) fn merge(saved: &mut SavedSession, turn_id: &str, blocks: Vec<Block>) {
    let blocks = blocks
        .into_iter()
        .filter(|block| !block.kind.singleton() || !has_kind(saved, block.kind))
        .collect();
    saved.turns.push(TurnBlocks {
        turn_id: turn_id.into(),
        blocks,
    });
}
