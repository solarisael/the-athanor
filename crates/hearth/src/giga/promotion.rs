use crate::error::DomainError;
use crate::remember::{MAX_ARRAY_VALUES, normalize_eligibility_keys};
use crate::room::RoomKey;

use super::candidate::{GigaCandidateKind, GigaReviewState};
use super::source::{
    GigaScope, GigaSourceRef, GigaVisibility, giga_nonempty, giga_rfc3339, giga_strings,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaPromotionKind {
    Memory,
    CodingLesson,
    ProjectLesson,
}

impl GigaPromotionKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "memory" => Ok(Self::Memory),
            "coding_lesson" => Ok(Self::CodingLesson),
            "project_lesson" => Ok(Self::ProjectLesson),
            other => Err(DomainError::UnknownGigaValue {
                field: "durable_kind".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::CodingLesson => "coding_lesson",
            Self::ProjectLesson => "project_lesson",
        }
    }

    pub const fn accepts(self, candidate_kind: GigaCandidateKind) -> bool {
        matches!(
            (self, candidate_kind),
            (Self::Memory, GigaCandidateKind::Memory)
                | (Self::CodingLesson, GigaCandidateKind::CodingLesson)
                | (Self::ProjectLesson, GigaCandidateKind::ProjectLesson)
        )
    }
}

fn giga_edited_text(field: &str, value: String) -> Result<String, DomainError> {
    giga_nonempty(field, value)
}

fn giga_optional_edited_text(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, DomainError> {
    value
        .map(|value| giga_edited_text(field, value))
        .transpose()
}

fn giga_promotion_strings(field: &str, values: Vec<String>) -> Result<Vec<String>, DomainError> {
    if values.len() > MAX_ARRAY_VALUES {
        return Err(DomainError::InvalidGiga {
            field: field.into(),
            message: format!("must contain at most {MAX_ARRAY_VALUES} values"),
        });
    }
    giga_strings(field, values)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaMemoryPromotionPayload {
    title: String,
    body: String,
    threads: Vec<String>,
}

impl GigaMemoryPromotionPayload {
    pub fn new(title: String, body: String, threads: Vec<String>) -> Result<Self, DomainError> {
        Ok(Self {
            title: giga_edited_text("target.payload.title", title)?,
            body: giga_edited_text("target.payload.body", body)?,
            threads: giga_promotion_strings("target.payload.threads", threads)?,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn threads(&self) -> &[String] {
        &self.threads
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaCodingLessonPromotionPayload {
    title: String,
    body: String,
    shape: Option<String>,
    proof_pattern: String,
    trigger_context: String,
    language_keys: Vec<String>,
    technology_keys: Vec<String>,
    thread_keys: Vec<String>,
    tags: Vec<String>,
}

impl GigaCodingLessonPromotionPayload {
    pub fn new(
        title: String,
        body: String,
        shape: Option<String>,
        proof_pattern: String,
        trigger_context: String,
        language_keys: Vec<String>,
        technology_keys: Vec<String>,
        thread_keys: Vec<String>,
        tags: Vec<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            title: giga_edited_text("target.payload.title", title)?,
            body: giga_edited_text("target.payload.body", body)?,
            shape: giga_optional_edited_text("target.payload.shape", shape)?,
            proof_pattern: giga_edited_text("target.payload.proof_pattern", proof_pattern)?,
            trigger_context: giga_edited_text("target.payload.trigger_context", trigger_context)?,
            language_keys: normalize_eligibility_keys(
                "target.payload.language_keys",
                language_keys,
            )?,
            technology_keys: normalize_eligibility_keys(
                "target.payload.technology_keys",
                technology_keys,
            )?,
            thread_keys: normalize_eligibility_keys("target.payload.thread_keys", thread_keys)?,
            tags: giga_promotion_strings("target.payload.tags", tags)?,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn shape(&self) -> Option<&str> {
        self.shape.as_deref()
    }
    pub fn proof_pattern(&self) -> &str {
        &self.proof_pattern
    }
    pub fn trigger_context(&self) -> &str {
        &self.trigger_context
    }
    pub fn language_keys(&self) -> &[String] {
        &self.language_keys
    }
    pub fn technology_keys(&self) -> &[String] {
        &self.technology_keys
    }
    pub fn thread_keys(&self) -> &[String] {
        &self.thread_keys
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaProjectLessonPromotionPayload {
    title: String,
    body: String,
    project: String,
    proof_pattern: String,
    trigger_context: String,
    language_keys: Vec<String>,
    technology_keys: Vec<String>,
    thread_keys: Vec<String>,
    tags: Vec<String>,
}

impl GigaProjectLessonPromotionPayload {
    pub fn new(
        title: String,
        body: String,
        project: String,
        proof_pattern: String,
        trigger_context: String,
        language_keys: Vec<String>,
        technology_keys: Vec<String>,
        thread_keys: Vec<String>,
        tags: Vec<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            title: giga_edited_text("target.payload.title", title)?,
            body: giga_edited_text("target.payload.body", body)?,
            project: giga_edited_text("target.payload.project", project)?,
            proof_pattern: giga_edited_text("target.payload.proof_pattern", proof_pattern)?,
            trigger_context: giga_edited_text("target.payload.trigger_context", trigger_context)?,
            language_keys: normalize_eligibility_keys(
                "target.payload.language_keys",
                language_keys,
            )?,
            technology_keys: normalize_eligibility_keys(
                "target.payload.technology_keys",
                technology_keys,
            )?,
            thread_keys: normalize_eligibility_keys("target.payload.thread_keys", thread_keys)?,
            tags: giga_promotion_strings("target.payload.tags", tags)?,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn project(&self) -> &str {
        &self.project
    }
    pub fn proof_pattern(&self) -> &str {
        &self.proof_pattern
    }
    pub fn trigger_context(&self) -> &str {
        &self.trigger_context
    }
    pub fn language_keys(&self) -> &[String] {
        &self.language_keys
    }
    pub fn technology_keys(&self) -> &[String] {
        &self.technology_keys
    }
    pub fn thread_keys(&self) -> &[String] {
        &self.thread_keys
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GigaPromotionPayload {
    Memory(GigaMemoryPromotionPayload),
    CodingLesson(GigaCodingLessonPromotionPayload),
    ProjectLesson {
        payload: GigaProjectLessonPromotionPayload,
        publication_consent: GigaPublicationConsent,
    },
}

impl GigaPromotionPayload {
    pub const fn kind(&self) -> GigaPromotionKind {
        match self {
            Self::Memory(_) => GigaPromotionKind::Memory,
            Self::CodingLesson(_) => GigaPromotionKind::CodingLesson,
            Self::ProjectLesson { .. } => GigaPromotionKind::ProjectLesson,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GigaPublicationConsent(());

impl GigaPublicationConsent {
    pub fn new(operator_approved: bool) -> Result<Self, DomainError> {
        if !operator_approved {
            return Err(DomainError::InvalidGiga {
                field: "publication_consent".into(),
                message: "project publication requires operator approval".into(),
            });
        }
        Ok(Self(()))
    }
}

fn giga_exact_source_set(left: &[GigaSourceRef], right: &[GigaSourceRef]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .enumerate()
            .all(|(index, source)| !left[..index].contains(source) && right.contains(source))
        && right
            .iter()
            .enumerate()
            .all(|(index, source)| !right[..index].contains(source) && left.contains(source))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaPromotionRequest {
    candidate_id: String,
    room: RoomKey,
    reviewer_id: String,
    operator_identity: String,
    authorization_basis: String,
    source_refs: Vec<GigaSourceRef>,
    payload: GigaPromotionPayload,
    reviewed_at: String,
}

impl GigaPromotionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: String,
        room: RoomKey,
        reviewer_id: String,
        operator_identity: String,
        authorization_basis: String,
        source_refs: Vec<GigaSourceRef>,
        payload: GigaPromotionPayload,
        reviewed_at: String,
    ) -> Result<Self, DomainError> {
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "promotion must retain exact sources".into(),
            });
        }
        for (index, source) in source_refs.iter().enumerate() {
            if source_refs[..index]
                .iter()
                .any(|known| known.source_id() == source.source_id())
            {
                return Err(DomainError::InvalidGiga {
                    field: "source_refs".into(),
                    message: "source IDs must be unique".into(),
                });
            }
            if source.scope().visibility() == GigaVisibility::Private
                && source.scope().room() != Some(&room)
            {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        Ok(Self {
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            room,
            reviewer_id: giga_nonempty("reviewer_id", reviewer_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            authorization_basis: giga_nonempty("authorization_basis", authorization_basis)?,
            source_refs,
            payload,
            reviewed_at: giga_rfc3339("reviewed_at", reviewed_at)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn validate_candidate(
        &self,
        candidate_id: &str,
        room: &RoomKey,
        kind: GigaCandidateKind,
        review_state: GigaReviewState,
        source_refs: &[GigaSourceRef],
        project_keys: &[String],
        scope: &GigaScope,
    ) -> Result<(), DomainError> {
        if candidate_id != self.candidate_id {
            return Err(DomainError::InvalidGiga {
                field: "candidate_id".into(),
                message: "does not match the locked candidate".into(),
            });
        }
        if room != &self.room {
            return Err(DomainError::GigaScopeViolation);
        }
        if review_state != GigaReviewState::InReview {
            return Err(DomainError::InvalidGiga {
                field: "review_state".into(),
                message: "promotion requires an in_review candidate".into(),
            });
        }
        if !self.payload.kind().accepts(kind) {
            return Err(DomainError::InvalidGiga {
                field: "target.kind".into(),
                message: "does not match candidate kind or is not promotable".into(),
            });
        }
        if !giga_exact_source_set(&self.source_refs, source_refs) {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "must exactly match the locked candidate source set and hashes".into(),
            });
        }
        match &self.payload {
            GigaPromotionPayload::Memory(_) | GigaPromotionPayload::CodingLesson(_) => {
                if scope.visibility() != GigaVisibility::Private || scope.room() != Some(&self.room)
                {
                    return Err(DomainError::GigaScopeViolation);
                }
            }
            GigaPromotionPayload::ProjectLesson { payload, .. } => {
                if project_keys.len() != 1
                    || project_keys[0] != payload.project
                    || scope.project() != Some(payload.project())
                {
                    return Err(DomainError::GigaScopeViolation);
                }
            }
        }
        Ok(())
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn reviewer_id(&self) -> &str {
        &self.reviewer_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.operator_identity
    }
    pub fn authorization_basis(&self) -> &str {
        &self.authorization_basis
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn payload(&self) -> &GigaPromotionPayload {
        &self.payload
    }
    pub fn reviewed_at(&self) -> &str {
        &self.reviewed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaPromotionAuthority {
    Full,
}

impl GigaPromotionAuthority {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "full" => Ok(Self::Full),
            other => Err(DomainError::UnknownGigaValue {
                field: "promotion_authority".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        "full"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GigaPromotionReceiptCommon {
    candidate_id: String,
    durable_id: u64,
    reviewer_id: String,
    operator_identity: String,
    reviewed_at: String,
    committed_at: String,
}

impl GigaPromotionReceiptCommon {
    fn new(
        candidate_id: String,
        durable_id: u64,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        if durable_id == 0 {
            return Err(DomainError::InvalidGiga {
                field: "durable_id".into(),
                message: "must be positive".into(),
            });
        }
        Ok(Self {
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            durable_id,
            reviewer_id: giga_nonempty("reviewer_id", reviewer_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            reviewed_at: giga_rfc3339("reviewed_at", reviewed_at)?,
            committed_at: giga_rfc3339("committed_at", committed_at)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaMemoryPromotionReceipt {
    common: GigaPromotionReceiptCommon,
    room: RoomKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaCodingLessonPromotionReceipt {
    common: GigaPromotionReceiptCommon,
    scope: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaProjectLessonPromotionReceipt {
    common: GigaPromotionReceiptCommon,
    project: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GigaPromotionReceipt {
    Memory(GigaMemoryPromotionReceipt),
    CodingLesson(GigaCodingLessonPromotionReceipt),
    ProjectLesson(GigaProjectLessonPromotionReceipt),
}

impl GigaPromotionReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn memory(
        candidate_id: String,
        memory_id: u64,
        room: RoomKey,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self::Memory(GigaMemoryPromotionReceipt {
            common: GigaPromotionReceiptCommon::new(
                candidate_id,
                memory_id,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            )?,
            room,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn coding_lesson(
        candidate_id: String,
        coding_lesson_id: u64,
        scope: String,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self::CodingLesson(GigaCodingLessonPromotionReceipt {
            common: GigaPromotionReceiptCommon::new(
                candidate_id,
                coding_lesson_id,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            )?,
            scope: giga_nonempty("scope", scope)?,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn project_lesson(
        candidate_id: String,
        project_lesson_id: u64,
        project: String,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self::ProjectLesson(GigaProjectLessonPromotionReceipt {
            common: GigaPromotionReceiptCommon::new(
                candidate_id,
                project_lesson_id,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            )?,
            project: giga_nonempty("project", project)?,
        }))
    }

    fn common(&self) -> &GigaPromotionReceiptCommon {
        match self {
            Self::Memory(receipt) => &receipt.common,
            Self::CodingLesson(receipt) => &receipt.common,
            Self::ProjectLesson(receipt) => &receipt.common,
        }
    }

    pub fn candidate_id(&self) -> &str {
        &self.common().candidate_id
    }
    pub const fn review_state(&self) -> GigaReviewState {
        GigaReviewState::Promoted
    }
    pub const fn durable_kind(&self) -> GigaPromotionKind {
        match self {
            Self::Memory(_) => GigaPromotionKind::Memory,
            Self::CodingLesson(_) => GigaPromotionKind::CodingLesson,
            Self::ProjectLesson(_) => GigaPromotionKind::ProjectLesson,
        }
    }
    pub fn durable_id(&self) -> u64 {
        self.common().durable_id
    }
    pub const fn durable(&self) -> bool {
        true
    }
    pub const fn authority(&self) -> GigaPromotionAuthority {
        GigaPromotionAuthority::Full
    }
    pub fn reviewer_id(&self) -> &str {
        &self.common().reviewer_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.common().operator_identity
    }
    pub fn reviewed_at(&self) -> &str {
        &self.common().reviewed_at
    }
    pub fn committed_at(&self) -> &str {
        &self.common().committed_at
    }
}

impl GigaMemoryPromotionReceipt {
    pub fn memory_id(&self) -> u64 {
        self.common.durable_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
}

impl GigaCodingLessonPromotionReceipt {
    pub fn coding_lesson_id(&self) -> u64 {
        self.common.durable_id
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

impl GigaProjectLessonPromotionReceipt {
    pub fn project_lesson_id(&self) -> u64 {
        self.common.durable_id
    }
    pub fn project(&self) -> &str {
        &self.project
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::giga::fixtures::{giga_private_scope, giga_project_scope, giga_test_source};

    #[test]
    fn giga_memory_coding_and_project_promotions_validate_their_candidates() {
        let room = RoomKey::new("lab").unwrap();
        let private_scope = giga_private_scope();
        let private_source = giga_test_source("turn-1", 'a', private_scope.clone());

        let memory = GigaPromotionRequest::new(
            "candidate-memory".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "reviewed exact source".into(),
            vec![private_source.clone()],
            GigaPromotionPayload::Memory(
                GigaMemoryPromotionPayload::new(
                    "Edited memory".into(),
                    "Durable human-edited body".into(),
                    vec!["consent".into()],
                )
                .unwrap(),
            ),
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        memory
            .validate_candidate(
                "candidate-memory",
                &room,
                GigaCandidateKind::Memory,
                GigaReviewState::InReview,
                &[private_source.clone()],
                &[],
                &private_scope,
            )
            .unwrap();

        let coding = GigaPromotionRequest::new(
            "candidate-coding".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "proof reviewed".into(),
            vec![private_source.clone()],
            GigaPromotionPayload::CodingLesson(
                GigaCodingLessonPromotionPayload::new(
                    "Sanitize inherited state".into(),
                    "Clear inherited variables before invoking tools.".into(),
                    Some("process".into()),
                    "failure then passing proof".into(),
                    "inherited environment state reaches a child tool process".into(),
                    vec!["rust".into()],
                    vec![],
                    vec!["subagent-dispatch".into()],
                    vec!["environment".into()],
                )
                .unwrap(),
            ),
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        coding
            .validate_candidate(
                "candidate-coding",
                &room,
                GigaCandidateKind::CodingLesson,
                GigaReviewState::InReview,
                &[private_source],
                &[],
                &private_scope,
            )
            .unwrap();

        let project_scope = giga_project_scope();
        let project_source = giga_test_source("turn-2", 'b', project_scope.clone());
        let project = GigaPromotionRequest::new(
            "candidate-project".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "operator approved publication".into(),
            vec![project_source.clone()],
            GigaPromotionPayload::ProjectLesson {
                payload: GigaProjectLessonPromotionPayload::new(
                    "Stable Athanor rule".into(),
                    "Keep queue mutations transactional.".into(),
                    "athanor".into(),
                    "rollback observed".into(),
                    "queue work crosses a durable transaction boundary".into(),
                    vec![],
                    vec!["postgresql".into()],
                    vec!["subagent-dispatch".into()],
                    vec!["queue".into()],
                )
                .unwrap(),
                publication_consent: GigaPublicationConsent::new(true).unwrap(),
            },
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        project
            .validate_candidate(
                "candidate-project",
                &room,
                GigaCandidateKind::ProjectLesson,
                GigaReviewState::InReview,
                &[project_source],
                &["athanor".into()],
                &project_scope,
            )
            .unwrap();
    }

    #[test]
    fn giga_promotion_requires_exact_source_refs_and_matching_target_kind() {
        let room = RoomKey::new("lab").unwrap();
        let scope = giga_private_scope();
        let first = giga_test_source("turn-1", 'a', scope.clone());
        let second = giga_test_source("turn-2", 'b', scope.clone());
        let request = GigaPromotionRequest::new(
            "candidate-memory".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "reviewed exact sources".into(),
            vec![first.clone(), second.clone()],
            GigaPromotionPayload::Memory(
                GigaMemoryPromotionPayload::new(
                    "Edited title".into(),
                    "Edited body".into(),
                    vec![],
                )
                .unwrap(),
            ),
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();

        request
            .validate_candidate(
                "candidate-memory",
                &room,
                GigaCandidateKind::Memory,
                GigaReviewState::InReview,
                &[second.clone(), first.clone()],
                &[],
                &scope,
            )
            .unwrap();
        let rehashed = giga_test_source("turn-2", 'c', scope.clone());
        for candidate_sources in [vec![first.clone()], vec![first.clone(), rehashed]] {
            assert!(matches!(
                request.validate_candidate(
                    "candidate-memory",
                    &room,
                    GigaCandidateKind::Memory,
                    GigaReviewState::InReview,
                    &candidate_sources,
                    &[],
                    &scope,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "source_refs"
            ));
        }

        for kind in [
            GigaCandidateKind::CodingLesson,
            GigaCandidateKind::ProjectLesson,
            GigaCandidateKind::Correction,
            GigaCandidateKind::Supersession,
            GigaCandidateKind::EntityUpdate,
            GigaCandidateKind::ThreadUpdate,
        ] {
            assert!(matches!(
                request.validate_candidate(
                    "candidate-memory",
                    &room,
                    kind,
                    GigaReviewState::InReview,
                    &[first.clone(), second.clone()],
                    &[],
                    &scope,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "target.kind"
            ));
        }

        let cross_room_source = giga_test_source(
            "turn-3",
            'd',
            GigaScope::new(
                Some("other-room".into()),
                None,
                GigaVisibility::Private,
                false,
            )
            .unwrap(),
        );
        assert_eq!(
            GigaPromotionRequest::new(
                "candidate-memory".into(),
                room,
                "kintsu".into(),
                "sol".into(),
                "reviewed exact source".into(),
                vec![cross_room_source],
                GigaPromotionPayload::Memory(
                    GigaMemoryPromotionPayload::new(
                        "Edited title".into(),
                        "Edited body".into(),
                        vec![],
                    )
                    .unwrap(),
                ),
                "2026-07-24T12:04:00Z".into(),
            ),
            Err(DomainError::GigaScopeViolation)
        );
    }

    #[test]
    fn giga_project_promotion_requires_operator_consent_and_exact_project_scope() {
        assert!(matches!(
            GigaPublicationConsent::new(false),
            Err(DomainError::InvalidGiga { field, .. }) if field == "publication_consent"
        ));
        let consent = GigaPublicationConsent::new(true).unwrap();

        let room = RoomKey::new("lab").unwrap();
        let scope = giga_project_scope();
        let source = giga_test_source("turn-1", 'a', scope.clone());
        let payload = || GigaPromotionPayload::ProjectLesson {
            payload: GigaProjectLessonPromotionPayload::new(
                "Edited title".into(),
                "Edited body".into(),
                "athanor".into(),
                "transaction rollback preserves the prior durable state".into(),
                "a project rule changes coupled database writes".into(),
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .unwrap(),
            publication_consent: consent,
        };

        let request = GigaPromotionRequest::new(
            "candidate-project".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "publication reviewed".into(),
            vec![source.clone()],
            payload(),
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        for projects in [
            Vec::new(),
            vec!["other-project".into()],
            vec!["athanor".into(), "other-project".into()],
        ] {
            assert_eq!(
                request.validate_candidate(
                    "candidate-project",
                    &room,
                    GigaCandidateKind::ProjectLesson,
                    GigaReviewState::InReview,
                    &[source.clone()],
                    &projects,
                    &scope,
                ),
                Err(DomainError::GigaScopeViolation)
            );
        }
    }

    #[test]
    fn giga_promotion_payloads_require_human_edited_durable_fields() {
        assert!(GigaMemoryPromotionPayload::new(" ".into(), "body".into(), vec![]).is_err());
        assert!(GigaMemoryPromotionPayload::new("title".into(), "\n".into(), vec![]).is_err());
        assert!(
            GigaCodingLessonPromotionPayload::new(
                "".into(),
                "body".into(),
                None,
                "proof".into(),
                "trigger".into(),
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .is_err()
        );
        assert!(
            GigaCodingLessonPromotionPayload::new(
                "title".into(),
                "body".into(),
                Some(" ".into()),
                "proof".into(),
                "trigger".into(),
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .is_err()
        );
        for (proof_pattern, trigger_context) in [(" ", "trigger context"), ("proof pattern", "\n")]
        {
            assert!(
                GigaCodingLessonPromotionPayload::new(
                    "title".into(),
                    "body".into(),
                    None,
                    proof_pattern.into(),
                    trigger_context.into(),
                    vec![],
                    vec![],
                    vec![],
                    vec![],
                )
                .is_err()
            );
        }
        assert!(
            GigaProjectLessonPromotionPayload::new(
                "title".into(),
                "body".into(),
                " ".into(),
                "proof".into(),
                "trigger".into(),
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .is_err()
        );
        for (proof_pattern, trigger_context) in [("", "trigger context"), ("proof pattern", " ")] {
            assert!(
                GigaProjectLessonPromotionPayload::new(
                    "title".into(),
                    "body".into(),
                    "project".into(),
                    proof_pattern.into(),
                    trigger_context.into(),
                    vec![],
                    vec![],
                    vec![],
                    vec![],
                )
                .is_err()
            );
        }
    }

    #[test]
    fn giga_promotion_receipts_are_exhaustive_and_require_positive_typed_ids() {
        let room = RoomKey::new("lab").unwrap();
        assert!(
            GigaPromotionReceipt::memory(
                "candidate-1".into(),
                0,
                room.clone(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .is_err()
        );
        assert!(matches!(
            GigaPromotionReceipt::memory(
                "candidate-1".into(),
                7,
                room,
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::Memory(_)
        ));
        assert!(matches!(
            GigaPromotionReceipt::coding_lesson(
                "candidate-2".into(),
                8,
                "lab".into(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::CodingLesson(_)
        ));
        assert!(matches!(
            GigaPromotionReceipt::project_lesson(
                "candidate-3".into(),
                9,
                "kintsu".into(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::ProjectLesson(_)
        ));
    }
}
