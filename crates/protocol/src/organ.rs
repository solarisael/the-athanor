use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganOperation {
    CanonWrite,
    CanonRead,
    Remember,
    PaperBoatSleep,
    PaperBoatWake,
    Recall,
    VaultRecall,
    Anamnesis,
    AnamnesisWrite,
    LessonQuery,
    LessonUpdate,
    LessonDelete,
    LessonTriggerMatch,
    LessonTriggerRecord,
    DesignDocumentQuery,
    DesignDocumentWrite,
    EntityResolve,
    HallwayCreate,
    HallwayJoin,
    HallwayPost,
    HallwayRead,
    HallwayInbox,
    HallwayKnockPolicy,
    HallwayKnock,
    QuestPost,
    QuestBoard,
    QuestClaim,
    QuestReport,
    QuestEvidence,
    RestartRequest,
    RestartTransition,
    RestartVerify,
    RestartStatus,
    GigaSetEnablement,
    GigaConversationIngest,
    GigaCandidateList,
    GigaToolReview,
    GigaToolPromote,
    GigaHealth,
    GigaQueueMaintenance,
    SubstrateHealth,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganTargetScope {
    #[default]
    Room,
    House,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrganRequest {
    pub operation: OrganOperation,
    pub params: Map<String, Value>,
    #[serde(default)]
    pub target_scope: OrganTargetScope,
}

impl OrganOperation {
    pub fn method(self) -> String {
        match self {
            Self::CanonWrite => "canon_write",
            Self::CanonRead => "canon_read",
            Self::Remember => "remember",
            Self::PaperBoatSleep => "paper_boat_sleep",
            Self::PaperBoatWake => "paper_boat_wake",
            Self::Recall => "recall",
            Self::VaultRecall => "vault_recall",
            Self::Anamnesis => "anamnesis",
            Self::AnamnesisWrite => "anamnesis_write",
            Self::LessonQuery => "lesson_query",
            Self::LessonUpdate => "lesson_update",
            Self::LessonDelete => "lesson_delete",
            Self::LessonTriggerMatch => "lesson_trigger_match",
            Self::LessonTriggerRecord => "lesson_trigger_record",
            Self::DesignDocumentQuery => "design_document_query",
            Self::DesignDocumentWrite => "design_document_write",
            Self::EntityResolve => "entity_resolve",
            Self::HallwayCreate => "hallway_create",
            Self::HallwayJoin => "hallway_join",
            Self::HallwayPost => "hallway_post",
            Self::HallwayRead => "hallway_read",
            Self::HallwayInbox => "hallway_inbox",
            Self::HallwayKnockPolicy => "hallway_knock_policy",
            Self::HallwayKnock => "hallway_knock",
            Self::QuestPost => "quest_post",
            Self::QuestBoard => "quest_board",
            Self::QuestClaim => "quest_claim",
            Self::QuestReport => "quest_report",
            Self::QuestEvidence => "quest_evidence",
            Self::RestartRequest => "restart_request",
            Self::RestartTransition => "restart_transition",
            Self::RestartVerify => "restart_verify",
            Self::RestartStatus => "restart_status",
            Self::GigaSetEnablement => "giga_set_enablement",
            Self::GigaConversationIngest => "giga_conversation_ingest",
            Self::GigaCandidateList => "giga_candidate_list",
            Self::GigaToolReview => "giga_tool_review",
            Self::GigaToolPromote => "giga_tool_promote",
            Self::GigaHealth => "giga_health",
            Self::GigaQueueMaintenance => "giga_queue_maintenance",
            Self::SubstrateHealth => "substrate_health",
        }
        .to_owned()
    }

    pub const fn is_read_only(self) -> bool {
        matches!(
            self,
            Self::CanonRead
                | Self::Recall
                | Self::VaultRecall
                | Self::Anamnesis
                | Self::LessonQuery
                | Self::LessonTriggerMatch
                | Self::DesignDocumentQuery
                | Self::EntityResolve
                | Self::HallwayInbox
                | Self::GigaCandidateList
                | Self::GigaHealth
                | Self::SubstrateHealth
        )
    }
}
