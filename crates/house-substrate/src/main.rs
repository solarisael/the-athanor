use athanor_substrate::backup::{backup_with_migrations, restore_checked, source_migrations};
use athanor_substrate::migrations::{migration_pool, run_migrations};
use athanor_substrate::{
    AnamnesisParams, AnamnesisSeed, AnamnesisWrite, AppError, Config, DesignDocumentQueryParams,
    DesignDocumentWriteParams, EntityResolveParams, LessonContextParams, LessonDeleteParams,
    LessonQueryParams, LessonTriggerMatchParams, LessonUpdateParams, RecallParams, RememberRequest,
    SubstrateHealthOptions, ThreadContinuation as ServiceThreadContinuation, anamnesis,
    anamnesis_write, canon_read, canon_write, cluster_maintenance, design_document_query,
    design_document_write, entity_resolve, giga_candidate_list, giga_conversation_ingest,
    giga_event_claim, giga_event_finish, giga_event_ingest, giga_event_replay, giga_health,
    giga_promote, giga_queue_maintenance, giga_review, giga_tool_promote, giga_tool_review,
    hallway_create, hallway_inbox, hallway_join, hallway_knock, hallway_knock_policy, hallway_post,
    hallway_read, lesson_context, lesson_delete, lesson_query, lesson_trigger_match, lesson_update,
    paper_boat_sleep, paper_boat_wake, recall, refresh_semantic_vocabulary, remember,
    spawn_giga_worker, substrate_health, substrate_health_with_config,
};
use chrono::NaiveDate;
use house_core::{
    AnamnesisAddRequest as DomainAnamnesisAddRequest,
    AnamnesisAppendRequest as DomainAnamnesisAppendRequest,
    AnamnesisReadRequest as DomainAnamnesisReadRequest, CanonReadRequest, CanonWriteRequest,
    ClusterMaintenanceRequest as DomainClusterMaintenanceRequest, GigaEvent, GigaEventClaimRequest,
    GigaEventFinishRequest, GigaEventReplayRequest, GigaPromotionRequest,
    GigaQueueMaintenanceRequest, GigaReviewAction, PaperBoatSleepRequest, PaperBoatWakeRequest,
    RecallRequest as DomainRecallRequest, RememberRequest as DomainRememberRequest,
    hallway::{
        HallwayCreateRequest, HallwayInboxRequest, HallwayJoinRequest, HallwayKnockPolicyRequest,
        HallwayKnockRequest, HallwayPostRequest, HallwayReadRequest,
    },
};
use house_protocol::{
    ClusterMaintenanceResultWire, GigaCandidateListRequest, GigaConversationIngestParams,
    GigaEventClaimResult, GigaEventFinishResult, GigaEventReplayResult, GigaHealthRequest,
    GigaPromoteResult, GigaToolPromoteParams, GigaToolReviewParams, PROTOCOL_VERSION,
    PaperBoatSleepResult, PaperBoatWakeResult, ProtocolError, ProtocolErrorBody, RequestEnvelope,
    ResponseEnvelope, ResponsePayload, SubstrateHealthParams, SubstrateMigrationsParams,
    VaultRecallParams, success,
};
use house_vault::{VaultRecallRequest, recall as vault_recall};
use serde::Serialize;
use serde_json::Value;
use std::{
    env,
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

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
    Recall(RecallParams),
    VaultRecall(VaultRecallParams),
    Anamnesis(AnamnesisParams),
    AnamnesisWrite(AnamnesisWrite),
    LessonQuery(LessonQueryParams),
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

fn positive_i64(value: u64, field: &str) -> Result<i64, ProtocolError> {
    i64::try_from(value)
        .map_err(|_| invalid_params(format!("{field} is out of PostgreSQL BIGINT range")))
}

fn positive_i32(value: u32, field: &str) -> Result<i32, ProtocolError> {
    i32::try_from(value)
        .map_err(|_| invalid_params(format!("{field} is out of PostgreSQL INTEGER range")))
}

fn optional_date(value: Option<&str>, field: &str) -> Result<Option<NaiveDate>, ProtocolError> {
    value
        .map(|raw| {
            NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .map_err(|_| invalid_params(format!("{field} must use YYYY-MM-DD")))
        })
        .transpose()
}

fn remember_service_request(
    request: DomainRememberRequest,
) -> Result<RememberRequest, ProtocolError> {
    let supersedes = request
        .supersedes()
        .iter()
        .map(|&id| positive_i64(id, "supersedes ID"))
        .collect::<Result<Vec<_>, _>>()?;
    let continues = request
        .continues()
        .iter()
        .map(|continuation| {
            Ok(ServiceThreadContinuation {
                thread: continuation.thread.clone(),
                previous_memory_id: positive_i64(
                    continuation.previous_memory_id,
                    "previousMemoryId",
                )?,
            })
        })
        .collect::<Result<Vec<_>, ProtocolError>>()?;
    Ok(RememberRequest {
        room: request.room().to_string(),
        kind: request.kind().as_str().into(),
        title: request.title().into(),
        body: request.body().into(),
        lesson: None,
        source_path: request.source_path().map(str::to_owned),
        source_memory_path: request.source_memory_path().map(str::to_owned),
        threads: request.threads().to_vec(),
        continues,
        supersedes,
        shape: request.shape().map(str::to_owned),
        voice: request.voice().map(str::to_owned),
        register: request.register().to_vec(),
        scope: request.scope().map(str::to_owned),
        project: request.project().map(str::to_owned),
        proof_pattern: request.proof_pattern().map(str::to_owned),
        trigger_context: request.trigger_context().map(str::to_owned),
        example_text: request.example_text().map(str::to_owned),
        language_keys: request.language_keys().to_vec(),
        technology_keys: request.technology_keys().to_vec(),
        thread_keys: request.thread_keys().to_vec(),
        tags: request.tags().to_vec(),
        condition: request.triggers().condition.clone(),
        ast_condition: request.triggers().ast_condition.clone(),
        trigger_scope: request.triggers().trigger_scope.clone(),
        interrupt_mode: request.triggers().interrupt_mode.clone(),
        repeat_cooldown_secs: request.triggers().repeat_cooldown_secs,
        backup: request.backup(),
    })
}

fn recall_service_request(request: DomainRecallRequest) -> RecallParams {
    RecallParams {
        room: request.room().to_string(),
        query: request.query().into(),
        semantic_top_k: request.semantic_top_k(),
        semantic_min_similarity: request.semantic_min_similarity(),
        content_top_k: request.content_top_k(),
        content_min_similarity: request.content_min_similarity(),
        temporal_decay: request.temporal_decay(),
    }
}

fn anamnesis_service_request(request: DomainAnamnesisReadRequest) -> AnamnesisParams {
    AnamnesisParams {
        room: request.room().to_string(),
        mode: request.mode().as_str().into(),
        query: request.query().unwrap_or_default().into(),
        limit: Some(request.limit()),
    }
}

fn anamnesis_add_service_request(
    request: DomainAnamnesisAddRequest,
) -> Result<AnamnesisWrite, ProtocolError> {
    let seed_rep = request
        .seed_rep()
        .map(|seed| {
            Ok(AnamnesisSeed {
                number: positive_i32(seed.number(), "seedRep.number")?,
                occurred_on: optional_date(seed.occurred_on(), "seedRep.occurredOn")?,
                how_it_went: seed.how_it_went().into(),
                portal_pull: seed.portal_pull().into(),
                lighter: seed.lighter().into(),
                source_path: None,
            })
        })
        .transpose()?;
    Ok(AnamnesisWrite {
        room: request.room().to_string(),
        operation: "add".into(),
        kind: Some(request.kind().as_str().into()),
        fidelity: Some(request.fidelity().as_str().into()),
        activation: Some(request.activation().as_str().into()),
        dormant: request.dormant(),
        title: request.title().into(),
        shape: request.shape().map(str::to_owned),
        ramp: Some(request.ramp().into()),
        counsel: request.counsel().map(str::to_owned),
        peak: request.peak().map(str::to_owned),
        beginning: request.beginning().map(str::to_owned),
        verify_note: request.verify_note().map(str::to_owned),
        source_paths: request.source_paths().to_vec(),
        canon_links: request.canon().to_vec(),
        tags: request.tags().to_vec(),
        allow_empty_cycle: request.allow_empty_cycle(),
        seed_rep,
        backup: true,
        rep_number: None,
        occurred_on: None,
        how_it_went: None,
        portal_pull: None,
        lighter: None,
    })
}

fn anamnesis_append_service_request(
    request: DomainAnamnesisAppendRequest,
) -> Result<AnamnesisWrite, ProtocolError> {
    Ok(AnamnesisWrite {
        room: request.room().to_string(),
        operation: "append-rep".into(),
        kind: None,
        fidelity: None,
        activation: None,
        dormant: false,
        title: request.title().into(),
        shape: None,
        ramp: None,
        counsel: None,
        peak: None,
        beginning: None,
        verify_note: None,
        source_paths: request.source_paths().to_vec(),
        canon_links: Vec::new(),
        tags: Vec::new(),
        allow_empty_cycle: false,
        seed_rep: None,
        backup: true,
        rep_number: Some(positive_i32(request.rep_number(), "repNumber")?),
        occurred_on: optional_date(request.occurred_on(), "occurredOn")?,
        how_it_went: Some(request.how_it_went().into()),
        portal_pull: Some(request.portal_pull().into()),
        lighter: Some(request.lighter().into()),
    })
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
        "remember" => envelope
            .remember_request()
            .and_then(remember_service_request)
            .map(ProtocolRequest::Remember),
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
            .map(recall_service_request)
            .map(ProtocolRequest::Recall),
        "vault_recall" => envelope
            .vault_recall_request()
            .map(ProtocolRequest::VaultRecall),
        "anamnesis" => envelope
            .anamnesis_request()
            .map(anamnesis_service_request)
            .map(ProtocolRequest::Anamnesis),
        "anamnesis_write" => match envelope.params.get("operation").and_then(Value::as_str) {
            Some("add") => envelope
                .anamnesis_add_request()
                .and_then(anamnesis_add_service_request)
                .map(ProtocolRequest::AnamnesisWrite),
            Some("append-rep") => envelope
                .anamnesis_append_request()
                .and_then(anamnesis_append_service_request)
                .map(ProtocolRequest::AnamnesisWrite),
            Some(operation) => Err(invalid_params(format!(
                "unsupported anamnesis_write operation: {operation}"
            ))),
            None => Err(invalid_params("anamnesis_write requires operation")),
        },
        "lesson_query" => serde_json::from_value(envelope.params.clone())
            .map(ProtocolRequest::LessonQuery)
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

fn protocol_error(id: String, error: ProtocolError) -> String {
    error_json(id, error.into())
}

fn app_error(id: String, operation: &str, error: AppError) -> String {
    error_json(id, error.protocol_error_body(operation))
}

fn success_json<T: Serialize>(id: String, result: T) -> Result<String, serde_json::Error> {
    serde_json::to_string(&success(id, result))
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
            let manifest = backup_with_migrations(
                &config.database_url,
                &PathBuf::from(&values[0]),
                keep,
                source,
            )?;
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
        let flag = env::var("SOLARISAEL_PG_WSL").ok();
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _wsl_keepalive = WslKeepalive::start()?;
    if cli_subcommand().await? {
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("warn")
        .init();
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
                let operation = match &request {
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
                };
                let validation = match &request {
                    ProtocolRequest::CanonWrite(_) | ProtocolRequest::CanonRead(_) => Ok(()),
                    ProtocolRequest::Remember(request) => request.validate(),
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
                    ProtocolRequest::Recall(request) => request.validate(),
                    ProtocolRequest::VaultRecall(_) => Ok(()),
                    ProtocolRequest::Anamnesis(request) => request.validate().map(|_| ()),
                    ProtocolRequest::AnamnesisWrite(_)
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
                if let Err(error) = validation {
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
                                        Ok(pool) => match spawn_giga_worker(&pool, &config) {
                                            Ok(worker) => {
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
                }
            }
            Err(error) => protocol_error(id, error),
        };
        stdout.write_all(response.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    if let Some((_, _, Some(worker))) = runtime {
        worker.shutdown().await;
    }
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
            ProtocolRequest::Remember(request) => assert_eq!(request.supersedes, vec![12, 3]),
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
            ProtocolRequest::AnamnesisWrite(request) => {
                assert_eq!(request.operation, "append-rep");
                assert_eq!(request.rep_number, Some(1));
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
}
