use athanor_substrate::backup::{backup_with_migrations, restore_checked, source_migrations};
use athanor_substrate::{
    AnamnesisParams, AnamnesisSeed, AnamnesisWrite, AppError, Config, RecallParams,
    RememberRequest, ThreadContinuation as ServiceThreadContinuation, anamnesis, anamnesis_write,
    cluster_maintenance, giga_candidate_list, giga_event_claim, giga_event_finish,
    giga_event_ingest, giga_event_replay, giga_health, giga_process, giga_promote,
    giga_queue_maintenance, giga_review, recall, refresh_semantic_vocabulary, remember,
};
use chrono::NaiveDate;
use house_core::{
    AnamnesisAddRequest as DomainAnamnesisAddRequest,
    AnamnesisAppendRequest as DomainAnamnesisAppendRequest,
    AnamnesisReadRequest as DomainAnamnesisReadRequest,
    ClusterMaintenanceRequest as DomainClusterMaintenanceRequest, GigaEvent, GigaEventClaimRequest,
    GigaEventFinishRequest, GigaEventReplayRequest, GigaProcessRequest, GigaPromotionRequest,
    GigaQueueMaintenanceRequest, GigaReviewAction, RecallRequest as DomainRecallRequest,
    RememberRequest as DomainRememberRequest,
};
use house_protocol::{
    GigaCandidateListRequest, GigaEventClaimResult, GigaEventFinishResult, GigaEventReplayResult,
    GigaHealthRequest, GigaPromoteResult, PROTOCOL_VERSION, ProtocolError, ProtocolErrorBody,
    RequestEnvelope, ResponseEnvelope, ResponsePayload, success,
};
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
    Remember(RememberRequest),
    Recall(RecallParams),
    Anamnesis(AnamnesisParams),
    AnamnesisWrite(AnamnesisWrite),
    Cluster(DomainClusterMaintenanceRequest),
    GigaEvent(GigaEvent),
    GigaProcess(GigaProcessRequest),
    GigaEventClaim(GigaEventClaimRequest),
    GigaEventFinish(GigaEventFinishRequest),
    GigaEventReplay(GigaEventReplayRequest),
    GigaQueueMaintenance(GigaQueueMaintenanceRequest),
    GigaPromote(GigaPromotionRequest),
    GigaCandidateList(GigaCandidateListRequest),
    GigaReview(GigaReviewAction),
    GigaHealth(GigaHealthRequest),
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
        source_memory_path: None,
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
        "remember" => envelope
            .remember_request()
            .and_then(remember_service_request)
            .map(ProtocolRequest::Remember),
        "recall" => envelope
            .recall_request()
            .map(recall_service_request)
            .map(ProtocolRequest::Recall),
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
        "giga_event_ingest" => envelope
            .giga_event_ingest_request()
            .map(ProtocolRequest::GigaEvent),
        "giga_process" => envelope
            .giga_process_request()
            .map(ProtocolRequest::GigaProcess),
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
        "giga_candidate_list" => envelope
            .giga_candidate_list_request()
            .map(ProtocolRequest::GigaCandidateList),
        "giga_review" => envelope
            .giga_review_request()
            .map(ProtocolRequest::GigaReview),
        "giga_health" => envelope
            .giga_health_request()
            .map(ProtocolRequest::GigaHealth),
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
    let config = Config::from_env().map_err(|error| error.to_string())?;
    match command.as_str() {
        "backup" => {
            let values =
                expect(&["--output-dir", "--keep"]).map_err(|error| format!("backup: {error}"))?;
            let keep = values[1]
                .parse::<usize>()
                .map_err(|_| "backup: --keep must be an integer".to_string())?;
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
        "semantic-vocabulary-refresh" => {
            if !args.is_empty() {
                return Err("semantic-vocabulary-refresh: no arguments accepted".into());
            }
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
                    ProtocolRequest::Remember(_) => "remember",
                    ProtocolRequest::Recall(_) => "recall",
                    ProtocolRequest::Anamnesis(_) => "anamnesis",
                    ProtocolRequest::AnamnesisWrite(_) => "anamnesis_write",
                    ProtocolRequest::Cluster(_) => "cluster_maintenance",
                    ProtocolRequest::GigaEvent(_) => "giga_event_ingest",
                    ProtocolRequest::GigaProcess(_) => "giga_process",
                    ProtocolRequest::GigaEventClaim(_) => "giga_event_claim",
                    ProtocolRequest::GigaEventFinish(_) => "giga_event_finish",
                    ProtocolRequest::GigaEventReplay(_) => "giga_event_replay",
                    ProtocolRequest::GigaQueueMaintenance(_) => "giga_queue_maintenance",
                    ProtocolRequest::GigaPromote(_) => "giga_promote",
                    ProtocolRequest::GigaCandidateList(_) => "giga_candidate_list",
                    ProtocolRequest::GigaReview(_) => "giga_review",
                    ProtocolRequest::GigaHealth(_) => "giga_health",
                };
                let validation = match &request {
                    ProtocolRequest::Remember(request) => request.validate(),
                    ProtocolRequest::Recall(request) => request.validate(),
                    ProtocolRequest::Anamnesis(request) => request.validate().map(|_| ()),
                    ProtocolRequest::AnamnesisWrite(_)
                    | ProtocolRequest::Cluster(_)
                    | ProtocolRequest::GigaEvent(_)
                    | ProtocolRequest::GigaProcess(_)
                    | ProtocolRequest::GigaEventClaim(_)
                    | ProtocolRequest::GigaEventFinish(_)
                    | ProtocolRequest::GigaEventReplay(_)
                    | ProtocolRequest::GigaQueueMaintenance(_)
                    | ProtocolRequest::GigaPromote(_)
                    | ProtocolRequest::GigaCandidateList(_)
                    | ProtocolRequest::GigaReview(_)
                    | ProtocolRequest::GigaHealth(_) => Ok(()),
                };
                if let Err(error) = validation {
                    app_error(id, operation, error)
                } else {
                    let initialization_error = if runtime.is_none() {
                        match Config::from_env() {
                            Ok(config) => match config.pool().await {
                                Ok(pool) => {
                                    runtime = Some((config, pool));
                                    None
                                }
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
                        let (config, pool) = runtime
                            .as_ref()
                            .expect("successful initialization stores the runtime");
                        match request {
                            ProtocolRequest::Remember(request) => {
                                match remember(pool, config, request).await {
                                    Ok(result) => success_json(id, result)?,
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
                            ProtocolRequest::Cluster(request) => {
                                match cluster_maintenance(
                                    pool,
                                    request.operation().as_str(),
                                    request.dry_run(),
                                    request.if_stale(),
                                    request.k() as usize,
                                )
                                .await
                                {
                                    Ok(result) => success_json(id, result)?,
                                    Err(error) => app_error(id, operation, error),
                                }
                            }
                            ProtocolRequest::GigaEvent(request) => {
                                match giga_event_ingest(pool, request).await {
                                    Ok(result) => success_json(id, result)?,
                                    Err(error) => app_error(id, operation, error),
                                }
                            }
                            ProtocolRequest::GigaProcess(request) => {
                                match giga_process(pool, config, request).await {
                                    Ok(result) => success_json(id, result)?,
                                    Err(error) => app_error(id, operation, error),
                                }
                            }
                            ProtocolRequest::GigaEventClaim(request) => {
                                match giga_event_claim(pool, request).await {
                                    Ok(result) => {
                                        success_json(id, GigaEventClaimResult::from(result))?
                                    }
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
                            ProtocolRequest::GigaHealth(request) => {
                                match giga_health(pool, request).await {
                                    Ok(result) => success_json(id, result)?,
                                    Err(error) => app_error(id, operation, error),
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
}
