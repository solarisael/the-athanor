pub mod retention;
use crate::insula_writer::{EmitterSpan, system_binding};
use crate::migrations::{migration_pool, run_migrations};
use crate::{
    AppError, Config, DesignDocumentQueryParams, DesignDocumentWriteParams, EntityResolveParams,
    LessonContextParams, LessonDeleteParams, LessonQueryParams, LessonTriggerMatchParams,
    LessonTriggerRecordParams, LessonUpdateParams, OutcomeClass, QuestBoardParams,
    QuestChargebookParams, QuestClaimParams, QuestClockParams, QuestEvidenceParams,
    QuestPostParams, QuestReportParams, SubstrateHealthOptions, TrustedBinding, anamnesis,
    anamnesis_write, canon_read, canon_write, cluster_maintenance, design_document_query,
    design_document_write, entity_resolve, giga_candidate_list, giga_conversation_ingest,
    giga_event_claim, giga_event_finish, giga_event_ingest, giga_event_replay, giga_promote,
    giga_queue_maintenance, giga_review, giga_tool_promote, giga_tool_review, hallway_create,
    hallway_inbox, hallway_join, hallway_knock, hallway_knock_policy, hallway_read, lesson_context,
    lesson_delete, lesson_query, lesson_trigger_match, lesson_trigger_record, lesson_update,
    paper_boat_sleep, paper_boat_wake, quest_board, quest_chargebook, quest_claim, quest_clock,
    quest_evidence, quest_post, quest_report, recall, remember, restart_claim, restart_request,
    restart_status, restart_transition, restart_verify, validate_trusted_binding,
};
use hearth::{
    CanonReadRequest, CanonWriteRequest,
    ClusterMaintenanceRequest as DomainClusterMaintenanceRequest, GigaEvent, GigaEventClaimRequest,
    GigaEventFinishRequest, GigaEventReplayRequest, GigaPromotionRequest,
    GigaQueueMaintenanceRequest, GigaReviewAction, RecallRequest, RememberRequest,
    hallway::{
        HallwayCreateRequest, HallwayInboxRequest, HallwayJoinRequest, HallwayKnockPolicyRequest,
        HallwayKnockRequest, HallwayPostRequest, HallwayReadRequest,
    },
};
use protocol::restart::{
    RestartClaimParams, RestartRequestParams, RestartStatusParams, RestartTransitionParams,
    RestartVerifyParams,
};
use protocol::{
    ClusterMaintenanceResultWire, GigaCandidateListRequest, GigaConversationIngestParams,
    GigaEventClaimResult, GigaEventFinishResult, GigaEventReplayResult, GigaHealthRequest,
    GigaPromoteResult, GigaToolPromoteParams, GigaToolReviewParams, PROTOCOL_VERSION,
    PaperBoatSleepResult, PaperBoatWakeResult, ProtocolError, ProtocolErrorBody, RequestEnvelope,
    ResponseEnvelope, ResponsePayload, SubstrateHealthParams, SubstrateMigrationsParams,
    VaultRecallParams, success,
};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use summoning::{
    AnamnesisReadRequest, AnamnesisWriteRequest, PaperBoatSleepRequest, PaperBoatWakeRequest,
};
use vault::{VaultRecallRequest, recall as vault_recall};

#[derive(Debug)]
pub enum ProtocolRequest {
    CanonWrite(CanonWriteRequest),
    CanonRead(CanonReadRequest),
    Remember(RememberRequest),
    PaperBoatSleep(PaperBoatSleepRequest),
    PaperBoatWake(PaperBoatWakeRequest),
    HallwayCreate(HallwayCreateRequest),
    HallwayJoin(HallwayJoinRequest),
    HallwayPost(HallwayPostRequest),
    HallwayRead(HallwayReadRequest),
    HallwayInbox(HallwayInboxRequest),
    HallwayKnockPolicy(HallwayKnockPolicyRequest),
    HallwayKnock(HallwayKnockRequest),
    Recall(RecallRequest),
    VaultRecall(VaultRecallParams),
    Anamnesis(AnamnesisReadRequest),
    AnamnesisWrite(AnamnesisWriteRequest),
    LessonQuery(LessonQueryParams),
    QuestPost(QuestPostParams),
    QuestBoard(QuestBoardParams),
    QuestClaim(QuestClaimParams),
    QuestReport(QuestReportParams),
    QuestClock(QuestClockParams),
    QuestChargebook(QuestChargebookParams),
    QuestEvidence(QuestEvidenceParams),
    RestartRequest(RestartRequestParams),
    RestartClaim(RestartClaimParams),
    RestartTransition(RestartTransitionParams),
    RestartVerify(RestartVerifyParams),
    RestartStatus(RestartStatusParams),
    LessonContext(LessonContextParams),
    LessonUpdate(LessonUpdateParams),
    LessonDelete(LessonDeleteParams),
    LessonTriggerMatch(LessonTriggerMatchParams),
    LessonTriggerRecord(LessonTriggerRecordParams),
    DesignDocumentQuery(DesignDocumentQueryParams),
    DesignDocumentWrite(DesignDocumentWriteParams),
    EntityResolve(EntityResolveParams),
    Cluster(DomainClusterMaintenanceRequest),
    GigaEvent(GigaEvent),
    GigaConversationIngest(GigaConversationIngestParams),
    GigaEventClaim(GigaEventClaimRequest),
    GigaEventFinish(GigaEventFinishRequest),
    GigaEventReplay(GigaEventReplayRequest),
    GigaQueueMaintenance(GigaQueueMaintenanceRequest),
    GigaPromote(GigaPromotionRequest),
    GigaToolPromote(GigaToolPromoteParams),
    GigaCandidateList(GigaCandidateListRequest),
    GigaReview(GigaReviewAction),
    GigaToolReview(GigaToolReviewParams),
    GigaHealth(GigaHealthRequest),
    SubstrateHealth(SubstrateHealthParams),
    SubstrateMigrations(SubstrateMigrationsParams),
}

fn invalid_params(message: impl Into<String>) -> ProtocolError {
    ProtocolError::InvalidParams(message.into())
}

pub fn decode_line(line: &str) -> (String, Result<ProtocolRequest, ProtocolError>) {
    let envelope = match RequestEnvelope::parse_line(line) {
        Ok(envelope) => envelope,
        Err(error) => return ("unknown".into(), Err(error)),
    };
    decode_envelope(envelope)
}

pub fn decode_envelope(
    envelope: RequestEnvelope,
) -> (String, Result<ProtocolRequest, ProtocolError>) {
    let id = envelope.id.clone();
    if id.trim().is_empty() {
        return (
            id,
            Err(ProtocolError::Malformed("id must be non-empty".into())),
        );
    }
    if envelope.protocol != PROTOCOL_VERSION {
        return (id, Err(ProtocolError::ProtocolMismatch(envelope.protocol)));
    }
    let request = match envelope.method.as_str() {
        "canon_write" => envelope
            .canon_write_request()
            .map(ProtocolRequest::CanonWrite),
        "canon_read" => envelope
            .canon_read_request()
            .map(ProtocolRequest::CanonRead),
        "remember" => envelope.remember_request().map(ProtocolRequest::Remember),
        "paper_boat_sleep" => envelope
            .paper_boat_sleep_request()
            .map(ProtocolRequest::PaperBoatSleep),
        "paper_boat_wake" => envelope
            .paper_boat_wake_request()
            .map(ProtocolRequest::PaperBoatWake),
        "hallway_create" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayCreate)
            .map_err(|error| invalid_params(error.to_string())),
        "hallway_join" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayJoin)
            .map_err(|error| invalid_params(error.to_string())),
        "hallway_post" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayPost)
            .map_err(|error| invalid_params(error.to_string())),
        "hallway_read" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayRead)
            .map_err(|error| invalid_params(error.to_string())),
        "hallway_inbox" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayInbox)
            .map_err(|error| invalid_params(error.to_string())),
        "hallway_knock_policy" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayKnockPolicy)
            .map_err(|error| invalid_params(error.to_string())),
        "hallway_knock" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::HallwayKnock)
            .map_err(|error| invalid_params(error.to_string())),
        "recall" => envelope.recall_request().map(ProtocolRequest::Recall),
        "vault_recall" => envelope
            .vault_recall_request()
            .map(ProtocolRequest::VaultRecall),
        "anamnesis" => envelope.anamnesis_request().map(ProtocolRequest::Anamnesis),
        "anamnesis_write" => match envelope.params.get("operation").and_then(Value::as_str) {
            Some("add") => envelope
                .anamnesis_add_request()
                .map(AnamnesisWriteRequest::Add)
                .map(ProtocolRequest::AnamnesisWrite),
            Some("append-rep") => envelope
                .anamnesis_append_request()
                .map(AnamnesisWriteRequest::AppendRep)
                .map(ProtocolRequest::AnamnesisWrite),
            Some(operation) => Err(invalid_params(format!(
                "unsupported anamnesis_write operation: {operation}"
            ))),
            None => Err(invalid_params("anamnesis_write requires operation")),
        },
        "lesson_query" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonQuery)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_post" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestPost)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_board" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestBoard)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_claim" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestClaim)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_report" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestReport)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_clock" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestClock)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_chargebook" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestChargebook)
            .map_err(|error| invalid_params(error.to_string())),
        "quest_evidence" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::QuestEvidence)
            .map_err(|error| invalid_params(error.to_string())),
        "restart_request" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::RestartRequest)
            .map_err(|error| invalid_params(error.to_string())),
        "restart_claim" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::RestartClaim)
            .map_err(|error| invalid_params(error.to_string())),
        "restart_transition" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::RestartTransition)
            .map_err(|error| invalid_params(error.to_string())),
        "restart_verify" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::RestartVerify)
            .map_err(|error| invalid_params(error.to_string())),
        "restart_status" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::RestartStatus)
            .map_err(|error| invalid_params(error.to_string())),
        "lesson_context" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonContext)
            .map_err(|error| invalid_params(error.to_string())),
        "lesson_update" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonUpdate)
            .map_err(|error| invalid_params(error.to_string())),
        "lesson_delete" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonDelete)
            .map_err(|error| invalid_params(error.to_string())),
        "lesson_trigger_match" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonTriggerMatch)
            .map_err(|error| invalid_params(error.to_string())),
        "lesson_trigger_record" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonTriggerRecord)
            .map_err(|error| invalid_params(error.to_string())),
        "design_document_query" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::DesignDocumentQuery)
            .map_err(|error| invalid_params(error.to_string())),
        "design_document_write" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::DesignDocumentWrite)
            .map_err(|error| invalid_params(error.to_string())),
        "entity_resolve" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::EntityResolve)
            .map_err(|error| invalid_params(error.to_string())),
        "giga_event_ingest" => envelope
            .giga_event_ingest_request()
            .map(ProtocolRequest::GigaEvent),
        "giga_conversation_ingest" => envelope
            .giga_conversation_ingest_request()
            .map(ProtocolRequest::GigaConversationIngest),
        "giga_event_claim" => envelope
            .giga_event_claim_request()
            .map(ProtocolRequest::GigaEventClaim),
        "giga_event_finish" => envelope
            .giga_event_finish_request()
            .map(ProtocolRequest::GigaEventFinish),
        "giga_event_replay" => envelope
            .giga_event_replay_request()
            .map(ProtocolRequest::GigaEventReplay),
        "giga_queue_maintenance" => envelope
            .giga_queue_maintenance_request()
            .map(ProtocolRequest::GigaQueueMaintenance),
        "giga_promote" => envelope
            .giga_promote_request()
            .map(ProtocolRequest::GigaPromote),
        "giga_tool_promote" => envelope
            .giga_tool_promote_request()
            .map(ProtocolRequest::GigaToolPromote),
        "giga_candidate_list" => envelope
            .giga_candidate_list_request()
            .map(ProtocolRequest::GigaCandidateList),
        "giga_review" => envelope
            .giga_review_request()
            .map(ProtocolRequest::GigaReview),
        "giga_tool_review" => envelope
            .giga_tool_review_request()
            .map(ProtocolRequest::GigaToolReview),
        "giga_health" => envelope
            .giga_health_request()
            .map(ProtocolRequest::GigaHealth),
        "substrate_health" => envelope
            .substrate_health_request()
            .map(ProtocolRequest::SubstrateHealth),
        "substrate_migrations" => envelope
            .substrate_migrations_request()
            .map(ProtocolRequest::SubstrateMigrations),
        "cluster_maintenance" => envelope
            .cluster_maintenance_request()
            .map(ProtocolRequest::Cluster),
        method => Err(ProtocolError::UnknownMethod(method.into())),
    };
    (id, request)
}

fn error_json(id: String, error: ProtocolErrorBody) -> String {
    serde_json::to_string(&ResponseEnvelope::<Value> {
        protocol: PROTOCOL_VERSION,
        id,
        payload: ResponsePayload::Error { error },
    })
    .expect("protocol error serialization cannot fail")
}

/// A response carried together with the mechanical outcome that produced it,
/// so the dispatch loop can close one observation span at the single door
/// instead of at every handler arm.
pub struct Dispatched {
    pub json: String,
    pub outcome: OutcomeClass,
    pub error_class: Option<&'static str>,
}

pub const INSULA_COMPONENT: &str = "akasha";
pub const INSULA_LAYER: &str = "substrate";

// Error classes name the error variant and nothing else: no message, code, or
// caller text may reach an observation. `atom` in insula.rs also refuses
// anything but a lowercase mechanical name, so `type_name` is unusable here.
pub fn protocol_error_class(error: &ProtocolError) -> &'static str {
    match error {
        ProtocolError::Malformed(_) => "protocol_error.malformed",
        ProtocolError::ProtocolMismatch(_) => "protocol_error.protocol_mismatch",
        ProtocolError::UnknownMethod(_) => "protocol_error.unknown_method",
        ProtocolError::InvalidParams(_) => "protocol_error.invalid_params",
    }
}

/// The observed operation is the protocol method itself: one table, so a new
/// method cannot reach the dispatch loop without naming what it does.
pub fn operation_name(request: &ProtocolRequest) -> &'static str {
    match request {
        ProtocolRequest::CanonWrite(_) => "canon_write",
        ProtocolRequest::CanonRead(_) => "canon_read",
        ProtocolRequest::Remember(_) => "remember",
        ProtocolRequest::PaperBoatSleep(_) => "paper_boat_sleep",
        ProtocolRequest::PaperBoatWake(_) => "paper_boat_wake",
        ProtocolRequest::HallwayCreate(_) => "hallway_create",
        ProtocolRequest::HallwayJoin(_) => "hallway_join",
        ProtocolRequest::HallwayPost(_) => "hallway_post",
        ProtocolRequest::HallwayRead(_) => "hallway_read",
        ProtocolRequest::HallwayInbox(_) => "hallway_inbox",
        ProtocolRequest::HallwayKnockPolicy(_) => "hallway_knock_policy",
        ProtocolRequest::HallwayKnock(_) => "hallway_knock",
        ProtocolRequest::Recall(_) => "recall",
        ProtocolRequest::VaultRecall(_) => "vault_recall",
        ProtocolRequest::Anamnesis(_) => "anamnesis",
        ProtocolRequest::AnamnesisWrite(_) => "anamnesis_write",
        ProtocolRequest::LessonQuery(_) => "lesson_query",
        ProtocolRequest::QuestPost(_) => "quest_post",
        ProtocolRequest::QuestBoard(_) => "quest_board",
        ProtocolRequest::QuestClaim(_) => "quest_claim",
        ProtocolRequest::QuestReport(_) => "quest_report",
        ProtocolRequest::QuestClock(_) => "quest_clock",
        ProtocolRequest::QuestChargebook(_) => "quest_chargebook",
        ProtocolRequest::QuestEvidence(_) => "quest_evidence",
        ProtocolRequest::RestartRequest(_) => "restart_request",
        ProtocolRequest::RestartClaim(_) => "restart_claim",
        ProtocolRequest::RestartTransition(_) => "restart_transition",
        ProtocolRequest::RestartVerify(_) => "restart_verify",
        ProtocolRequest::RestartStatus(_) => "restart_status",
        ProtocolRequest::LessonContext(_) => "lesson_context",
        ProtocolRequest::LessonUpdate(_) => "lesson_update",
        ProtocolRequest::LessonDelete(_) => "lesson_delete",
        ProtocolRequest::LessonTriggerMatch(_) => "lesson_trigger_match",
        ProtocolRequest::LessonTriggerRecord(_) => "lesson_trigger_record",
        ProtocolRequest::DesignDocumentQuery(_) => "design_document_query",
        ProtocolRequest::DesignDocumentWrite(_) => "design_document_write",
        ProtocolRequest::EntityResolve(_) => "entity_resolve",
        ProtocolRequest::Cluster(_) => "cluster_maintenance",
        ProtocolRequest::GigaEvent(_) => "giga_event_ingest",
        ProtocolRequest::GigaConversationIngest(_) => "giga_conversation_ingest",
        ProtocolRequest::GigaEventClaim(_) => "giga_event_claim",
        ProtocolRequest::GigaEventFinish(_) => "giga_event_finish",
        ProtocolRequest::GigaEventReplay(_) => "giga_event_replay",
        ProtocolRequest::GigaQueueMaintenance(_) => "giga_queue_maintenance",
        ProtocolRequest::GigaPromote(_) => "giga_promote",
        ProtocolRequest::GigaToolPromote(_) => "giga_tool_promote",
        ProtocolRequest::GigaCandidateList(_) => "giga_candidate_list",
        ProtocolRequest::GigaReview(_) => "giga_review",
        ProtocolRequest::GigaToolReview(_) => "giga_tool_review",
        ProtocolRequest::GigaHealth(_) => "giga_health",
        ProtocolRequest::SubstrateHealth(_) => "substrate_health",
        ProtocolRequest::SubstrateMigrations(_) => "substrate_migrations",
    }
}

/// Hallway and Docket requests carry a whole authenticated room, spirit, and
/// session triple. Other methods use the House service voice.
pub fn insula_binding(request: &ProtocolRequest) -> TrustedBinding {
    let identity = match request {
        ProtocolRequest::HallwayCreate(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::HallwayJoin(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::HallwayPost(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::HallwayRead(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::HallwayInbox(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::HallwayKnockPolicy(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::HallwayKnock(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestPost(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestBoard(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestClaim(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestReport(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestClock(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestChargebook(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        ProtocolRequest::QuestEvidence(request) => {
            Some((&request.room, &request.spirit, &request.session))
        }
        // The restart plane binds only where a whole triple exists: the
        // requesting session names itself, and the successor proves itself by
        // room, spirit, and its new session. A keeper claim, a token-fenced
        // transition, and the anonymous status read have no spirit to bind, so
        // they stay in the House service voice.
        ProtocolRequest::RestartRequest(request) => Some((
            &request.requester_room,
            &request.requester_spirit,
            &request.requester_session,
        )),
        ProtocolRequest::RestartVerify(request) => {
            Some((&request.room, &request.spirit, &request.successor_session))
        }
        _ => None,
    };
    let Some((room, spirit, session)) = identity else {
        return system_binding();
    };
    let binding = TrustedBinding {
        room: room.clone(),
        spirit: spirit.clone(),
        session_id: session.clone(),
        ..system_binding()
    };
    // A caller-supplied triple is not yet an Insula binding. An invalid one
    // would be refused at ingest and lose the observation entirely, so it is
    // observed under the service voice instead.
    if validate_trusted_binding(&binding).is_ok() {
        binding
    } else {
        system_binding()
    }
}

pub fn protocol_error(id: String, error: ProtocolError) -> Dispatched {
    Dispatched {
        outcome: OutcomeClass::Refused,
        error_class: Some(protocol_error_class(&error)),
        json: error_json(id, error.into()),
    }
}

fn app_error(id: String, operation: &str, error: AppError) -> Dispatched {
    Dispatched {
        outcome: error.insula_outcome(),
        error_class: Some(error.insula_class()),
        json: error_json(id, error.protocol_error_body(operation)),
    }
}

fn success_json<T: Serialize>(id: String, result: T) -> Result<Dispatched, serde_json::Error> {
    Ok(Dispatched {
        json: serde_json::to_string(&success(id, result))?,
        outcome: OutcomeClass::Ok,
        error_class: None,
    })
}
/// Methods answered without the shared pool: they never bootstrap it.
pub fn needs_runtime(request: &ProtocolRequest) -> bool {
    !matches!(
        request,
        ProtocolRequest::VaultRecall(_)
            | ProtocolRequest::SubstrateHealth(_)
            | ProtocolRequest::SubstrateMigrations(_)
    )
}

#[derive(Default)]
pub struct NativeServices<'a> {
    pub health_config: Option<Result<Config, AppError>>,
    pub nats_auth: Option<Option<&'a origami::cranes::broker::NatsAuth>>,
    pub giga_enablement: Option<crate::GigaEnablement>,
}

pub async fn execute(
    id: String,
    request: ProtocolRequest,
    runtime: Option<(&Config, &sqlx::PgPool)>,
    bootstrap_error: Option<AppError>,
    span: Option<&EmitterSpan>,
    services: NativeServices<'_>,
) -> Result<Dispatched, serde_json::Error> {
    if matches!(request, ProtocolRequest::SubstrateMigrations(_)) {
        let operation = operation_name(&request);
        return Ok(match Config::from_env() {
            Ok(config) => match migration_pool(&config).await {
                Ok(pool) => match run_migrations(&pool).await {
                    Ok(result) => success_json(id, result)?,
                    Err(error) => app_error(id, operation, error),
                },
                Err(error) => app_error(id, operation, error),
            },
            Err(error) => app_error(id, operation, error),
        });
    }
    execute_service(id, request, runtime, bootstrap_error, span, services).await
}

pub async fn execute_service(
    id: String,
    request: ProtocolRequest,
    runtime: Option<(&Config, &sqlx::PgPool)>,
    bootstrap_error: Option<AppError>,
    span: Option<&EmitterSpan>,
    services: NativeServices<'_>,
) -> Result<Dispatched, serde_json::Error> {
    let operation = operation_name(&request);
    let validation = match &request {
        ProtocolRequest::CanonWrite(_) | ProtocolRequest::CanonRead(_) => Ok(()),
        ProtocolRequest::Remember(_) => Ok(()),
        ProtocolRequest::PaperBoatSleep(_) | ProtocolRequest::PaperBoatWake(_) => Ok(()),
        ProtocolRequest::HallwayCreate(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::HallwayJoin(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::HallwayPost(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::HallwayRead(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::HallwayInbox(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::HallwayKnockPolicy(request) => {
            request.validate().map_err(AppError::Invalid)
        }
        ProtocolRequest::HallwayKnock(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::Recall(_) => Ok(()),
        ProtocolRequest::VaultRecall(_) => Ok(()),
        ProtocolRequest::QuestPost(request) => request.validate(),
        ProtocolRequest::QuestBoard(request) => request.validate(),
        ProtocolRequest::QuestClaim(request) => request.validate(),
        ProtocolRequest::QuestReport(request) => request.validate(),
        ProtocolRequest::QuestClock(request) => request.validate(),
        ProtocolRequest::QuestChargebook(request) => request.validate(),
        ProtocolRequest::QuestEvidence(request) => request.validate(),
        ProtocolRequest::RestartRequest(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::RestartClaim(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::RestartTransition(request) => {
            request.validate().map_err(AppError::Invalid)
        }
        ProtocolRequest::RestartVerify(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::RestartStatus(request) => request.validate().map_err(AppError::Invalid),
        ProtocolRequest::Anamnesis(_)
        | ProtocolRequest::AnamnesisWrite(_)
        | ProtocolRequest::LessonQuery(_)
        | ProtocolRequest::LessonContext(_)
        | ProtocolRequest::LessonUpdate(_)
        | ProtocolRequest::LessonDelete(_)
        | ProtocolRequest::LessonTriggerMatch(_)
        | ProtocolRequest::LessonTriggerRecord(_)
        | ProtocolRequest::DesignDocumentQuery(_)
        | ProtocolRequest::DesignDocumentWrite(_)
        | ProtocolRequest::EntityResolve(_)
        | ProtocolRequest::Cluster(_)
        | ProtocolRequest::GigaEvent(_)
        | ProtocolRequest::GigaConversationIngest(_)
        | ProtocolRequest::GigaEventClaim(_)
        | ProtocolRequest::GigaEventFinish(_)
        | ProtocolRequest::GigaEventReplay(_)
        | ProtocolRequest::GigaQueueMaintenance(_)
        | ProtocolRequest::GigaPromote(_)
        | ProtocolRequest::GigaToolPromote(_)
        | ProtocolRequest::GigaCandidateList(_)
        | ProtocolRequest::GigaReview(_)
        | ProtocolRequest::GigaToolReview(_)
        | ProtocolRequest::GigaHealth(_)
        | ProtocolRequest::SubstrateHealth(_)
        | ProtocolRequest::SubstrateMigrations(_) => Ok(()),
    };
    let dispatched = if let Err(error) = validation {
        app_error(id, operation, error)
    } else {
        match request {
            ProtocolRequest::VaultRecall(request) => {
                match vault_recall(VaultRecallRequest {
                    room_dir: PathBuf::from(request.room_dir),
                    room: request.room,
                    query: request.query,
                }) {
                    Ok(result) => success_json(id, result)?,
                    Err(error) => protocol_error(
                        id,
                        ProtocolError::InvalidParams(format!("{}: {error}", error.code())),
                    ),
                }
            }
            ProtocolRequest::SubstrateHealth(request) => {
                let result = crate::substrate_health_with_config(
                    SubstrateHealthOptions {
                        skip_embedding: request.skip_embedding,
                        max_backup_age_hours: request.max_backup_age_hours,
                        ..Default::default()
                    },
                    services.health_config.unwrap_or_else(Config::from_env),
                )
                .await;
                success_json(id, result)?
            }
            ProtocolRequest::SubstrateMigrations(_) => protocol_error(
                id,
                invalid_params(
                    "administrative migrations require the native administration entry point",
                ),
            ),
            request => {
                if let Some(error) = bootstrap_error {
                    app_error(id, operation, error)
                } else {
                    let (config, pool) =
                        runtime.expect("the native owner supplies a runtime or its failure");
                    match request {
                        ProtocolRequest::CanonWrite(request) => {
                            match canon_write(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::CanonRead(request) => {
                            match canon_read(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::Remember(request) => {
                            match remember(pool, config, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::PaperBoatSleep(request) => {
                            match paper_boat_sleep(pool, config, request).await {
                                Ok(receipt) => {
                                    success_json(id, PaperBoatSleepResult::from(receipt))?
                                }
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::PaperBoatWake(request) => {
                            match paper_boat_wake(pool, request).await {
                                Ok(receipt) => {
                                    success_json(id, PaperBoatWakeResult::from(receipt))?
                                }
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayCreate(request) => {
                            match hallway_create(pool, request).await {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayJoin(request) => {
                            match hallway_join(pool, request).await {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayPost(request) => {
                            match crate::hallway::hallway_post_with_auth(
                                pool,
                                config,
                                request,
                                services.nats_auth,
                            )
                            .await
                            {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayRead(request) => {
                            match hallway_read(pool, request).await {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayInbox(request) => {
                            match hallway_inbox(pool, request).await {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayKnockPolicy(request) => {
                            match hallway_knock_policy(pool, request).await {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::HallwayKnock(request) => {
                            match hallway_knock(pool, request).await {
                                Ok(receipt) => success_json(id, receipt)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::Recall(request) => {
                            match recall(pool, config, request, span).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::Anamnesis(request) => {
                            match anamnesis(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::AnamnesisWrite(request) => {
                            match anamnesis_write(pool, config, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestPost(request) => {
                            match quest_post(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestBoard(request) => {
                            match quest_board(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestClaim(request) => {
                            match quest_claim(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestReport(request) => {
                            match quest_report(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestClock(request) => {
                            match quest_clock(pool, config, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestChargebook(request) => {
                            match quest_chargebook(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::QuestEvidence(request) => {
                            match quest_evidence(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::RestartRequest(request) => {
                            match restart_request(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::RestartClaim(request) => {
                            match restart_claim(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::RestartTransition(request) => {
                            match restart_transition(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::RestartVerify(request) => {
                            match restart_verify(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::RestartStatus(request) => {
                            match restart_status(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::LessonQuery(request) => {
                            match lesson_query(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::LessonContext(request) => {
                            match lesson_context(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::LessonUpdate(request) => {
                            match lesson_update(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::LessonDelete(request) => {
                            match lesson_delete(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::LessonTriggerMatch(request) => {
                            match lesson_trigger_match(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::LessonTriggerRecord(request) => {
                            match lesson_trigger_record(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::DesignDocumentQuery(request) => {
                            match design_document_query(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::DesignDocumentWrite(request) => {
                            match design_document_write(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::EntityResolve(request) => {
                            match entity_resolve(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::Cluster(request) => {
                            match cluster_maintenance(pool, request).await {
                                Ok(result) => {
                                    success_json(id, ClusterMaintenanceResultWire::from(result))?
                                }
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaEvent(request) => {
                            match giga_event_ingest(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaConversationIngest(request) => {
                            match giga_conversation_ingest(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaEventClaim(request) => {
                            match giga_event_claim(pool, request).await {
                                Ok(result) => success_json(id, GigaEventClaimResult::from(result))?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaEventFinish(request) => {
                            match giga_event_finish(pool, request).await {
                                Ok(result) => {
                                    success_json(id, GigaEventFinishResult::from(result))?
                                }
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaEventReplay(request) => {
                            match giga_event_replay(pool, request).await {
                                Ok(result) => {
                                    success_json(id, GigaEventReplayResult::from(result))?
                                }
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaQueueMaintenance(request) => {
                            match giga_queue_maintenance(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaPromote(request) => {
                            match giga_promote(pool, config, request).await {
                                Ok(result) => success_json(id, GigaPromoteResult::from(result))?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaToolPromote(request) => {
                            match giga_tool_promote(pool, config, request).await {
                                Ok(result) => success_json(id, GigaPromoteResult::from(result))?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaCandidateList(request) => {
                            match giga_candidate_list(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaReview(request) => {
                            match giga_review(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaToolReview(request) => {
                            match giga_tool_review(pool, request).await {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::GigaHealth(request) => {
                            match crate::giga::giga_health_with_enablement(
                                pool,
                                request,
                                services
                                    .giga_enablement
                                    .unwrap_or_else(crate::GigaEnablement::from_env),
                            )
                            .await
                            {
                                Ok(result) => success_json(id, result)?,
                                Err(error) => app_error(id, operation, error),
                            }
                        }
                        ProtocolRequest::VaultRecall(_)
                        | ProtocolRequest::SubstrateHealth(_)
                        | ProtocolRequest::SubstrateMigrations(_) => {
                            unreachable!(
                                "pre-configuration methods are handled before database initialization"
                            )
                        }
                    }
                }
            }
        }
    };
    Ok(dispatched)
}
