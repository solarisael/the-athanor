//! One ranking profile per recall mode.
//!
//! A profile never picks or drops a candidate: it rides the score a lane
//! already earned, and it breaks ties inside a canon exactness tier. With the
//! room gate off every mode is flat, so the mode name still travels and only
//! telemetry can tell the modes apart.

use hearth::RecallMode;
use serde_json::Value;
use std::cmp::Ordering;

/// A memory younger than this window rides the recency lift.
const RECENCY_WINDOW_DAYS: f64 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RankingProfile {
    mode: RecallMode,
    canon_kinds: &'static [(&'static str, f64)],
    memory_types: &'static [(&'static str, f64)],
    recency: f64,
}

/// The room's people, its rituals and its stories, weighted toward what was
/// said lately.
const CONVERSATION: RankingProfile = RankingProfile {
    mode: RecallMode::Conversation,
    canon_kinds: &[("person", 2.0), ("ritual", 2.0), ("mythology", 2.0)],
    memory_types: &[("narrative", 1.5), ("relationship", 1.5)],
    recency: 1.1,
};

/// Projects and the decisions taken in them. A decision does not go stale
/// because it is old, so work carries no recency bias.
const WORK: RankingProfile = RankingProfile {
    mode: RecallMode::Work,
    canon_kinds: &[("project", 2.0)],
    memory_types: &[("decision", 1.5), ("technical", 1.5)],
    recency: 1.0,
};

const fn flat(mode: RecallMode) -> RankingProfile {
    RankingProfile {
        mode,
        canon_kinds: &[],
        memory_types: &[],
        recency: 1.0,
    }
}

/// The profile a mode ranks with. `enabled` is the room gate: off, every mode
/// is flat and ranking is exactly what it was before modes existed.
pub(crate) fn for_mode(mode: RecallMode, enabled: bool) -> RankingProfile {
    if !enabled {
        return flat(mode);
    }
    match mode {
        RecallMode::Conversation => CONVERSATION,
        RecallMode::Work => WORK,
        RecallMode::Mixed | RecallMode::Quiet => flat(mode),
    }
}

/// One canon row as far as ordering is concerned.
pub(crate) struct CanonOrder<'a> {
    pub exactness: i32,
    pub kind: &'a str,
    pub weighty: bool,
    pub name: &'a str,
}

impl RankingProfile {
    pub(crate) fn is_flat(&self) -> bool {
        self.canon_kinds.is_empty() && self.memory_types.is_empty() && self.recency == 1.0
    }

    /// Canon kind weight, used only inside one exactness tier: an exact row the
    /// caller named never loses its seat to a weighted kind below it.
    pub(crate) fn canon_kind_weight(&self, kind: &str) -> f64 {
        weight(self.canon_kinds, kind)
    }

    /// Multiplier for one fused candidate. A candidate whose lane never
    /// selected its memory type passes through untouched.
    pub(crate) fn candidate_multiplier(
        &self,
        memory_type: Option<&str>,
        age_days: Option<f64>,
    ) -> f64 {
        let typed = memory_type.map_or(1.0, |kind| weight(self.memory_types, kind));
        let recent = match age_days {
            Some(age) if age < RECENCY_WINDOW_DAYS => self.recency,
            _ => 1.0,
        };
        typed * recent
    }

    /// Ride the fused scores. Only the lanes that carry `memory_type` and
    /// `memory_age_days` can move; the caller sorts afterwards.
    pub(crate) fn apply_to_candidates(&self, candidates: &mut [Value]) {
        if self.is_flat() {
            return;
        }
        for candidate in candidates.iter_mut() {
            let multiplier = self.candidate_multiplier(
                candidate["memory_type"].as_str(),
                candidate["memory_age_days"].as_f64(),
            );
            if multiplier == 1.0 {
                continue;
            }
            let scored = candidate["score"].as_f64().unwrap_or(0.0) * multiplier;
            candidate["score"] = Value::from(scored);
            if let Some(reasons) = candidate["reasons"].as_array_mut() {
                reasons.push(Value::from(format!(
                    "{} mode ranking profile",
                    self.mode.as_str()
                )));
            }
        }
    }

    /// Canon order: the exactness tier first, then the profile's kind weight,
    /// then the House's standing order of weighty, then name.
    pub(crate) fn compare_canon(&self, left: &CanonOrder<'_>, right: &CanonOrder<'_>) -> Ordering {
        left.exactness
            .cmp(&right.exactness)
            .then_with(|| {
                self.canon_kind_weight(right.kind)
                    .partial_cmp(&self.canon_kind_weight(left.kind))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| right.weighty.cmp(&left.weighty))
            .then_with(|| left.name.cmp(right.name))
    }
}

/// Apply the profile, then put the fused candidates in final order. The fusion
/// block calls exactly this, so a proof that reads this reads production.
pub(crate) fn apply_and_order(profile: &RankingProfile, candidates: &mut [Value]) {
    profile.apply_to_candidates(candidates);
    candidates.sort_by(compare_candidates);
}

/// Final order of the fused candidates: score first, then a stable path, chunk
/// and memory walk so equal scores never shuffle between runs.
pub(crate) fn compare_candidates(left: &Value, right: &Value) -> Ordering {
    right["score"]
        .as_f64()
        .unwrap_or(0.0)
        .partial_cmp(&left["score"].as_f64().unwrap_or(0.0))
        .unwrap_or(Ordering::Equal)
        .then_with(|| {
            left["source_path"]
                .as_str()
                .cmp(&right["source_path"].as_str())
        })
        .then_with(|| left["chunk_index"].as_i64().cmp(&right["chunk_index"].as_i64()))
        .then_with(|| left["memory_id"].as_i64().cmp(&right["memory_id"].as_i64()))
}

fn weight(rows: &[(&'static str, f64)], name: &str) -> f64 {
    rows.iter()
        .find(|(row, _)| *row == name)
        .map_or(1.0, |(_, weight)| *weight)
}

#[cfg(test)]
mod tests {
    use super::{CanonOrder, apply_and_order, for_mode};
    use hearth::{RecallMode, RecallRequest, UnknownRecallMode};
    use protocol::{ProtocolError, RecallParams};
    use serde_json::{Value, json};

    /// One fixed corpus, read the same way by every mode under test.
    fn corpus() -> Vec<Value> {
        vec![
            candidate(1, "technical", 0.50, "a.md", None),
            candidate(2, "narrative", 0.48, "b.md", None),
            candidate(3, "decision", 0.46, "c.md", None),
            candidate(4, "relationship", 0.44, "d.md", None),
            candidate(5, "reference", 0.42, "e.md", Some(3.0)),
        ]
    }

    fn candidate(
        memory_id: i64,
        memory_type: &str,
        score: f64,
        source_path: &str,
        age_days: Option<f64>,
    ) -> Value {
        json!({
            "memory_id": memory_id,
            "memory_type": memory_type,
            "memory_age_days": age_days,
            "score": score,
            "source_path": source_path,
            "chunk_index": 0,
            "reasons": ["BM25F field-aware lexical score"],
        })
    }

    fn ranked(mode: RecallMode, enabled: bool) -> Vec<i64> {
        let mut candidates = corpus();
        apply_and_order(&for_mode(mode, enabled), &mut candidates);
        candidates
            .iter()
            .map(|candidate| candidate["memory_id"].as_i64().unwrap())
            .collect()
    }

    #[test]
    fn mode_profiles_reorder_one_corpus_and_stay_flat_when_the_gate_is_off() {
        let conversation = ranked(RecallMode::Conversation, true);
        let work = ranked(RecallMode::Work, true);
        assert_eq!(conversation[..3], [2, 4, 1]);
        assert_eq!(work[..3], [1, 3, 2]);
        assert_ne!(conversation[..3], work[..3]);

        let gated_conversation = ranked(RecallMode::Conversation, false);
        let gated_work = ranked(RecallMode::Work, false);
        assert_eq!(gated_conversation, gated_work);
        assert_eq!(gated_conversation[..3], [1, 2, 3]);
        assert_eq!(ranked(RecallMode::Mixed, true), gated_conversation);
    }

    #[test]
    fn the_conversation_profile_lifts_a_recent_memory_and_work_ignores_age() {
        let conversation = for_mode(RecallMode::Conversation, true);
        assert_eq!(
            conversation.candidate_multiplier(Some("reference"), Some(3.0)),
            1.1
        );
        assert_eq!(
            conversation.candidate_multiplier(Some("reference"), Some(31.0)),
            1.0
        );
        // A lane that never selected the type multiplies by nothing.
        assert_eq!(conversation.candidate_multiplier(None, None), 1.0);
        assert_eq!(
            for_mode(RecallMode::Work, true).candidate_multiplier(Some("decision"), Some(3.0)),
            1.5
        );
    }

    #[test]
    fn canon_order_lifts_the_profile_kinds_inside_the_exactness_tier() {
        let work = for_mode(RecallMode::Work, true);
        let project = CanonOrder {
            exactness: 1,
            kind: "project",
            weighty: false,
            name: "striatum",
        };
        let person = CanonOrder {
            exactness: 1,
            kind: "person",
            weighty: true,
            name: "sol",
        };
        let exact_person = CanonOrder {
            exactness: 0,
            kind: "person",
            weighty: false,
            name: "sol",
        };
        assert!(work.compare_canon(&project, &person).is_lt());
        // Exactness still outranks every weight: the named row keeps its seat.
        assert!(work.compare_canon(&exact_person, &project).is_lt());
        // Gate off: the House's standing order, weighty first.
        let flat = for_mode(RecallMode::Work, false);
        assert!(flat.compare_canon(&project, &person).is_gt());
    }

    #[test]
    fn an_unknown_mode_string_is_refused_by_the_typed_boundary() {
        assert_eq!(
            RecallMode::try_from("nostalgia"),
            Err(UnknownRecallMode("nostalgia".into()))
        );
        let params: RecallParams = serde_json::from_value(json!({
            "room": "lab",
            "query": "alpha",
            "mode": "nostalgia"
        }))
        .unwrap();
        let refusal = RecallRequest::try_from(params).unwrap_err();
        let ProtocolError::InvalidParams(message) = refusal else {
            panic!("an unknown mode must refuse as invalid params, got {refusal:?}");
        };
        assert!(
            message.contains("unknown recall mode `nostalgia`"),
            "refusal names the mode it refused: {message}"
        );
    }

    #[test]
    fn a_recall_without_a_mode_resolves_to_mixed() {
        let params: RecallParams =
            serde_json::from_value(json!({"room": "lab", "query": "alpha"})).unwrap();
        assert_eq!(params.mode, None);
        assert_eq!(
            RecallRequest::try_from(params).unwrap().mode(),
            RecallMode::Mixed
        );
        let named: RecallParams = serde_json::from_value(json!({
            "room": "lab",
            "query": "alpha",
            "mode": "conversation"
        }))
        .unwrap();
        assert_eq!(
            RecallRequest::try_from(named).unwrap().mode(),
            RecallMode::Conversation
        );
    }
}
