use crate::error::DomainError;

use super::candidate::{GigaClassifierIdentity, GigaReviewState};
use super::source::{GigaSourceRef, giga_nonempty, giga_rfc3339, giga_strings};

#[derive(Clone, Debug, PartialEq)]
pub struct GigaResonance {
    event_id: String,
    score: f64,
    classifier: GigaClassifierIdentity,
    source_refs: Vec<GigaSourceRef>,
}
impl GigaResonance {
    pub fn new(
        event_id: String,
        score: f64,
        classifier: GigaClassifierIdentity,
        source_refs: Vec<GigaSourceRef>,
    ) -> Result<Self, DomainError> {
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return Err(DomainError::InvalidGigaScore {
                field: "resonance_score".into(),
                value: score,
            });
        }
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "resonance.source_refs".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            event_id: giga_nonempty("resonance.event_id", event_id)?,
            score,
            classifier,
            source_refs,
        })
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub const fn score(&self) -> f64 {
        self.score
    }
    pub fn classifier(&self) -> &GigaClassifierIdentity {
        &self.classifier
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GigaReviewAction {
    candidate_id: String,
    reviewer_id: String,
    previous_state: GigaReviewState,
    new_state: GigaReviewState,
    reason: String,
    authorization_basis: String,
    source_refs: Vec<GigaSourceRef>,
    promotion_target: Option<String>,
    merge_target: Option<String>,
    merge_source_candidates: Vec<String>,
    resonance: Option<GigaResonance>,
    reviewed_at: String,
}
impl GigaReviewAction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: String,
        reviewer_id: String,
        previous_state: GigaReviewState,
        new_state: GigaReviewState,
        reason: String,
        authorization_basis: String,
        source_refs: Vec<GigaSourceRef>,
        promotion_target: Option<String>,
        merge_target: Option<String>,
        merge_source_candidates: Vec<String>,
        resonance: Option<GigaResonance>,
        reviewed_at: String,
    ) -> Result<Self, DomainError> {
        if !previous_state.can_transition(new_state) {
            return Err(DomainError::InvalidGigaTransition {
                from: previous_state.as_str().into(),
                to: new_state.as_str().into(),
            });
        }
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "review must retain exact sources".into(),
            });
        }
        let promotion_target = promotion_target
            .map(|value| giga_nonempty("promotion_target", value))
            .transpose()?;
        let merge_target = merge_target
            .map(|value| giga_nonempty("merge_target", value))
            .transpose()?;
        let merge_source_candidates =
            giga_strings("merge_source_candidates", merge_source_candidates)?;
        match new_state {
            GigaReviewState::Promoted if promotion_target.is_none()=>return Err(DomainError::InvalidGiga{field:"promotion_target".into(),message:"required for promotion".into()}),
            GigaReviewState::Merged if merge_target.is_none()||merge_source_candidates.len()<2||!merge_source_candidates.iter().any(|source|source==&candidate_id)||!merge_source_candidates.iter().any(|source|source!=&candidate_id)=>return Err(DomainError::InvalidGiga{field:"merge_target".into(),message:"merge target and all distinct source candidates, including this candidate, are required".into()}),
            GigaReviewState::Corrected|GigaReviewState::Superseded if promotion_target.is_none()||source_refs.len()<2=>return Err(DomainError::InvalidGiga{field:"promotion_target".into(),message:"target and exact new/old source references are required".into()}),
            _=>{}
        }
        if new_state != GigaReviewState::Merged
            && (merge_target.is_some() || !merge_source_candidates.is_empty())
        {
            return Err(DomainError::InvalidGiga {
                field: "merge_target".into(),
                message: "only valid for merged reviews".into(),
            });
        }
        if !matches!(
            new_state,
            GigaReviewState::Promoted | GigaReviewState::Corrected | GigaReviewState::Superseded
        ) && promotion_target.is_some()
        {
            return Err(DomainError::InvalidGiga {
                field: "promotion_target".into(),
                message: "not valid for this transition".into(),
            });
        }
        let resonance_transition =
            previous_state == GigaReviewState::Curio && new_state == GigaReviewState::InReview;
        if resonance_transition != resonance.is_some() {
            return Err(DomainError::InvalidGiga {
                field: "resonance".into(),
                message: "required only for curio resonance to in_review".into(),
            });
        }
        Ok(Self {
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            reviewer_id: giga_nonempty("reviewer_id", reviewer_id)?,
            previous_state,
            new_state,
            reason: giga_nonempty("reason", reason)?,
            authorization_basis: giga_nonempty("authorization_basis", authorization_basis)?,
            source_refs,
            promotion_target,
            merge_target,
            merge_source_candidates,
            resonance,
            reviewed_at: giga_rfc3339("reviewed_at", reviewed_at)?,
        })
    }
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn reviewer_id(&self) -> &str {
        &self.reviewer_id
    }
    pub const fn previous_state(&self) -> GigaReviewState {
        self.previous_state
    }
    pub const fn new_state(&self) -> GigaReviewState {
        self.new_state
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn authorization_basis(&self) -> &str {
        &self.authorization_basis
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn promotion_target(&self) -> Option<&str> {
        self.promotion_target.as_deref()
    }
    pub fn merge_target(&self) -> Option<&str> {
        self.merge_target.as_deref()
    }
    pub fn merge_source_candidates(&self) -> &[String] {
        &self.merge_source_candidates
    }
    pub fn resonance(&self) -> Option<&GigaResonance> {
        self.resonance.as_ref()
    }
    pub fn reviewed_at(&self) -> &str {
        &self.reviewed_at
    }
}
