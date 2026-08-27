use crate::error::DomainError;
use crate::room::RoomKey;

use super::source::{
    GigaScope, GigaSourceRef, GigaVisibility, giga_hash, giga_nonempty, giga_rfc3339, giga_strings,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaCandidateKind {
    Memory,
    CodingLesson,
    ProjectLesson,
    Correction,
    Supersession,
    EntityUpdate,
    ThreadUpdate,
}
impl GigaCandidateKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "memory" => Ok(Self::Memory),
            "coding_lesson" => Ok(Self::CodingLesson),
            "project_lesson" => Ok(Self::ProjectLesson),
            "correction" => Ok(Self::Correction),
            "supersession" => Ok(Self::Supersession),
            "entity_update" => Ok(Self::EntityUpdate),
            "thread_update" => Ok(Self::ThreadUpdate),
            other => Err(DomainError::UnknownGigaValue {
                field: "kind".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::CodingLesson => "coding_lesson",
            Self::ProjectLesson => "project_lesson",
            Self::Correction => "correction",
            Self::Supersession => "supersession",
            Self::EntityUpdate => "entity_update",
            Self::ThreadUpdate => "thread_update",
        }
    }
    pub const fn requires_proof(self) -> bool {
        matches!(
            self,
            Self::CodingLesson | Self::ProjectLesson | Self::Correction | Self::Supersession
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GigaScores {
    priority: f64,
    novelty: f64,
    durability: f64,
    confidence: f64,
}
impl GigaScores {
    pub fn new(
        priority: f64,
        novelty: f64,
        durability: f64,
        confidence: f64,
    ) -> Result<Self, DomainError> {
        for (field, value) in [
            ("priority", priority),
            ("novelty", novelty),
            ("durability", durability),
            ("confidence", confidence),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DomainError::InvalidGigaScore {
                    field: field.into(),
                    value,
                });
            }
        }
        Ok(Self {
            priority,
            novelty,
            durability,
            confidence,
        })
    }
    pub const fn priority(&self) -> f64 {
        self.priority
    }
    pub const fn novelty(&self) -> f64 {
        self.novelty
    }
    pub const fn durability(&self) -> f64 {
        self.durability
    }
    pub const fn confidence(&self) -> f64 {
        self.confidence
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaClassifierIdentity {
    model: String,
    provider_type: String,
    model_version: String,
    prompt_version: String,
    configuration_digest: String,
    run_id: String,
    completed_at: String,
}
impl GigaClassifierIdentity {
    pub fn new(
        model: String,
        provider_type: String,
        model_version: String,
        prompt_version: String,
        configuration_digest: String,
        run_id: String,
        completed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            model: giga_nonempty("model", model)?,
            provider_type: giga_nonempty("provider_type", provider_type)?,
            model_version: giga_nonempty("model_version", model_version)?,
            prompt_version: giga_nonempty("prompt_version", prompt_version)?,
            configuration_digest: giga_hash("configuration_digest", configuration_digest)?,
            run_id: giga_nonempty("run_id", run_id)?,
            completed_at: giga_rfc3339("completed_at", completed_at)?,
        })
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn provider_type(&self) -> &str {
        &self.provider_type
    }
    pub fn model_version(&self) -> &str {
        &self.model_version
    }
    pub fn prompt_version(&self) -> &str {
        &self.prompt_version
    }
    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn completed_at(&self) -> &str {
        &self.completed_at
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaAuthority {
    PointerOnly,
}
impl GigaAuthority {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pointer-only" => Ok(Self::PointerOnly),
            other => Err(DomainError::UnknownGigaValue {
                field: "authority".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        "pointer-only"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaReviewState {
    Unreviewed,
    InReview,
    Promoted,
    Merged,
    Corrected,
    Dismissed,
    Unresolved,
    Curio,
    Expired,
    Superseded,
}
impl GigaReviewState {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "unreviewed" => Ok(Self::Unreviewed),
            "in_review" => Ok(Self::InReview),
            "promoted" => Ok(Self::Promoted),
            "merged" => Ok(Self::Merged),
            "corrected" => Ok(Self::Corrected),
            "dismissed" => Ok(Self::Dismissed),
            "unresolved" => Ok(Self::Unresolved),
            "curio" => Ok(Self::Curio),
            "expired" => Ok(Self::Expired),
            "superseded" => Ok(Self::Superseded),
            other => Err(DomainError::UnknownGigaValue {
                field: "review_state".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::InReview => "in_review",
            Self::Promoted => "promoted",
            Self::Merged => "merged",
            Self::Corrected => "corrected",
            Self::Dismissed => "dismissed",
            Self::Unresolved => "unresolved",
            Self::Curio => "curio",
            Self::Expired => "expired",
            Self::Superseded => "superseded",
        }
    }
    pub const fn can_transition(self, to: Self) -> bool {
        matches!(
            (self, to),
            (
                Self::Unreviewed,
                Self::InReview | Self::Dismissed | Self::Expired
            ) | (
                Self::InReview,
                Self::Promoted
                    | Self::Merged
                    | Self::Corrected
                    | Self::Dismissed
                    | Self::Unresolved
                    | Self::Curio
            ) | (Self::Unresolved, Self::InReview)
                | (
                    Self::Curio,
                    Self::InReview | Self::Dismissed | Self::Expired | Self::Superseded
                )
                | (
                    Self::Promoted | Self::Merged | Self::Corrected,
                    Self::Superseded
                )
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GigaCandidate {
    candidate_schema_version: u8,
    candidate_id: String,
    event_id: String,
    room: RoomKey,
    session_id: String,
    kind: GigaCandidateKind,
    source_refs: Vec<GigaSourceRef>,
    proof_refs: Vec<String>,
    scores: GigaScores,
    project_keys: Vec<String>,
    thread_keys: Vec<String>,
    entity_hints: Vec<String>,
    retrieval_terms: Vec<String>,
    proposed_title: String,
    gist: String,
    rationale: String,
    scope: GigaScope,
    authority: GigaAuthority,
    review_state: GigaReviewState,
    classifier: GigaClassifierIdentity,
    created_at: String,
    expires_at: Option<String>,
    promotion_refs: Vec<String>,
}
impl GigaCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: String,
        event_id: String,
        room: RoomKey,
        session_id: String,
        kind: GigaCandidateKind,
        source_refs: Vec<GigaSourceRef>,
        proof_refs: Vec<String>,
        scores: GigaScores,
        project_keys: Vec<String>,
        thread_keys: Vec<String>,
        entity_hints: Vec<String>,
        retrieval_terms: Vec<String>,
        proposed_title: String,
        gist: String,
        rationale: String,
        scope: GigaScope,
        authority: GigaAuthority,
        review_state: GigaReviewState,
        classifier: GigaClassifierIdentity,
        created_at: String,
        expires_at: Option<String>,
        promotion_refs: Vec<String>,
    ) -> Result<Self, DomainError> {
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "must not be empty".into(),
            });
        }
        if authority != GigaAuthority::PointerOnly {
            return Err(DomainError::GigaPointerOnly);
        }
        if review_state != GigaReviewState::Unreviewed || !promotion_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "review_state".into(),
                message: "new candidates must be unreviewed with no promotion refs".into(),
            });
        }
        let proof_refs = giga_strings("proof_refs", proof_refs)?;
        if kind.requires_proof() && proof_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "proof_refs".into(),
                message: "required for this candidate kind".into(),
            });
        }
        for proof in &proof_refs {
            if !source_refs.iter().any(|source| source.source_id() == proof) {
                return Err(DomainError::GigaProofNotSource);
            }
        }
        if (scope.visibility() == GigaVisibility::Private && scope.room() != Some(&room))
            || (scope.visibility() == GigaVisibility::Shared && scope.room().is_some())
        {
            return Err(DomainError::GigaScopeViolation);
        }
        let mut source_project: Option<&str> = None;
        let all_shared = source_refs
            .iter()
            .all(|source| source.scope().visibility() == GigaVisibility::Shared);
        let requires_review = source_refs
            .iter()
            .any(|source| source.scope().publication_review_required());
        for source in &source_refs {
            if source.scope().visibility() == GigaVisibility::Private
                && source.scope().room() != Some(&room)
            {
                return Err(DomainError::GigaScopeViolation);
            }
            if let Some(project) = source.scope().project() {
                if source_project.is_some_and(|known| known != project) {
                    return Err(DomainError::GigaScopeViolation);
                }
                source_project = Some(project);
            }
        }
        if scope.visibility() == GigaVisibility::Shared && !all_shared {
            return Err(DomainError::GigaScopeViolation);
        }
        if requires_review && !scope.publication_review_required() {
            return Err(DomainError::GigaScopeViolation);
        }
        let project_keys = giga_strings("project_keys", project_keys)?;
        if let Some(project) = scope.project() {
            if !project_keys.iter().any(|key| key == project) {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        if let Some(project) = source_project {
            if scope.project() != Some(project) {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        if kind == GigaCandidateKind::ProjectLesson
            && (project_keys.len() != 1 || scope.project() != Some(project_keys[0].as_str()))
        {
            return Err(DomainError::InvalidGiga {
                field: "project_keys".into(),
                message: "project_lesson requires one explicit matching project".into(),
            });
        }
        if kind == GigaCandidateKind::EntityUpdate && entity_hints.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "entity_hints".into(),
                message: "entity_update requires an explicit entity identity".into(),
            });
        }
        if kind == GigaCandidateKind::ThreadUpdate && thread_keys.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "thread_keys".into(),
                message: "thread_update requires an explicit thread key".into(),
            });
        }
        Ok(Self {
            candidate_schema_version: 1,
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            event_id: giga_nonempty("event_id", event_id)?,
            room,
            session_id: giga_nonempty("session_id", session_id)?,
            kind,
            source_refs,
            proof_refs,
            scores,
            project_keys,
            thread_keys: giga_strings("thread_keys", thread_keys)?,
            entity_hints: giga_strings("entity_hints", entity_hints)?,
            retrieval_terms: giga_strings("retrieval_terms", retrieval_terms)?,
            proposed_title,
            gist,
            rationale,
            scope,
            authority,
            review_state,
            classifier,
            created_at: giga_rfc3339("created_at", created_at)?,
            expires_at: expires_at
                .map(|value| giga_rfc3339("expires_at", value))
                .transpose()?,
            promotion_refs,
        })
    }
    pub const fn candidate_schema_version(&self) -> u8 {
        self.candidate_schema_version
    }
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub const fn kind(&self) -> GigaCandidateKind {
        self.kind
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn proof_refs(&self) -> &[String] {
        &self.proof_refs
    }
    pub const fn scores(&self) -> GigaScores {
        self.scores
    }
    pub fn project_keys(&self) -> &[String] {
        &self.project_keys
    }
    pub fn thread_keys(&self) -> &[String] {
        &self.thread_keys
    }
    pub fn entity_hints(&self) -> &[String] {
        &self.entity_hints
    }
    pub fn retrieval_terms(&self) -> &[String] {
        &self.retrieval_terms
    }
    pub fn proposed_title(&self) -> &str {
        &self.proposed_title
    }
    pub fn gist(&self) -> &str {
        &self.gist
    }
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
    pub fn scope(&self) -> &GigaScope {
        &self.scope
    }
    pub const fn authority(&self) -> GigaAuthority {
        self.authority
    }
    pub const fn review_state(&self) -> GigaReviewState {
        self.review_state
    }
    pub fn classifier(&self) -> &GigaClassifierIdentity {
        &self.classifier
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }
    pub fn promotion_refs(&self) -> &[String] {
        &self.promotion_refs
    }
}
