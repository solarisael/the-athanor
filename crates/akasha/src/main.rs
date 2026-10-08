use akasha::backup::{BackupError, backup_with_migrations, restore_checked, source_migrations};
use akasha::insula_writer::{
    end_span, flush_insula_emitter, init_insula_emitter, record_point, record_timed_point,
    start_span, system_binding,
};
use akasha::migrations::{migration_pool, run_migrations};
#[cfg(test)]
use akasha::native::retention::retention_schedule;
use akasha::native::{
    INSULA_COMPONENT, INSULA_LAYER, decode_line, execute, insula_binding, needs_runtime,
    operation_name, protocol_error,
};
#[cfg(test)]
use akasha::native::{ProtocolRequest, protocol_error_class};
use akasha::{
    AppError, Config, GigaWorkerHandle, OutcomeClass, SubstrateHealthOptions, TrustedBinding,
    refresh_semantic_vocabulary, spawn_giga_worker, substrate_health_with_config,
};
#[cfg(test)]
use protocol::ProtocolError;
use std::{
    env,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Instant,
};
#[cfg(test)]
use summoning::AnamnesisWriteRequest;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

fn backup_error_class(error: &BackupError) -> &'static str {
    error.failure_code().as_str()
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

type Runtime = (Config, sqlx::PgPool, Option<GigaWorkerHandle>);

/// Pool connect, schema check, GIGA worker, then the emitter — before the
/// first request's span starts, so the first recall of every process lands
/// in Insula with its children instead of being the one nobody sees. The
/// bootstrap's own cost is the `substrate.bootstrap` point: it is inside the
/// wall time the caller waited and would otherwise be invisible.
async fn bootstrap_runtime(binding: &TrustedBinding) -> Result<Runtime, AppError> {
    let started = Instant::now();
    let config = Config::from_env()?;
    let pool = config.pool().await?;
    // enough: the GIGA worker's own claim and finish seams stay unobserved;
    // door: spawn_giga_worker in giga_worker.rs.
    let worker = spawn_giga_worker(&pool, &config)?;
    init_insula_emitter(pool.clone());
    record_timed_point(
        binding,
        INSULA_COMPONENT,
        INSULA_LAYER,
        "substrate.bootstrap",
        started.elapsed(),
        OutcomeClass::Ok,
        None,
    );
    Ok((config, pool, worker))
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
        // stderr lands in the operator's OMP terminal; a shared-cluster stall is not his to read there
        .with_env_filter("warn,sqlx::query=error")
        .init();
    let retention = tokio::spawn(akasha::native::retention::run(None, "solarisael"));
    let mut runtime: Option<Runtime> = None;
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
                let bootstrap_error = if runtime.is_none() && needs_runtime(&request) {
                    match bootstrap_runtime(&binding).await {
                        Ok(ready) => {
                            runtime = Some(ready);
                            None
                        }
                        Err(error) => Some(error),
                    }
                } else {
                    None
                };
                let span = start_span(&binding, INSULA_COMPONENT, INSULA_LAYER, operation);
                let dispatched = execute(
                    id,
                    request,
                    runtime.as_ref().map(|(config, pool, _)| (config, pool)),
                    bootstrap_error,
                    span.as_ref(),
                    akasha::native::NativeServices::default(),
                )
                .await?;
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
    retention.abort();
    if let Err(error) = retention.await {
        if !error.is_cancelled() {
            return Err(error.into());
        }
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
    fn hallway_knock_protocol_accepts_an_absent_root_parent_and_preserves_a_continuation() {
        let mut envelope = serde_json::json!({
            "protocol": 1, "id": "knock", "method": "hallway_knock",
            "params": {
                "hallway": "shared-hallway", "room": "kodo", "spirit": "Kodo",
                "session": "kodo-knock", "idempotencyKey": "root",
                "messageId": 42, "recipientRoom": "kintsu", "maxTurns": 2
            }
        });
        let (_, root) = decode_line(&envelope.to_string());
        let ProtocolRequest::HallwayKnock(root) = root.unwrap() else {
            panic!("expected root Knock");
        };
        assert_eq!(root.parent_knock_id, None);
        assert!(
            serde_json::to_value(root)
                .unwrap()
                .get("parentKnockId")
                .is_none()
        );

        let parent = "3d3051cb-aee1-4a2d-9316-15e383374f39";
        envelope["params"]["parentKnockId"] = serde_json::json!(parent);
        let (_, child) = decode_line(&envelope.to_string());
        let ProtocolRequest::HallwayKnock(child) = child.unwrap() else {
            panic!("expected child Knock");
        };
        assert_eq!(child.parent_knock_id.as_deref(), Some(parent));
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
            let class = error.insula_class();
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
            BackupError::ToolNotFound {
                tool: "pg_dump".into(),
                probed: vec![body.into()],
            },
        ] {
            let class = backup_error_class(&error);
            assert!(is_mechanical_name(class), "{class} is not mechanical");
            assert!(!class.contains("secret"), "{class} leaked a message");
        }
        assert!(is_mechanical_name(akasha::backup::POST_WRITE_OPERATION));
    }

    #[test]
    fn refusals_and_faults_are_separate_outcome_classes() {
        assert_eq!(
            AppError::Invalid("bad field".into()).insula_outcome(),
            OutcomeClass::Refused
        );
        assert_eq!(
            AppError::Refusal {
                code: "rule",
                message: "static refusal"
            }
            .insula_outcome(),
            OutcomeClass::Refused
        );
        assert_eq!(
            AppError::Database(sqlx::Error::PoolClosed).insula_outcome(),
            OutcomeClass::Error
        );
        assert_eq!(
            AppError::Config("missing".into()).insula_outcome(),
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
