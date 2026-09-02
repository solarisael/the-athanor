use akasha::backup::{BackupError, backup_with_migrations, restore_checked, source_migrations};
use akasha::insula_writer::{
    end_span, flush_insula_emitter, init_insula_emitter, record_point, start_span, system_binding,
};
use akasha::migrations::{migration_pool, run_migrations};
use akasha::{
    AppError, Config, DesignDocumentQueryParams, DesignDocumentWriteParams, EntityResolveParams,
    LessonContextParams, LessonDeleteParams, LessonQueryParams, LessonTriggerMatchParams,
    LessonUpdateParams, OutcomeClass, QuestBoardParams, QuestChargebookParams, QuestClaimParams,
    QuestClockParams, QuestEvidenceParams, QuestPostParams, QuestReportParams,
    SubstrateHealthOptions, TrustedBinding, anamnesis, anamnesis_write, canon_read, canon_write,
    cluster_maintenance, design_document_query, design_document_write, entity_resolve,
    giga_candidate_list, giga_conversation_ingest, giga_event_claim, giga_event_finish,
    giga_event_ingest, giga_event_replay, giga_health, giga_promote, giga_queue_maintenance,
    giga_review, giga_tool_promote, giga_tool_review, hallway_create, hallway_inbox, hallway_join,
    hallway_knock, hallway_knock_policy, hallway_post, hallway_read, lesson_context, lesson_delete,
    lesson_query, lesson_trigger_match, lesson_update, paper_boat_sleep, paper_boat_wake,
    quest_board, quest_chargebook, quest_claim, quest_clock, quest_evidence, quest_post,
    quest_report, recall, refresh_semantic_vocabulary, remember, restart_claim, restart_request,
    restart_status, restart_transition, restart_verify, spawn_giga_worker, substrate_health,
    substrate_health_with_config, validate_trusted_binding,
};
use chrono::{DateTime, Duration, Timelike, Utc};
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
use std::{
    env,
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use summoning::{
    AnamnesisReadRequest, AnamnesisWriteRequest, PaperBoatSleepRequest, PaperBoatWakeRequest,
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use vault::{VaultRecallRequest, recall as vault_recall};

#[derive(Debug)]
enum ProtocolRequest {
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

fn decode_line(line: &str) -> (String, Result<ProtocolRequest, ProtocolError>) {
    let envelope = match RequestEnvelope::parse_line(line) {
        Ok(envelope) => envelope,
        Err(error) => return ("unknown".into(), Err(error)),
    };
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
        "recall" => envelope
            .recall_request()
            .map(ProtocolRequest::Recall),
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
struct Dispatched {
    json: String,
    outcome: OutcomeClass,
    error_class: Option<&'static str>,
}

const INSULA_COMPONENT: &str = "akasha";
const INSULA_LAYER: &str = "substrate";

// Error classes name the error variant and nothing else: no message, code, or
// caller text may reach an observation. `atom` in insula.rs also refuses
// anything but a lowercase mechanical name, so `type_name` is unusable here.
fn protocol_error_class(error: &ProtocolError) -> &'static str {
    match error {
        ProtocolError::Malformed(_) => "protocol_error.malformed",
        ProtocolError::ProtocolMismatch(_) => "protocol_error.protocol_mismatch",
        ProtocolError::UnknownMethod(_) => "protocol_error.unknown_method",
        ProtocolError::InvalidParams(_) => "protocol_error.invalid_params",
    }
}

fn app_error_class(error: &AppError) -> &'static str {
    match error {
        AppError::Invalid(_) => "app_error.invalid",
        AppError::Refusal { .. } => "app_error.refusal",
        AppError::Config(_) => "app_error.config",
        AppError::Database(_) => "app_error.database",
        AppError::DatabaseConnect(_) => "app_error.database_connect",
        AppError::DatabaseSchema(_) => "app_error.database_schema",
        AppError::Embedding(_) => "app_error.embedding",
        AppError::Protocol(_) => "app_error.protocol",
        AppError::Io(_) => "app_error.io",
    }
}

fn backup_error_class(error: &BackupError) -> &'static str {
    match error {
        BackupError::Config(_) => "backup_error.config",
        BackupError::State(_) => "backup_error.state",
        BackupError::Io(_) => "backup_error.io",
        BackupError::Command(_) => "backup_error.command",
        BackupError::Manifest(_) => "backup_error.manifest",
    }
}

/// `Invalid` and `Refusal` are the House refusing a request, not the substrate
/// failing at one: the same class of event whether request validation or a
/// service raises it.
fn app_error_outcome(error: &AppError) -> OutcomeClass {
    match error {
        AppError::Invalid(_) | AppError::Refusal { .. } => OutcomeClass::Refused,
        _ => OutcomeClass::Error,
    }
}

/// The observed operation is the protocol method itself: one table, so a new
/// method cannot reach the dispatch loop without naming what it does.
fn operation_name(request: &ProtocolRequest) -> &'static str {
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
fn insula_binding(request: &ProtocolRequest) -> TrustedBinding {
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

fn protocol_error(id: String, error: ProtocolError) -> Dispatched {
    Dispatched {
        outcome: OutcomeClass::Refused,
        error_class: Some(protocol_error_class(&error)),
        json: error_json(id, error.into()),
    }
}

fn app_error(id: String, operation: &str, error: AppError) -> Dispatched {
    Dispatched {
        outcome: app_error_outcome(&error),
        error_class: Some(app_error_class(&error)),
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

async fn cli_subcommand() -> Result<bool, Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = argv.first().cloned() else {
        return Ok(false);
    };
    let args = &argv[1..];
    let expect = |names: &[&str]| -> Result<Vec<String>, String> {
        if args.len() != names.len() * 2 {
            return Err("unexpected or missing arguments".into());
        }
        let mut out = Vec::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            if args[index * 2] != *name {
                return Err(format!("expected {name} VALUE"));
            }
            out.push(args[index * 2 + 1].clone());
        }
        Ok(out)
    };
    match command.as_str() {
        "backup" => {
            let values =
                expect(&["--output-dir", "--keep"]).map_err(|error| format!("backup: {error}"))?;
            let keep = values[1]
                .parse::<usize>()
                .map_err(|_| "backup: --keep must be an integer".to_string())?;
            let config = Config::from_env().map_err(|error| error.to_string())?;
            let pool = config.pool().await.map_err(|error| error.to_string())?;
            let source = source_migrations(&pool).await?;
            init_insula_emitter(pool.clone());
            let backup = backup_with_migrations(
                &config.database_url,
                &PathBuf::from(&values[0]),
                keep,
                source,
            );
            let binding = system_binding();
            match &backup {
                Ok(manifest) => record_point(
                    &binding,
                    INSULA_COMPONENT,
                    INSULA_LAYER,
                    "pg_backup",
                    OutcomeClass::Ok,
                    None,
                    Some(("insula.backup", &manifest.sha256)),
                ),
                Err(error) => record_point(
                    &binding,
                    INSULA_COMPONENT,
                    INSULA_LAYER,
                    "pg_backup",
                    OutcomeClass::Error,
                    Some(backup_error_class(error)),
                    None,
                ),
            }
            let manifest = backup?;
            println!("{}", serde_json::to_string(&manifest)?);
        }
        "restore" => {
            let values = expect(&["--manifest", "--confirm-database"])
                .map_err(|error| format!("restore: {error}"))?;
            let config = Config::from_env().map_err(|error| error.to_string())?;
            let pool = config.pool().await.map_err(|error| error.to_string())?;
            restore_checked(
                &pool,
                &config.database_url,
                &PathBuf::from(&values[0]),
                &values[1],
            )
            .await?;
            println!("{{\"ok\":true}}");
        }
        "health" => {
            let mut env_file = None;
            let mut substrate_dir = None;
            let mut skip_embedding = false;
            let mut max_backup_age_hours = 24.0;
            let mut index = 0;
            while index < args.len() {
                match args[index].as_str() {
                    "--skip-embedding" => {
                        skip_embedding = true;
                        index += 1;
                    }
                    "--env-file" | "--substrate-dir" | "--max-backup-age-hours" => {
                        let value = args
                            .get(index + 1)
                            .ok_or_else(|| format!("health: {} requires a value", args[index]))?;
                        match args[index].as_str() {
                            "--env-file" => env_file = Some(PathBuf::from(value)),
                            "--substrate-dir" => substrate_dir = Some(PathBuf::from(value)),
                            "--max-backup-age-hours" => {
                                max_backup_age_hours = value.parse::<f64>().map_err(|_| {
                                    "health: --max-backup-age-hours must be a number".to_string()
                                })?;
                            }
                            _ => unreachable!(),
                        }
                        index += 2;
                    }
                    argument => {
                        return Err(format!("health: unexpected argument {argument}").into());
                    }
                }
            }
            if !max_backup_age_hours.is_finite() || max_backup_age_hours <= 0.0 {
                return Err("health: --max-backup-age-hours must be positive and finite".into());
            }
            let state_root = env_file.as_ref().and_then(|path| {
                (path.parent()?.file_name()?.to_str()? == "substrate")
                    .then(|| path.parent()?.parent().map(PathBuf::from))
                    .flatten()
            });
            let backup_directory = state_root
                .as_ref()
                .map(|root| root.join("substrate").join("backups"));
            let config = env_file
                .as_ref()
                .map_or_else(Config::from_env, |path| Config::from_env_file(path));
            let verdict = substrate_health_with_config(
                SubstrateHealthOptions {
                    skip_embedding,
                    max_backup_age_hours,
                    state_root,
                    state_root_source: env_file.as_ref().map(|_| "explicit_env_file".into()),
                    dotenv: env_file,
                    substrate_dir,
                    backup_directory,
                },
                config,
            )
            .await;
            println!("{}", serde_json::to_string(&verdict)?);
            if !verdict.ok {
                return Err("substrate health is degraded".into());
            }
        }
        "migrations" => {
            let env_file = if args.is_empty() {
                None
            } else {
                Some(PathBuf::from(
                    expect(&["--env-file"])
                        .map_err(|error| format!("migrations: {error}"))?
                        .remove(0),
                ))
            };
            let config = env_file
                .as_ref()
                .map_or_else(Config::from_env, |path| Config::from_env_file(path))
                .map_err(|error| error.to_string())?;
            let pool = migration_pool(&config)
                .await
                .map_err(|error| error.to_string())?;
            let result = run_migrations(&pool)
                .await
                .map_err(|error| error.to_string())?;
            println!("{}", serde_json::to_string(&result)?);
        }
        "semantic-vocabulary-refresh" => {
            if !args.is_empty() {
                return Err("semantic-vocabulary-refresh: no arguments accepted".into());
            }
            let config = Config::from_env().map_err(|error| error.to_string())?;
            let pool = config.pool().await.map_err(|error| error.to_string())?;
            let refreshed = refresh_semantic_vocabulary(&pool, &config).await?;
            println!("{{\"ok\":true,\"refreshed\":{refreshed}}}");
        }
        _ => return Err(format!("unknown subcommand: {command}").into()),
    }
    Ok(true)
}

trait KeepaliveProcess {
    fn terminate(&mut self);
}

struct ChildKeepalive(Child);

impl KeepaliveProcess for ChildKeepalive {
    fn terminate(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct WslKeepalive<P: KeepaliveProcess>(Option<P>);

fn keepalive_requested(is_windows: bool, flag: Option<&str>) -> bool {
    is_windows && flag == Some("1")
}

impl WslKeepalive<ChildKeepalive> {
    fn start() -> Result<Self, std::io::Error> {
        let flag = env::var("ATHANOR_PG_WSL").ok();
        Self::start_with(cfg!(windows), flag.as_deref(), || {
            Command::new("wsl.exe")
                .args(["--exec", "sleep", "infinity"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(ChildKeepalive)
        })
    }
}

impl<P: KeepaliveProcess> WslKeepalive<P> {
    fn start_with<F>(is_windows: bool, flag: Option<&str>, spawn: F) -> Result<Self, std::io::Error>
    where
        F: FnOnce() -> Result<P, std::io::Error>,
    {
        if !keepalive_requested(is_windows, flag) {
            return Ok(Self(None));
        }
        Ok(Self(Some(spawn()?)))
    }
}

// Cleanup is guaranteed for ordinary Rust teardown. A forced process kill skips
// Drop, so this process cannot reap a keepalive in that case.
impl<P: KeepaliveProcess> Drop for WslKeepalive<P> {
    fn drop(&mut self) {
        if let Some(process) = self.0.as_mut() {
            process.terminate();
        }
    }
}

const RETENTION_INITIAL_DELAY: std::time::Duration = std::time::Duration::from_secs(5 * 60);
const RETENTION_CADENCE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

fn retention_schedule() -> (std::time::Duration, std::time::Duration) {
    (RETENTION_INITIAL_DELAY, RETENTION_CADENCE)
}

fn retention_cutoff(now: DateTime<Utc>) -> DateTime<Utc> {
    (now - Duration::days(14))
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .expect("UTC timestamps always support minute truncation")
}

fn retention_error_class(error: &akasha::InsulaError) -> &'static str {
    match error {
        akasha::InsulaError::Validation { .. } => "insula_error.validation",
        akasha::InsulaError::Database(_) => "insula_error.database",
        akasha::InsulaError::Invariant(_) => "insula_error.invariant",
    }
}

fn spawn_retention_service() {
    tokio::spawn(async {
        // This is idempotent maintenance, not a heartbeat or monitor: retention
        // receipts make scheduled sweeps replay-safe without asserting liveness.
        let binding = akasha::insula_writer::system_binding();
        let (initial_delay, cadence) = retention_schedule();
        tokio::time::sleep(initial_delay).await;
        let mut ticker = tokio::time::interval(cadence);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // enough: fixed 24h cadence; Docket standing-intent scheduling when it exists
        loop {
            ticker.tick().await;
            let config = match Config::from_env() {
                Ok(config) => config,
                Err(error) => {
                    tracing::warn!(
                        error_class = app_error_class(&error),
                        "retention_sweep_unavailable"
                    );
                    continue;
                }
            };
            let pool = match config.pool().await {
                Ok(pool) => pool,
                Err(error) => {
                    tracing::warn!(
                        error_class = app_error_class(&error),
                        "retention_sweep_unavailable"
                    );
                    continue;
                }
            };
            let settings = match akasha::RoomSettings::load(&pool, "house").await {
                Ok(settings) => settings,
                Err(error) => {
                    tracing::warn!(error = %error, "retention_settings_unavailable");
                    continue;
                }
            };

            let span = akasha::insula_writer::start_span(
                &binding,
                "akasha",
                "substrate",
                "retention_sweep",
            );
            match akasha::run_retention(
                &pool,
                "solarisael",
                retention_cutoff(Utc::now()),
                settings.insula_retention_days,
            )
            .await
            {
                Ok(receipt) => {
                    akasha::insula_writer::end_span(span, akasha::OutcomeClass::Ok, None);
                    if let Some(receipt_id) = receipt.receipt_id.as_deref() {
                        akasha::insula_writer::record_point(
                            &binding,
                            "akasha",
                            "substrate",
                            "retention_sweep",
                            akasha::OutcomeClass::Ok,
                            None,
                            Some(("insula.retention.raw_delete", receipt_id)),
                        );
                    }
                }
                Err(error) => {
                    let class = retention_error_class(&error);
                    akasha::insula_writer::end_span(span, akasha::OutcomeClass::Error, Some(class));
                    tracing::warn!(error_class = class, "retention_sweep_failed");
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _wsl_keepalive = WslKeepalive::start()?;
    if cli_subcommand().await? {
        flush_insula_emitter().await;
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("warn")
        .init();
    spawn_retention_service();
    let mut runtime = None;
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = io::BufWriter::new(io::stdout());
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (id, request) = decode_line(trimmed);
        let response = match request {
            Ok(request) => {
                let operation = operation_name(&request);
                let binding = insula_binding(&request);
                let span = start_span(&binding, INSULA_COMPONENT, INSULA_LAYER, operation);
                let validation = match &request {
                    ProtocolRequest::CanonWrite(_) | ProtocolRequest::CanonRead(_) => Ok(()),
                    ProtocolRequest::Remember(_) => Ok(()),
                    ProtocolRequest::PaperBoatSleep(_) | ProtocolRequest::PaperBoatWake(_) => {
                        Ok(())
                    }
                    ProtocolRequest::HallwayCreate(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::HallwayJoin(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::HallwayPost(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::HallwayRead(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::HallwayInbox(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::HallwayKnockPolicy(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::HallwayKnock(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::Recall(_) => Ok(()),
                    ProtocolRequest::VaultRecall(_) => Ok(()),
                    ProtocolRequest::QuestPost(request) => request.validate(),
                    ProtocolRequest::QuestBoard(request) => request.validate(),
                    ProtocolRequest::QuestClaim(request) => request.validate(),
                    ProtocolRequest::QuestReport(request) => request.validate(),
                    ProtocolRequest::QuestClock(request) => request.validate(),
                    ProtocolRequest::QuestChargebook(request) => request.validate(),
                    ProtocolRequest::QuestEvidence(request) => request.validate(),
                    ProtocolRequest::RestartRequest(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::RestartClaim(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::RestartTransition(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::RestartVerify(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::RestartStatus(request) => {
                        request.validate().map_err(AppError::Invalid)
                    }
                    ProtocolRequest::Anamnesis(_)
                    | ProtocolRequest::AnamnesisWrite(_)
                    | ProtocolRequest::LessonQuery(_)
                    | ProtocolRequest::LessonContext(_)
                    | ProtocolRequest::LessonUpdate(_)
                    | ProtocolRequest::LessonDelete(_)
                    | ProtocolRequest::LessonTriggerMatch(_)
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
                                    ProtocolError::InvalidParams(format!(
                                        "{}: {error}",
                                        error.code()
                                    )),
                                ),
                            }
                        }
                        ProtocolRequest::SubstrateHealth(request) => {
                            let result = substrate_health(SubstrateHealthOptions {
                                skip_embedding: request.skip_embedding,
                                max_backup_age_hours: request.max_backup_age_hours,
                                ..Default::default()
                            })
                            .await;
                            success_json(id, result)?
                        }
                        ProtocolRequest::SubstrateMigrations(_) => match Config::from_env() {
                            Ok(config) => match migration_pool(&config).await {
                                Ok(pool) => match run_migrations(&pool).await {
                                    Ok(result) => success_json(id, result)?,
                                    Err(error) => app_error(id, operation, error),
                                },
                                Err(error) => app_error(id, operation, error),
                            },
                            Err(error) => app_error(id, operation, error),
                        },
                        request => {
                            let initialization_error = if runtime.is_none() {
                                match Config::from_env() {
                                    Ok(config) => match config.pool().await {
                                        // enough: the GIGA worker's own claim and
                                        // finish seams stay unobserved; door:
                                        // spawn_giga_worker in giga_worker.rs.
                                        Ok(pool) => match spawn_giga_worker(&pool, &config) {
                                            Ok(worker) => {
                                                // Observation begins with the pool;
                                                // methods answered above this
                                                // door stay unobserved.
                                                init_insula_emitter(pool.clone());
                                                runtime = Some((config, pool, worker));
                                                None
                                            }
                                            Err(error) => Some(error),
                                        },
                                        Err(error) => Some(error),
                                    },
                                    Err(error) => Some(error),
                                }
                            } else {
                                None
                            };
                            if let Some(error) = initialization_error {
                                app_error(id, operation, error)
                            } else {
                                let (config, pool, _) = runtime
                                    .as_ref()
                                    .expect("successful initialization stores the runtime");
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
                                            Ok(receipt) => success_json(
                                                id,
                                                PaperBoatSleepResult::from(receipt),
                                            )?,
                                            Err(error) => app_error(id, operation, error),
                                        }
                                    }
                                    ProtocolRequest::PaperBoatWake(request) => {
                                        match paper_boat_wake(pool, request).await {
                                            Ok(receipt) => success_json(
                                                id,
                                                PaperBoatWakeResult::from(receipt),
                                            )?,
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
                                        match hallway_post(pool, config, request).await {
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
                                        match recall(pool, config, request).await {
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
                                            Ok(result) => success_json(
                                                id,
                                                ClusterMaintenanceResultWire::from(result),
                                            )?,
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
                                            Ok(result) => success_json(
                                                id,
                                                GigaEventClaimResult::from(result),
                                            )?,
                                            Err(error) => app_error(id, operation, error),
                                        }
                                    }
                                    ProtocolRequest::GigaEventFinish(request) => {
                                        match giga_event_finish(pool, request).await {
                                            Ok(result) => success_json(
                                                id,
                                                GigaEventFinishResult::from(result),
                                            )?,
                                            Err(error) => app_error(id, operation, error),
                                        }
                                    }
                                    ProtocolRequest::GigaEventReplay(request) => {
                                        match giga_event_replay(pool, request).await {
                                            Ok(result) => success_json(
                                                id,
                                                GigaEventReplayResult::from(result),
                                            )?,
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
                                            Ok(result) => {
                                                success_json(id, GigaPromoteResult::from(result))?
                                            }
                                            Err(error) => app_error(id, operation, error),
                                        }
                                    }
                                    ProtocolRequest::GigaToolPromote(request) => {
                                        match giga_tool_promote(pool, config, request).await {
                                            Ok(result) => {
                                                success_json(id, GigaPromoteResult::from(result))?
                                            }
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
                                        match giga_health(pool, request).await {
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
                end_span(span, dispatched.outcome, dispatched.error_class);
                dispatched
            }
            Err(error) => {
                // A refused line has no method to name, so the decode door
                // itself is the operation.
                let dispatched = protocol_error(id, error);
                record_point(
                    &system_binding(),
                    INSULA_COMPONENT,
                    INSULA_LAYER,
                    "protocol_decode",
                    dispatched.outcome,
                    dispatched.error_class,
                    None,
                );
                dispatched
            }
        };
        stdout.write_all(response.json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    if let Some((_, _, Some(worker))) = runtime {
        worker.shutdown().await;
    }
    flush_insula_emitter().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    struct ProbeProcess(Arc<AtomicBool>);

    impl KeepaliveProcess for ProbeProcess {
        fn terminate(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn wsl_keepalive_selection_and_normal_teardown_are_bounded() {
        let spawn_calls = Arc::new(AtomicUsize::new(0));
        let terminated = Arc::new(AtomicBool::new(false));

        let disabled = WslKeepalive::<ProbeProcess>::start_with(false, Some("1"), {
            let spawn_calls = Arc::clone(&spawn_calls);
            let terminated = Arc::clone(&terminated);
            move || {
                spawn_calls.fetch_add(1, Ordering::SeqCst);
                Ok(ProbeProcess(terminated))
            }
        })
        .expect("disabled keepalive must not fail");
        assert!(disabled.0.is_none());

        let disabled = WslKeepalive::<ProbeProcess>::start_with(true, None, {
            let spawn_calls = Arc::clone(&spawn_calls);
            let terminated = Arc::clone(&terminated);
            move || {
                spawn_calls.fetch_add(1, Ordering::SeqCst);
                Ok(ProbeProcess(terminated))
            }
        })
        .expect("unflagged keepalive must not fail");
        assert!(disabled.0.is_none());
        assert_eq!(spawn_calls.load(Ordering::SeqCst), 0);

        {
            let enabled = WslKeepalive::<ProbeProcess>::start_with(true, Some("1"), {
                let spawn_calls = Arc::clone(&spawn_calls);
                let terminated = Arc::clone(&terminated);
                move || {
                    spawn_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(ProbeProcess(terminated))
                }
            })
            .expect("enabled keepalive must start");
            assert!(enabled.0.is_some());
            assert_eq!(spawn_calls.load(Ordering::SeqCst), 1);
        }

        assert!(terminated.load(Ordering::SeqCst));
    }

    #[test]
    fn adapter_supersedes_strings_are_positive_and_deduplicated() {
        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"r1","method":"remember","params":{"room":"room","kind":"memory","title":"title","body":"body","supersedes":["12","3","12"]}}"#,
        );
        match request.unwrap() {
            ProtocolRequest::Remember(request) => assert_eq!(request.supersedes(), &[12, 3]),
            _ => panic!("expected remember"),
        }
    }

    #[test]
    fn rejects_non_decimal_supersedes() {
        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"r1","method":"remember","params":{"room":"room","kind":"memory","title":"title","body":"body","supersedes":["+1"]}}"#,
        );
        assert!(request.unwrap_err().to_string().contains("positive"));
    }

    #[test]
    fn recall_protocol_params_are_strict() {
        let (id, decoded) = decode_line(
            r#"{"protocol":1,"id":"r1","method":"recall","params":{"room":"room","query":"needle","unexpected":true}}"#,
        );
        assert_eq!(id, "r1");
        assert!(matches!(decoded, Err(ProtocolError::InvalidParams(_))));
        let (_, valid) = decode_line(
            r#"{"protocol":1,"id":"r2","method":"recall","params":{"room":"room","query":"needle"}}"#,
        );
        assert!(matches!(valid.unwrap(), ProtocolRequest::Recall(_)));
    }

    #[test]
    fn hallway_protocol_keeps_session_identity_explicit_and_non_singleton() {
        let (_, first) = decode_line(
            r#"{"protocol":1,"id":"h1","method":"hallway_join","params":{"hallway":"shared-hallway","room":"kintsu","spirit":"Kintsu","session":"session-one","idempotencyKey":"join-one"}}"#,
        );
        let (_, second) = decode_line(
            r#"{"protocol":1,"id":"h2","method":"hallway_join","params":{"hallway":"shared-hallway","room":"kintsu","spirit":"Kintsu","session":"session-two","idempotencyKey":"join-two"}}"#,
        );
        let ProtocolRequest::HallwayJoin(first) = first.unwrap() else {
            panic!("expected first Hallway join");
        };
        let ProtocolRequest::HallwayJoin(second) = second.unwrap() else {
            panic!("expected second Hallway join");
        };
        assert_eq!(first.spirit, second.spirit);
        assert_ne!(first.session, second.session);

        let (_, invalid) = decode_line(
            r#"{"protocol":1,"id":"h3","method":"hallway_read","params":{"hallway":"shared-hallway","room":"kintsu","spirit":"Kintsu","session":"session-one","unexpected":true}}"#,
        );
        assert!(matches!(invalid, Err(ProtocolError::InvalidParams(_))));
    }

    #[test]
    fn paper_boat_dispatch_is_domain_prefixed_and_rejects_empty_rooms() {
        let (_, sleep) = decode_line(
            r#"{"protocol":1,"id":"s1","method":"paper_boat_sleep","params":{"room":"kintsu","body":"letter","backup":true}}"#,
        );
        assert!(matches!(sleep.unwrap(), ProtocolRequest::PaperBoatSleep(_)));
        let (_, wake) = decode_line(
            r#"{"protocol":1,"id":"w1","method":"paper_boat_wake","params":{"room":"kintsu"}}"#,
        );
        assert!(matches!(wake.unwrap(), ProtocolRequest::PaperBoatWake(_)));
        let (_, empty) = decode_line(
            r#"{"protocol":1,"id":"w2","method":"paper_boat_wake","params":{"room":""}}"#,
        );
        assert!(matches!(empty, Err(ProtocolError::InvalidParams(_))));
    }

    #[test]
    fn vault_recall_dispatch_has_no_database_parameters() {
        let (_, valid) = decode_line(
            r#"{"protocol":1,"id":"v1","method":"vault_recall","params":{"room":"room","room_dir":"/rooms/room","query":"needle"}}"#,
        );
        assert!(matches!(valid.unwrap(), ProtocolRequest::VaultRecall(_)));
        let (_, invalid) = decode_line(
            r#"{"protocol":1,"id":"v2","method":"vault_recall","params":{"room":"room","room_dir":"/rooms/room","query":"needle","database_url":"forbidden"}}"#,
        );
        assert!(matches!(invalid, Err(ProtocolError::InvalidParams(_))));
    }

    #[test]
    fn protocol_errors_have_current_codes() {
        let (id, result) =
            decode_line(r#"{"protocol":2,"id":"x","method":"remember","params":{}}"#);
        assert_eq!(id, "x");
        assert_eq!(result.unwrap_err(), ProtocolError::ProtocolMismatch(2));
    }

    #[test]
    fn anamnesis_append_uses_shared_domain_validation() {
        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"a1","method":"anamnesis_write","params":{"operation":"append-rep","room":"tuner","title":"cycle","repNumber":1,"occurredOn":"2026-07-23","howItWent":"clean","portalPull":"none","lighter":"yes","sourcePaths":["memory/a.md"]}}"#,
        );
        match request.unwrap() {
            ProtocolRequest::AnamnesisWrite(AnamnesisWriteRequest::AppendRep(request)) => {
                assert_eq!(request.rep().number(), 1);
                assert_eq!(request.title(), "cycle");
            }
            _ => panic!("expected anamnesis write"),
        }
    }

    #[test]
    fn lesson_design_and_entity_protocols_are_strict_and_domain_prefixed() {
        let (_, lesson) = decode_line(
            r#"{"protocol":1,"id":"l1","method":"lesson_query","params":{"room":"kintsu","type":"coding","languageKeys":["rust"],"technologyKeys":["postgresql"],"limit":12}}"#,
        );
        assert!(matches!(lesson.unwrap(), ProtocolRequest::LessonQuery(_)));
        let (_, design) = decode_line(
            r##"{"protocol":1,"id":"d1","method":"design_document_write","params":{"system":"solarisael","docType":"token","name":"color.accent","values":{"hex":"#d4af37"},"provenance":{"source":"repo"},"supersedes":"7"}}"##,
        );
        assert!(matches!(
            design.unwrap(),
            ProtocolRequest::DesignDocumentWrite(_)
        ));
        let (_, entity) = decode_line(
            r#"{"protocol":1,"id":"e1","method":"entity_resolve","params":{"room":"kintsu","query":"North Star","limit":8}}"#,
        );
        assert!(matches!(entity.unwrap(), ProtocolRequest::EntityResolve(_)));
        let (_, invalid) = decode_line(
            r#"{"protocol":1,"id":"l2","method":"lesson_query","params":{"room":"kintsu","type":"coding","database_url":"forbidden"}}"#,
        );
        assert!(matches!(invalid, Err(ProtocolError::InvalidParams(_))));
    }

    #[test]
    fn docket_protocols_are_strict_and_camel_cased() {
        for line in [
            r#"{"protocol":1,"id":"q1","method":"quest_post","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"post-1","action":"goalDraft","houseId":"solarisael","title":"Guild","intent":"Keep books"}}"#,
            r#"{"protocol":1,"id":"q2","method":"quest_board","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","houseId":"solarisael","states":["offered"],"limit":20}}"#,
            r#"{"protocol":1,"id":"q3","method":"quest_claim","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"claim-1","questId":"00000000-0000-0000-0000-000000000001"}}"#,
            r#"{"protocol":1,"id":"q4","method":"quest_report","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"report-1","questId":"00000000-0000-0000-0000-000000000001","attemptId":"00000000-0000-0000-0000-000000000002","leaseToken":"token","action":"settleItem","body":"reviewed","authoredRole":"reviewer","itemPosition":1,"verdict":"met"}}"#,
        ] {
            let (_, request) = decode_line(line);
            match request.expect("Docket fixture must decode") {
                ProtocolRequest::QuestPost(request) => request.validate().unwrap(),
                ProtocolRequest::QuestBoard(request) => request.validate().unwrap(),
                ProtocolRequest::QuestClaim(request) => request.validate().unwrap(),
                ProtocolRequest::QuestReport(request) => request.validate().unwrap(),
                _ => panic!("expected Docket request"),
            }
        }
        let (_, invalid) = decode_line(
            r#"{"protocol":1,"id":"q5","method":"quest_report","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"report-2","questId":"00000000-0000-0000-0000-000000000001","attemptId":"00000000-0000-0000-0000-000000000002","leaseToken":"token","action":"progress","body":"working","role":"executor"}}"#,
        );
        assert!(matches!(invalid, Err(ProtocolError::InvalidParams(_))));
    }

    // Kills: a restart method that decodes loose params, or a validation arm
    // that lets the exit door ride without the session and secret that
    // authorize it.
    // red-proof: drop deny_unknown_fields from a restart params struct, or
    // remove the RestartTransition arm from main()'s validation table.
    #[test]
    fn restart_protocols_are_strict_and_camel_cased() {
        for line in [
            r#"{"protocol":1,"id":"r1","method":"restart_request","params":{"harness":"omp","workspace":"D:/athanor-wt/restart-intent","mode":"resume","sessionId":"s-1","reason":"installed release is newer than the loaded one","consentSource":"operator-standing-policy","requesterRoom":"kodo","requesterSpirit":"Kodo","requesterSession":"service:kodo","capability":"request-secret","idempotencyKey":"request-1"}}"#,
            r#"{"protocol":1,"id":"r2","method":"restart_claim","params":{"intentId":"00000000-0000-0000-0000-000000000001","claimant":"omp-keeper","capability":"secret","idempotencyKey":"claim-1"}}"#,
            r#"{"protocol":1,"id":"r3","method":"restart_transition","params":{"intentId":"00000000-0000-0000-0000-000000000001","to":"exiting","requesterSession":"service:kodo","capability":"exit-secret","detail":"installed release is newer"}}"#,
            r#"{"protocol":1,"id":"r4","method":"restart_transition","params":{"intentId":"00000000-0000-0000-0000-000000000001","claimToken":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","to":"relaunching"}}"#,
            r#"{"protocol":1,"id":"r5","method":"restart_verify","params":{"intentId":"00000000-0000-0000-0000-000000000001","successorSession":"service:kodo-2","successorProof":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789","room":"kodo","spirit":"Kodo","capability":"verify-secret"}}"#,
            r#"{"protocol":1,"id":"r6","method":"restart_status","params":{"workspace":"D:/athanor-wt/restart-intent"}}"#,
            r#"{"protocol":1,"id":"r7","method":"restart_status","params":{"workspace":"D:/athanor-wt/restart-intent","intentId":"00000000-0000-0000-0000-000000000001"}}"#,
        ] {
            let (_, request) = decode_line(line);
            match request.expect("restart fixture must decode") {
                ProtocolRequest::RestartRequest(request) => request.validate().unwrap(),
                ProtocolRequest::RestartClaim(request) => request.validate().unwrap(),
                ProtocolRequest::RestartTransition(request) => request.validate().unwrap(),
                ProtocolRequest::RestartVerify(request) => request.validate().unwrap(),
                ProtocolRequest::RestartStatus(request) => request.validate().unwrap(),
                _ => panic!("expected restart request"),
            }
        }
        let (_, snake) = decode_line(
            r#"{"protocol":1,"id":"r7","method":"restart_request","params":{"harness":"omp","workspace":"D:/w","mode":"resume","reason":"why","consentSource":"operator-approval","requester_room":"kodo","requesterSpirit":"Kodo","requesterSession":"service:kodo","capability":"request-secret","idempotencyKey":"request-2"}}"#,
        );
        assert!(matches!(snake, Err(ProtocolError::InvalidParams(_))));
        let (_, tokened_exit) = decode_line(
            r#"{"protocol":1,"id":"r8","method":"restart_transition","params":{"intentId":"00000000-0000-0000-0000-000000000001","claimToken":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","to":"exiting","requesterSession":"service:kodo","capability":"exit-secret","detail":"armed"}}"#,
        );
        match tokened_exit.expect("the tokened exit decodes: it is validation that refuses it") {
            ProtocolRequest::RestartTransition(request) => assert!(
                request.validate().is_err(),
                "the exit door takes no lease token"
            ),
            _ => panic!("expected restart transition"),
        }
        let (_, unfenced_exit) = decode_line(
            r#"{"protocol":1,"id":"r9","method":"restart_transition","params":{"intentId":"00000000-0000-0000-0000-000000000001","to":"exiting","detail":"armed"}}"#,
        );
        match unfenced_exit.expect("the unfenced exit decodes: validation is what refuses it") {
            ProtocolRequest::RestartTransition(request) => assert!(
                request.validate().is_err(),
                "naming the intent id is not authority to arm an exit"
            ),
            _ => panic!("expected restart transition"),
        }
    }

    #[test]
    fn lesson_trigger_match_protocol_is_strict_and_camel_cased() {
        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"t1","method":"lesson_trigger_match","params":{"room":"kodo","session":"s-1","surfaces":[{"kind":"tool","tool":"edit","path":"src/lib.rs","text":"x.unwrap()"},{"kind":"prose","text":"I will just unwrap it"}]}}"#,
        );
        match request.unwrap() {
            ProtocolRequest::LessonTriggerMatch(request) => {
                assert_eq!(request.session, "s-1");
                assert_eq!(request.surfaces.len(), 2);
                assert_eq!(request.surfaces[0].tool.as_deref(), Some("edit"));
                assert_eq!(request.surfaces[1].path, None);
            }
            _ => panic!("expected lesson trigger match"),
        }
        let (_, snake) = decode_line(
            r#"{"protocol":1,"id":"t2","method":"lesson_trigger_match","params":{"room":"kodo","session":"s-1","surfaces":[{"kind":"tool","tool_name":"edit","text":"x"}]}}"#,
        );
        assert!(matches!(snake, Err(ProtocolError::InvalidParams(_))));
        let (_, missing) = decode_line(
            r#"{"protocol":1,"id":"t3","method":"lesson_trigger_match","params":{"room":"kodo","surfaces":[]}}"#,
        );
        assert!(matches!(missing, Err(ProtocolError::InvalidParams(_))));
    }

    #[test]
    fn substrate_lifecycle_protocol_is_strict_and_domain_prefixed() {
        let (_, health) = decode_line(
            r#"{"protocol":1,"id":"h1","method":"substrate_health","params":{"skipEmbedding":true,"maxBackupAgeHours":12}}"#,
        );
        assert!(matches!(
            health.unwrap(),
            ProtocolRequest::SubstrateHealth(_)
        ));
        let (_, migrations) =
            decode_line(r#"{"protocol":1,"id":"m1","method":"substrate_migrations","params":{}}"#);
        assert!(matches!(
            migrations.unwrap(),
            ProtocolRequest::SubstrateMigrations(_)
        ));
        let (_, partial) = decode_line(
            r#"{"protocol":1,"id":"m2","method":"substrate_migrations","params":{"from":12}}"#,
        );
        assert!(matches!(partial, Err(ProtocolError::InvalidParams(_))));
    }
    #[test]
    fn retention_schedule_waits_five_minutes_then_runs_daily() {
        let (first_delay, cadence) = retention_schedule();

        assert_eq!(first_delay, std::time::Duration::from_secs(5 * 60));
        assert_eq!(cadence, std::time::Duration::from_secs(24 * 60 * 60));
    }

    /// insula.rs refuses any name that is not a lowercase mechanical atom, and
    /// a refused event is an observation lost at ingest. Mirrored here because
    /// the validator is private to the organ.
    fn is_mechanical_name(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 64
            && value.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || (index > 0 && matches!(byte, b'_' | b'.' | b':' | b'-'))
            })
    }

    #[test]
    fn observed_error_classes_are_mechanical_and_carry_no_body() {
        let body = "secret prompt body";
        for error in [
            AppError::Invalid(body.into()),
            AppError::Refusal {
                code: "rule",
                message: "static refusal",
            },
            AppError::Config(body.into()),
            AppError::Database(sqlx::Error::PoolClosed),
            AppError::DatabaseConnect(sqlx::Error::PoolClosed),
            AppError::DatabaseSchema(sqlx::Error::PoolClosed),
            AppError::Embedding(body.into()),
            AppError::Protocol(body.into()),
            AppError::Io(std::io::Error::other(body)),
        ] {
            let class = app_error_class(&error);
            assert!(is_mechanical_name(class), "{class} is not mechanical");
            assert!(!class.contains("secret"), "{class} leaked a message");
        }
        for error in [
            ProtocolError::Malformed(body.into()),
            ProtocolError::ProtocolMismatch(2),
            ProtocolError::UnknownMethod(body.into()),
            ProtocolError::InvalidParams(body.into()),
        ] {
            let class = protocol_error_class(&error);
            assert!(is_mechanical_name(class), "{class} is not mechanical");
            assert!(!class.contains("secret"), "{class} leaked a message");
        }
        for error in [
            BackupError::Config(body.into()),
            BackupError::Io(std::io::Error::other(body)),
            BackupError::Command(body.into()),
            BackupError::Manifest(body.into()),
        ] {
            let class = backup_error_class(&error);
            assert!(is_mechanical_name(class), "{class} is not mechanical");
            assert!(!class.contains("secret"), "{class} leaked a message");
        }
    }

    #[test]
    fn refusals_and_faults_are_separate_outcome_classes() {
        assert_eq!(
            app_error_outcome(&AppError::Invalid("bad field".into())),
            OutcomeClass::Refused
        );
        assert_eq!(
            app_error_outcome(&AppError::Refusal {
                code: "rule",
                message: "static refusal"
            }),
            OutcomeClass::Refused
        );
        assert_eq!(
            app_error_outcome(&AppError::Database(sqlx::Error::PoolClosed)),
            OutcomeClass::Error
        );
        assert_eq!(
            app_error_outcome(&AppError::Config("missing".into())),
            OutcomeClass::Error
        );
    }

    #[test]
    fn dispatched_methods_name_their_own_mechanical_operation() {
        for (line, expected) in [
            (
                r#"{"protocol":1,"id":"o1","method":"lesson_query","params":{"room":"tuner","type":"coding","limit":1}}"#,
                "lesson_query",
            ),
            (
                r#"{"protocol":1,"id":"o2","method":"hallway_inbox","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner"}}"#,
                "hallway_inbox",
            ),
            (
                r#"{"protocol":1,"id":"o3","method":"substrate_migrations","params":{}}"#,
                "substrate_migrations",
            ),
            (
                r#"{"protocol":1,"id":"o4","method":"quest_post","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"post-1","action":"draft","houseId":"solarisael","kind":"work","title":"Cut Docket","body":"Build it"}}"#,
                "quest_post",
            ),
            (
                r#"{"protocol":1,"id":"o5","method":"quest_board","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","houseId":"solarisael"}}"#,
                "quest_board",
            ),
            (
                r#"{"protocol":1,"id":"o6","method":"quest_claim","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"claim-1","questId":"00000000-0000-0000-0000-000000000001"}}"#,
                "quest_claim",
            ),
            (
                r#"{"protocol":1,"id":"o7","method":"quest_report","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","capability":"secret","idempotencyKey":"report-1","questId":"00000000-0000-0000-0000-000000000001","attemptId":"00000000-0000-0000-0000-000000000002","leaseToken":"token","action":"progress","body":"working"}}"#,
                "quest_report",
            ),
            (
                r#"{"protocol":1,"id":"o8","method":"restart_request","params":{"harness":"omp","workspace":"D:/w","mode":"resume","reason":"newer release installed","consentSource":"operator-standing-policy","requesterRoom":"kodo","requesterSpirit":"Kodo","requesterSession":"service:kodo","capability":"request-secret","idempotencyKey":"request-1"}}"#,
                "restart_request",
            ),
            (
                r#"{"protocol":1,"id":"o9","method":"restart_claim","params":{"intentId":"00000000-0000-0000-0000-000000000001","claimant":"omp-keeper","capability":"secret","idempotencyKey":"claim-1"}}"#,
                "restart_claim",
            ),
            (
                r#"{"protocol":1,"id":"o10","method":"restart_transition","params":{"intentId":"00000000-0000-0000-0000-000000000001","to":"exiting","requesterSession":"service:kodo","capability":"exit-secret","detail":"armed"}}"#,
                "restart_transition",
            ),
            (
                r#"{"protocol":1,"id":"o11","method":"restart_verify","params":{"intentId":"00000000-0000-0000-0000-000000000001","successorSession":"service:kodo-2","successorProof":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789","room":"kodo","spirit":"Kodo","capability":"verify-secret"}}"#,
                "restart_verify",
            ),
            (
                r#"{"protocol":1,"id":"o12","method":"restart_status","params":{"workspace":"D:/w"}}"#,
                "restart_status",
            ),
        ] {
            let (_, request) = decode_line(line);
            let operation = operation_name(&request.expect("fixture decodes"));
            assert_eq!(operation, expected);
            assert!(
                is_mechanical_name(operation),
                "{operation} is not mechanical"
            );
        }
    }

    #[test]
    fn hallway_and_docket_requests_are_observed_under_caller_identity() {
        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"b1","method":"hallway_inbox","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner"}}"#,
        );
        let binding = insula_binding(&request.expect("fixture decodes"));
        assert_eq!(binding.room, "tuner");
        assert_eq!(binding.spirit, "Tuner");
        assert_eq!(binding.session_id, "service:tuner");
        assert_eq!(binding.house_id, system_binding().house_id);

        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"b4","method":"quest_board","params":{"room":"tuner","spirit":"Tuner","session":"service:tuner","houseId":"solarisael"}}"#,
        );
        let binding = insula_binding(&request.expect("fixture decodes"));
        assert_eq!(binding.room, "tuner");
        assert_eq!(binding.spirit, "Tuner");
        assert_eq!(binding.session_id, "service:tuner");

        // The restart plane's two identity-bearing doors: the requesting
        // session names itself, the successor names its new session.
        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"b5","method":"restart_request","params":{"harness":"omp","workspace":"D:/w","mode":"resume","reason":"newer release installed","consentSource":"operator-standing-policy","requesterRoom":"tuner","requesterSpirit":"Tuner","requesterSession":"service:tuner","capability":"request-secret","idempotencyKey":"request-1"}}"#,
        );
        let binding = insula_binding(&request.expect("fixture decodes"));
        assert_eq!(binding.room, "tuner");
        assert_eq!(binding.spirit, "Tuner");
        assert_eq!(binding.session_id, "service:tuner");

        let (_, request) = decode_line(
            r#"{"protocol":1,"id":"b6","method":"restart_verify","params":{"intentId":"00000000-0000-0000-0000-000000000001","successorSession":"service:tuner-2","successorProof":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789","room":"tuner","spirit":"Tuner","capability":"verify-secret"}}"#,
        );
        let binding = insula_binding(&request.expect("fixture decodes"));
        assert_eq!(binding.session_id, "service:tuner-2");
    }

    #[test]
    fn service_methods_and_unusable_identities_fall_back_to_the_house_voice() {
        let (_, service) = decode_line(
            r#"{"protocol":1,"id":"b2","method":"lesson_query","params":{"room":"tuner","type":"coding","limit":1}}"#,
        );
        assert_eq!(
            insula_binding(&service.expect("fixture decodes")),
            system_binding()
        );
        let (_, unusable) = decode_line(
            r#"{"protocol":1,"id":"b3","method":"hallway_inbox","params":{"room":"Tuner_Room","spirit":"Tuner","session":"service:tuner"}}"#,
        );
        assert_eq!(
            insula_binding(&unusable.expect("fixture decodes")),
            system_binding()
        );
    }
}
