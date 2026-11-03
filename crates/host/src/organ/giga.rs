use super::substrate_config;
use crate::HostConfig;
use akasha::{AppError, GigaClassifierConfig, GigaEnablement, GigaWorkerHandle};
use hearth::conversation::LoggedTurn;
use protocol::{
    ConversationGigaCapture, GIGA_CONVERSATION_INGEST_MAX_TURNS, GigaConversationIngestParams,
    GigaConversationIngestResult, GigaConversationTurnParams,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const INGEST_METHOD: &str = "giga_conversation_ingest";

#[derive(Default)]
pub(crate) struct GigaRoomWorker {
    enablement: Option<GigaEnablement>,
    worker: Option<GigaWorkerHandle>,
    closing: bool,
    classifier: Option<GigaClassifierConfig>,
    classifier_requested: bool,
    unavailable: Option<&'static str>,
    rejected_policy_revision: Option<String>,
}

impl GigaRoomWorker {
    pub(crate) fn enablement(&self) -> GigaEnablement {
        self.enablement.unwrap_or_else(GigaEnablement::from_env)
    }

    pub(crate) async fn set_enablement(
        &mut self,
        host: &HostConfig,
        pool: Option<&PgPool>,
        enablement: GigaEnablement,
        credential: Option<String>,
    ) -> Result<Value, AppError> {
        if enablement.replay_mode {
            return Err(AppError::Refusal {
                code: "giga_replay_refused",
                message: "replay cannot change a shared GIGA producer",
            });
        }
        if self.closing {
            return Err(AppError::Refusal {
                code: "giga_closed",
                message: "the Host GIGA lifecycle is closing",
            });
        }
        self.classifier_requested = enablement.classifier_enabled();
        let revision = if self.classifier_requested {
            crate::judgment::giga_policy(&host.room_dir, &host.room).await
        } else {
            None
        };
        let running = self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished());
        let same_policy = self
            .classifier
            .as_ref()
            .is_some_and(|classifier| Some(classifier.policy_revision()) == revision.as_deref());

        if credential.is_none() && same_policy && running && self.enablement == Some(enablement) {
            self.unavailable = None;
            return Ok(self.status());
        }
        let classifier = if self.classifier_requested {
            match (revision, credential) {
                (Some(revision), Some(credential)) => {
                    let attempted_revision = revision.clone();
                    // The trusted Host operator scopes Recall; None keeps the
                    // local House Recall path. Never the marker or transcript.
                    let operator = host
                        .operator
                        .as_deref()
                        .map(hearth::OperatorKey::new)
                        .transpose();
                    match (
                        operator,
                        GigaClassifierConfig::new(
                            credential,
                            revision,
                            host.room_dir.clone(),
                            host.room.clone(),
                        ),
                    ) {
                        (Ok(operator), Ok(classifier)) => {
                            self.unavailable = None;
                            self.rejected_policy_revision = None;
                            Some(classifier.with_operator(operator))
                        }
                        _ => {
                            self.unavailable = Some("classifier_config_invalid");
                            self.rejected_policy_revision = Some(attempted_revision);
                            None
                        }
                    }
                }
                (Some(_), None) if same_policy => self.classifier.clone(),
                (Some(revision), None)
                    if self.rejected_policy_revision.as_deref() == Some(&revision) =>
                {
                    self.unavailable = Some("classifier_config_invalid");
                    None
                }
                (None, _) => {
                    self.unavailable = Some("room_grant_required");
                    None
                }
                _ => {
                    self.unavailable = Some("credential_required");
                    None
                }
            }
        } else {
            self.unavailable = None;
            self.rejected_policy_revision = None;
            None
        };
        let enablement = GigaEnablement {
            hippocampus_enabled: classifier.is_some(),
            ..enablement
        };
        if let Some(worker) = self.worker.take() {
            worker.shutdown().await;
        }
        self.enablement = Some(enablement);
        self.classifier = classifier.clone();
        if let Some(classifier) = classifier {
            let pool = pool.ok_or_else(|| {
                AppError::Config("GIGA requires the bound Host AKASHA runtime".into())
            })?;
            akasha::Config::validate_pool(pool).await?;
            let config = substrate_config(host)?;
            self.worker = akasha::spawn_room_giga_worker(pool, &config, enablement, classifier)?;
        }
        Ok(self.status())
    }

    pub(super) fn status(&self) -> Value {
        let enablement = self.enablement();
        json!({
            "ok": true,
            "capture_enabled": enablement.capture_enabled(),
            "classifier_enabled": enablement.classifier_enabled(),
            "classifier_requested": self.classifier_requested,
            "classifier_unavailable": self.unavailable,
            "credential_required": self.unavailable == Some("credential_required"),
            "worker_running": self.worker.as_ref().is_some_and(|worker| !worker.is_finished()),
        })
    }

    async fn close(&mut self) {
        self.closing = true;
        self.classifier = None;
        if let Some(worker) = self.worker.take() {
            worker.shutdown().await;
        }
    }
}

pub(crate) async fn serve(
    host: HostConfig,
    pool: Option<PgPool>,
    worker: Arc<Mutex<GigaRoomWorker>>,
    cancellation: CancellationToken,
) {
    {
        let mut worker = worker.lock().await;
        if worker.enablement.is_none() {
            if let Err(error) = worker
                .set_enablement(&host, pool.as_ref(), GigaEnablement::from_env(), None)
                .await
            {
                tracing::warn!(
                    room = host.room,
                    error_class = error.insula_class(),
                    "Host GIGA worker is unavailable"
                );
            }
        }
    }
    cancellation.cancelled().await;
    worker.lock().await.close().await;
}

/// Persist the GIGA turn jobs of one captured window before the capture reply.
/// Only turns with private ledger evidence are eligible; the receipt counts
/// every turn that did not become a job.
pub(crate) async fn capture(
    host: &HostConfig,
    pool: Option<&PgPool>,
    worker: &Mutex<GigaRoomWorker>,
    request: &ConversationGigaCapture,
    persisted: bool,
    turns: &[LoggedTurn],
    ledger_failures: usize,
) -> Value {
    let skipped = if request.replay_mode {
        Some("replay")
    } else if request.subagent {
        Some("subagent")
    } else if !request.giga_enabled {
        Some("disabled")
    } else if !persisted {
        Some("not_persisted")
    } else {
        None
    };
    if let Some(reason) = skipped {
        return receipt(
            "skipped",
            Some(reason),
            0,
            ledger_failures,
            Ingested::default(),
            Value::Null,
        );
    }
    let enablement = GigaEnablement {
        giga_enabled: true,
        hippocampus_enabled: request.hippocampus_enabled,
        replay_mode: false,
    };
    let classifier = classifier_status(host, pool, worker, enablement).await;
    let jobs: Vec<GigaConversationTurnParams> = turns
        .iter()
        .map(|turn| GigaConversationTurnParams {
            role: turn.role.clone(),
            source_id: turn.source_id.clone(),
            content_hash: turn.content_hash.clone(),
            session_id: turn.session_id.clone(),
            timestamp: turn.source_timestamp.clone(),
            has_stable_id: turn.has_stable_id,
        })
        .collect();
    let mut ingested = Ingested::default();
    match pool {
        Some(pool) => {
            ingest(
                pool,
                &host.room,
                &request.project_keys,
                &jobs,
                &mut ingested,
            )
            .await
        }
        None if !jobs.is_empty() => ingested.fail(
            jobs.len(),
            &AppError::Config("GIGA capture requires the bound Host AKASHA runtime".into()),
        ),
        None => {}
    }
    let stored = ingested.accepted + ingested.duplicate;
    let status = match (ingested.failed + ledger_failures, stored) {
        (0, _) => "captured",
        (_, 0) => "failed",
        _ => "partial",
    };
    receipt(
        status,
        None,
        jobs.len(),
        ledger_failures,
        ingested,
        classifier,
    )
}

/// Apply the capture's switches to the room producer. Classifier trouble is
/// reported beside the capture and never decides it.
async fn classifier_status(
    host: &HostConfig,
    pool: Option<&PgPool>,
    worker: &Mutex<GigaRoomWorker>,
    enablement: GigaEnablement,
) -> Value {
    let mut worker = worker.lock().await;
    match worker.set_enablement(host, pool, enablement, None).await {
        Ok(status) => status,
        Err(error) => {
            let body = error.protocol_error_body("giga_set_enablement");
            let mut status = worker.status();
            status["classifier_error"] =
                json!({ "code": body.code, "message": body.message, "retryable": body.retryable });
            status
        }
    }
}

/// Each ingest call is atomic, so one refused identity fails its whole chunk.
/// Such a chunk is resent turn by turn and only the refused turn stays out. A
/// store failure stops the capture and counts every remaining turn failed.
async fn ingest(
    pool: &PgPool,
    room: &str,
    project_keys: &[String],
    jobs: &[GigaConversationTurnParams],
    ingested: &mut Ingested,
) {
    for (index, chunk) in jobs.chunks(GIGA_CONVERSATION_INGEST_MAX_TURNS).enumerate() {
        let start = index * GIGA_CONVERSATION_INGEST_MAX_TURNS;
        match ingest_call(pool, room, project_keys, chunk).await {
            Ok(result) => ingested.record(result),
            Err(error) if refused(&error) && chunk.len() > 1 => {
                for (offset, job) in chunk.iter().enumerate() {
                    match ingest_call(pool, room, project_keys, std::slice::from_ref(job)).await {
                        Ok(result) => ingested.record(result),
                        Err(error) if refused(&error) => ingested.fail(1, &error),
                        Err(error) => return ingested.fail(jobs.len() - start - offset, &error),
                    }
                }
            }
            Err(error) if refused(&error) => ingested.fail(chunk.len(), &error),
            Err(error) => return ingested.fail(jobs.len() - start, &error),
        }
    }
}

async fn ingest_call(
    pool: &PgPool,
    room: &str,
    project_keys: &[String],
    jobs: &[GigaConversationTurnParams],
) -> Result<GigaConversationIngestResult, AppError> {
    akasha::giga_conversation_ingest(
        pool,
        GigaConversationIngestParams {
            room: room.to_owned(),
            project_keys: project_keys.to_vec(),
            turns: jobs.to_vec(),
        },
    )
    .await
}

/// A refusal is a verdict on the submitted turns; anything else is the store
/// failing at them.
fn refused(error: &AppError) -> bool {
    matches!(error, AppError::Invalid(_) | AppError::Refusal { .. })
}

#[derive(Default)]
struct Ingested {
    accepted: usize,
    duplicate: usize,
    failed: usize,
    event_ids: Vec<String>,
    errors: Vec<CaptureError>,
}

struct CaptureError {
    code: String,
    message: String,
    retryable: bool,
    turns: usize,
}

impl Ingested {
    /// Event IDs are the distinct live owner events, in first-seen order; one
    /// owner may span chunks.
    fn record(&mut self, result: GigaConversationIngestResult) {
        self.accepted += result.accepted_turns;
        self.duplicate += result.duplicate_turns;
        for event_id in result.event_ids {
            if !self.event_ids.contains(&event_id) {
                self.event_ids.push(event_id);
            }
        }
    }

    /// Repeated failures of one kind fold into one entry with a turn count.
    fn fail(&mut self, turns: usize, error: &AppError) {
        let body = error.protocol_error_body(INGEST_METHOD);
        self.failed += turns;
        match self
            .errors
            .iter_mut()
            .find(|known| known.code == body.code && known.message == body.message)
        {
            Some(known) => known.turns += turns,
            None => self.errors.push(CaptureError {
                code: body.code,
                message: body.message,
                retryable: body.retryable,
                turns,
            }),
        }
    }
}

fn receipt(
    status: &str,
    reason: Option<&str>,
    submitted: usize,
    ledger_failures: usize,
    ingested: Ingested,
    classifier: Value,
) -> Value {
    let errors: Vec<Value> = ingested
        .errors
        .into_iter()
        .map(|error| {
            json!({
                "code": error.code,
                "message": error.message,
                "retryable": error.retryable,
                "turns": error.turns,
            })
        })
        .collect();
    json!({
        "status": status,
        "reason": reason,
        "submittedTurns": submitted,
        "acceptedTurns": ingested.accepted,
        "duplicateTurns": ingested.duplicate,
        "ledgerFailures": ledger_failures,
        "eventIds": ingested.event_ids,
        "errors": errors,
        "classifierStatus": classifier,
    })
}
