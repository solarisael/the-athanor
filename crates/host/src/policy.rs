use protocol::{
    RecallAction, RecallPolicyDecision, RecallPolicyFacts, RecallPolicyState,
    RecallRefreshCompletion, RecallRequestedMode, RecallResolvedMode, RecoveryState,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const WORKING_SET_STALE_TURNS: u64 = 8;
const WORKING_SET_STALE_TOKEN_DELTA: u64 = 4_096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RecallWorkingSet {
    query_terms: Vec<String>,
    mode: RecallResolvedMode,
    active_project: Option<String>,
    entries: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallPolicySession {
    requested_mode: RecallRequestedMode,
    resolved_mode: RecallResolvedMode,
    active_project: Option<String>,
    resolution_reason: String,
    conversation_streak: u64,
    turns_since_refresh: u64,
    observed_conversation_tokens: u64,
    last_refresh_conversation_tokens: u64,
    working_set: Option<RecallWorkingSet>,
    recovery_state: RecoveryState,
    last_refresh_reason: Option<String>,
    last_refresh_at: Option<String>,
    degraded: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecallPolicySession {
    requested_mode: RecallRequestedMode,
    resolved_mode: RecallResolvedMode,
    active_project: Option<String>,
    resolution_reason: String,
    conversation_streak: u64,
    turns_since_refresh: u64,
    observed_conversation_tokens: u64,
    last_refresh_conversation_tokens: u64,
    working_set: Option<RecallWorkingSet>,
    recovery_pending: bool,
    recovery_terms: Vec<String>,
    last_refresh_reason: Option<String>,
    last_refresh_at: Option<String>,
    degraded: Option<String>,
}

impl<'de> Deserialize<'de> for RecallPolicySession {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let legacy = LegacyRecallPolicySession::deserialize(deserializer)?;
        Ok(Self {
            requested_mode: legacy.requested_mode,
            resolved_mode: legacy.resolved_mode,
            active_project: legacy.active_project,
            resolution_reason: legacy.resolution_reason,
            conversation_streak: legacy.conversation_streak,
            turns_since_refresh: legacy.turns_since_refresh,
            observed_conversation_tokens: legacy.observed_conversation_tokens,
            last_refresh_conversation_tokens: legacy.last_refresh_conversation_tokens,
            working_set: legacy.working_set,
            recovery_state: if legacy.recovery_pending {
                RecoveryState::Pending {
                    terms: legacy.recovery_terms,
                }
            } else {
                RecoveryState::Idle
            },
            last_refresh_reason: legacy.last_refresh_reason,
            last_refresh_at: legacy.last_refresh_at,
            degraded: legacy.degraded,
        })
    }
}

impl Serialize for RecallPolicySession {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Legacy<'a> {
            requested_mode: RecallRequestedMode,
            resolved_mode: RecallResolvedMode,
            active_project: &'a Option<String>,
            resolution_reason: &'a String,
            conversation_streak: u64,
            turns_since_refresh: u64,
            observed_conversation_tokens: u64,
            last_refresh_conversation_tokens: u64,
            working_set: &'a Option<RecallWorkingSet>,
            recovery_pending: bool,
            recovery_terms: &'a [String],
            last_refresh_reason: &'a Option<String>,
            last_refresh_at: &'a Option<String>,
            degraded: &'a Option<String>,
        }
        Legacy {
            requested_mode: self.requested_mode,
            resolved_mode: self.resolved_mode,
            active_project: &self.active_project,
            resolution_reason: &self.resolution_reason,
            conversation_streak: self.conversation_streak,
            turns_since_refresh: self.turns_since_refresh,
            observed_conversation_tokens: self.observed_conversation_tokens,
            last_refresh_conversation_tokens: self.last_refresh_conversation_tokens,
            working_set: &self.working_set,
            recovery_pending: self.recovery_state.is_pending(),
            recovery_terms: self.recovery_state.terms(),
            last_refresh_reason: &self.last_refresh_reason,
            last_refresh_at: &self.last_refresh_at,
            degraded: &self.degraded,
        }
        .serialize(serializer)
    }
}

impl RecallPolicySession {
    pub fn from_projection(projection: &RecallPolicyState) -> Self {
        Self {
            requested_mode: projection.requested_mode,
            resolved_mode: projection.resolved_mode,
            active_project: projection.active_project.clone(),
            resolution_reason: projection.resolution_reason.clone(),
            conversation_streak: 0,
            turns_since_refresh: 0,
            observed_conversation_tokens: 0,
            last_refresh_conversation_tokens: 0,
            working_set: None,
            recovery_state: projection.recovery_state.clone(),
            last_refresh_reason: projection.last_refresh_reason.clone(),
            last_refresh_at: projection.last_refresh_at.clone(),
            degraded: projection.degraded.clone(),
        }
    }

    pub fn fresh(projection: &RecallPolicyState) -> Self {
        let resolved_mode = projection.requested_mode.resolved();
        Self {
            requested_mode: projection.requested_mode,
            resolved_mode,
            active_project: None,
            resolution_reason: if projection.requested_mode == RecallRequestedMode::Auto {
                "default".to_owned()
            } else {
                "explicit-override".to_owned()
            },
            conversation_streak: 0,
            turns_since_refresh: 0,
            observed_conversation_tokens: 0,
            last_refresh_conversation_tokens: 0,
            working_set: None,
            recovery_state: RecoveryState::Idle,
            last_refresh_reason: None,
            last_refresh_at: None,
            degraded: None,
        }
    }

    pub fn evaluate(
        &mut self,
        requested_mode: RecallRequestedMode,
        facts: RecallPolicyFacts,
    ) -> RecallPolicyDecision {
        if !facts.working_set_present {
            self.working_set = None;
        }
        let intent = normalized(&facts.query_route.intent).unwrap_or_else(|| "general".to_owned());
        let active_project = facts.active_project.and_then(|value| normalized(&value));
        let previous_mode = self.resolved_mode;
        let previous_project = self.active_project.clone();
        let requested_changed = requested_mode != self.requested_mode;

        let (resolved_mode, resolution_reason, conversation_streak) =
            if requested_mode == RecallRequestedMode::Auto {
                resolve_auto_mode(
                    self,
                    &intent,
                    active_project.as_deref(),
                    facts.tool_evidence,
                )
            } else {
                (requested_mode.resolved(), "explicit-override".to_owned(), 0)
            };

        let mode_changed = requested_changed || resolved_mode != previous_mode;
        let project_changed = active_project != previous_project;
        if mode_changed || project_changed {
            self.working_set = None;
        }

        self.requested_mode = requested_mode;
        self.resolved_mode = resolved_mode;
        self.active_project = active_project.clone();
        self.resolution_reason = resolution_reason;
        self.conversation_streak = conversation_streak;
        self.turns_since_refresh = self.turns_since_refresh.saturating_add(1);
        self.observed_conversation_tokens = facts.conversation_tokens;

        let explicit_lookup = is_explicit_lookup(&intent);
        let technical = intent == "technical_project";
        let recovery_due = self.recovery_state.is_pending();
        let recovery_query_terms = self.recovery_state.terms();
        let eligible = resolved_mode != RecallResolvedMode::Quiet
            && (explicit_lookup || technical || recovery_due);
        let query_terms = unique_terms(
            facts
                .query_route
                .required_terms
                .into_iter()
                .chain(facts.query_route.recognized_entities)
                .chain(
                    matches!(
                        resolved_mode,
                        RecallResolvedMode::Work | RecallResolvedMode::Mixed
                    )
                    .then_some(active_project.clone())
                    .flatten(),
                )
                .chain(recovery_query_terms.iter().cloned())
                .chain(facts.query_route.terms),
            16,
        );
        let prior_terms = self
            .working_set
            .as_ref()
            .map(|working_set| working_set.query_terms.as_slice())
            .unwrap_or_default();
        let topic_changed = query_terms.len() >= 3
            && prior_terms.len() >= 3
            && overlap_ratio(&query_terms, prior_terms) < 0.25;
        let explicit_query_changed = explicit_lookup && query_terms.as_slice() != prior_terms;
        let stale = self.turns_since_refresh >= WORKING_SET_STALE_TURNS
            || self
                .observed_conversation_tokens
                .saturating_sub(self.last_refresh_conversation_tokens)
                >= WORKING_SET_STALE_TOKEN_DELTA;

        let refresh_reason = if eligible && recovery_due {
            Some("post-compaction-recovery")
        } else if eligible && requested_changed {
            Some("requested-mode-change")
        } else if eligible && project_changed {
            Some("active-project-change")
        } else if eligible && mode_changed {
            Some("resolved-mode-change")
        } else if eligible && self.working_set.is_none() {
            Some("empty-working-set")
        } else if eligible && explicit_query_changed {
            Some("explicit-lookup")
        } else if eligible && topic_changed {
            Some("topic-change")
        } else if eligible && stale {
            Some("stale-working-set")
        } else {
            None
        }
        .map(str::to_owned);

        let refresh = refresh_reason.is_some() && !query_terms.is_empty();
        let clear = resolved_mode == RecallResolvedMode::Quiet || mode_changed || project_changed;
        RecallPolicyDecision {
            action: match (clear, refresh) {
                (false, false) => RecallAction::None,
                (true, false) => RecallAction::Clear,
                (false, true) => RecallAction::Refresh,
                (true, true) => RecallAction::ClearThenRefresh,
            },
            query: query_terms.join(" "),
            query_terms,
            refresh_reason,
            intent,
            resolved_mode,
        }
    }

    pub fn complete_refresh(&mut self, input: RecallRefreshCompletion, now: String) {
        self.turns_since_refresh = 0;
        self.last_refresh_conversation_tokens = self.observed_conversation_tokens;
        self.recovery_state = RecoveryState::Idle;
        self.last_refresh_reason = Some(input.refresh_reason);
        self.last_refresh_at = Some(now);
        self.degraded = input.warning.and_then(|value| bounded(&value, 240));
        self.working_set = input.has_working_set.then(|| RecallWorkingSet {
            query_terms: unique_terms(input.query_terms, 16),
            mode: self.resolved_mode,
            active_project: self.active_project.clone(),
            entries: input.entries,
        });
    }

    pub fn fail_refresh(&mut self, reason: &str, now: String) {
        self.last_refresh_reason = Some("failed".to_owned());
        self.last_refresh_at = Some(now);
        self.degraded = bounded(reason, 240).or_else(|| Some("recall unavailable".to_owned()));
    }

    pub fn invalidate_after_compaction(&mut self, summary: &str) {
        self.working_set = None;
        self.recovery_state = RecoveryState::Pending {
            terms: summary_terms(summary),
        };
        self.turns_since_refresh = 0;
        self.last_refresh_reason = Some("compaction-invalidated".to_owned());
    }

    pub fn projection(&self, updated_at: String) -> RecallPolicyState {
        RecallPolicyState {
            requested_mode: self.requested_mode,
            resolved_mode: self.resolved_mode,
            active_project: self.active_project.clone(),
            resolution_reason: self.resolution_reason.clone(),
            last_refresh_reason: self.last_refresh_reason.clone(),
            last_refresh_at: self.last_refresh_at.clone(),
            working_set_entries: self
                .working_set
                .as_ref()
                .map(|working_set| working_set.entries)
                .unwrap_or(0),
            recovery_state: self.recovery_state.clone(),
            degraded: self.degraded.clone(),
            updated_at: Some(updated_at),
        }
    }
}

pub fn apply_requested_mode(
    projection: &RecallPolicyState,
    requested_mode: RecallRequestedMode,
    now: String,
) -> RecallPolicyState {
    let mut next = projection.clone();
    next.requested_mode = requested_mode;
    if requested_mode == RecallRequestedMode::Auto {
        next.resolution_reason = "awaiting-auto-resolution".to_owned();
    } else {
        next.resolved_mode = requested_mode.resolved();
        next.resolution_reason = "explicit-override".to_owned();
    }
    next.updated_at = Some(now);
    next
}

fn resolve_auto_mode(
    session: &RecallPolicySession,
    intent: &str,
    active_project: Option<&str>,
    tool_evidence: bool,
) -> (RecallResolvedMode, String, u64) {
    if intent == "technical_project" {
        return (RecallResolvedMode::Work, "technical-project".to_owned(), 0);
    }
    if is_explicit_lookup(intent) {
        return if active_project.is_some() {
            (
                RecallResolvedMode::Mixed,
                "project-aware-lookup".to_owned(),
                0,
            )
        } else {
            (
                RecallResolvedMode::Conversation,
                "explicit-lookup".to_owned(),
                0,
            )
        };
    }
    // Hands on files outrank prompt vocabulary: a session that has edited or
    // written this conversation is working, whatever the words sound like.
    if tool_evidence {
        return (RecallResolvedMode::Work, "tool-evidence".to_owned(), 0);
    }
    if active_project.is_some()
        && matches!(
            session.resolved_mode,
            RecallResolvedMode::Work | RecallResolvedMode::Mixed
        )
    {
        let streak = session.conversation_streak.saturating_add(1);
        return if streak >= 2 {
            (
                RecallResolvedMode::Conversation,
                "conversation-hysteresis-complete".to_owned(),
                streak,
            )
        } else {
            (
                RecallResolvedMode::Mixed,
                "conversation-hysteresis".to_owned(),
                streak,
            )
        };
    }
    (
        RecallResolvedMode::Conversation,
        if intent == "casual_contact" {
            "casual-contact"
        } else {
            "general"
        }
        .to_owned(),
        0,
    )
}

fn is_explicit_lookup(intent: &str) -> bool {
    matches!(intent, "memory_lookup" | "entity_lookup" | "date_lookup")
}

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn bounded(value: &str, limit: usize) -> Option<String> {
    normalized(value).map(|value| value.chars().take(limit).collect())
}

fn unique_terms(values: impl IntoIterator<Item = String>, limit: usize) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let term = value.trim().to_lowercase();
        if term.chars().count() < 2 || !seen.insert(term.clone()) {
            continue;
        }
        terms.push(term);
        if terms.len() >= limit {
            break;
        }
    }
    terms
}

fn summary_terms(summary: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut current = String::new();
    for character in summary.chars() {
        if is_summary_initial(character)
            || matches!(character, '_' | '.' | ':' | '/' | '+' | '#' | '-')
        {
            current.push(character);
        } else {
            push_summary_candidate(&mut candidates, &mut current);
        }
    }
    push_summary_candidate(&mut candidates, &mut current);
    unique_terms(candidates, 12)
}

fn is_summary_initial(character: char) -> bool {
    character.is_ascii_alphanumeric() || ('\u{00c0}'..='\u{00ff}').contains(&character)
}

fn push_summary_candidate(candidates: &mut Vec<String>, current: &mut String) {
    let candidate = current.trim_start_matches(|character| !is_summary_initial(character));
    if candidate.chars().count() >= 3 {
        candidates.push(candidate.to_owned());
    }
    current.clear();
}

fn overlap_ratio(left: &[String], right: &[String]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let right: HashSet<&str> = right.iter().map(String::as_str).collect();
    let overlap = left
        .iter()
        .filter(|term| right.contains(term.as_str()))
        .count();
    overlap as f64 / left.len().min(right.len()) as f64
}

trait ResolvedRequestedMode {
    fn resolved(self) -> RecallResolvedMode;
}

impl ResolvedRequestedMode for RecallRequestedMode {
    fn resolved(self) -> RecallResolvedMode {
        match self {
            Self::Auto | Self::Conversation => RecallResolvedMode::Conversation,
            Self::Work => RecallResolvedMode::Work,
            Self::Quiet => RecallResolvedMode::Quiet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RecallPolicySession, resolve_auto_mode};
    use protocol::{
        RecallPolicyState, RecallRequestedMode, RecallResolvedMode, RecoveryState,
    };

    fn projection() -> RecallPolicyState {
        RecallPolicyState {
            requested_mode: RecallRequestedMode::Auto,
            resolved_mode: RecallResolvedMode::Conversation,
            active_project: None,
            resolution_reason: "default".to_owned(),
            last_refresh_reason: None,
            last_refresh_at: None,
            working_set_entries: 0,
            recovery_state: RecoveryState::Idle,
            degraded: None,
            updated_at: None,
        }
    }

    fn session(resolved_mode: RecallResolvedMode, conversation_streak: u64) -> RecallPolicySession {
        let mut session = RecallPolicySession::fresh(&projection());
        session.resolved_mode = resolved_mode;
        session.conversation_streak = conversation_streak;
        session
    }

    #[test]
    fn tool_evidence_resolves_work_whatever_the_prompt_sounds_like() {
        for intent in ["general", "casual_contact", ""] {
            assert_eq!(
                resolve_auto_mode(
                    &session(RecallResolvedMode::Conversation, 0),
                    intent,
                    None,
                    true,
                ),
                (RecallResolvedMode::Work, "tool-evidence".to_owned(), 0),
                "hands on files must outrank the prompt vocabulary of {intent:?}"
            );
        }
    }

    #[test]
    fn tool_evidence_outranks_conversation_hysteresis() {
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Work, 0),
                "casual_contact",
                Some("the-athanor"),
                true,
            ),
            (RecallResolvedMode::Work, "tool-evidence".to_owned(), 0)
        );
    }

    #[test]
    fn intent_and_explicit_lookups_outrank_tool_evidence() {
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Conversation, 0),
                "technical_project",
                Some("the-athanor"),
                true,
            ),
            (RecallResolvedMode::Work, "technical-project".to_owned(), 0)
        );
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Conversation, 0),
                "memory_lookup",
                None,
                true,
            ),
            (
                RecallResolvedMode::Conversation,
                "explicit-lookup".to_owned(),
                0
            )
        );
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Conversation, 0),
                "entity_lookup",
                Some("the-athanor"),
                true,
            ),
            (
                RecallResolvedMode::Mixed,
                "project-aware-lookup".to_owned(),
                0
            )
        );
    }

    #[test]
    fn without_tool_evidence_the_prompt_decides_and_hysteresis_walks_work_back_to_conversation() {
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Conversation, 0),
                "general",
                None,
                false,
            ),
            (RecallResolvedMode::Conversation, "general".to_owned(), 0)
        );
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Conversation, 0),
                "casual_contact",
                None,
                false,
            ),
            (
                RecallResolvedMode::Conversation,
                "casual-contact".to_owned(),
                0
            )
        );
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Work, 0),
                "casual_contact",
                Some("the-athanor"),
                false,
            ),
            (
                RecallResolvedMode::Mixed,
                "conversation-hysteresis".to_owned(),
                1
            )
        );
        assert_eq!(
            resolve_auto_mode(
                &session(RecallResolvedMode::Mixed, 1),
                "casual_contact",
                Some("the-athanor"),
                false,
            ),
            (
                RecallResolvedMode::Conversation,
                "conversation-hysteresis-complete".to_owned(),
                2
            )
        );
    }
}
