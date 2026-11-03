use super::enablement::{GigaEnablement, claim_owner_enabled};
use super::process::giga_process_with_enablement;
use super::typesafe::GigaClassifierConfig;
use crate::{
    AppError, Config,
    giga::{
        GIGA_MAX_BATCH_TURNS, GIGA_MAX_BATCH_WAIT_SECONDS, giga_event_claim,
        giga_event_deadline_settle, giga_turn_release,
    },
};
use hearth::{GigaEventClaimReceipt, GigaEventClaimRequest, RoomKey};
use sqlx::PgPool;
use std::{env, sync::Arc, time::Duration};
use tokio::{
    sync::watch,
    task::{JoinError, JoinHandle, JoinSet},
};
use uuid::Uuid;

const GIGA_LEASE_SECONDS: u32 = 300;
const GIGA_POLL_INTERVAL: Duration = Duration::from_secs(1);
// A job and its deadline settlement both end before the 300-second lease, so
// a lease is never reclaimed while its job can still write.
const GIGA_JOB_DEADLINE: Duration = Duration::from_secs(120);
const GIGA_SETTLE_DEADLINE: Duration = Duration::from_secs(30);
const GIGA_MAX_CONCURRENCY: u32 = 16;

/// Batch release and capacity for one room worker, read once when it starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GigaQueueSettings {
    batch_turns: u32,
    batch_wait_seconds: u32,
    concurrency: usize,
}

impl GigaQueueSettings {
    fn from_env() -> Result<Self, AppError> {
        // A value that is not Unicode reads as empty and is refused as invalid.
        Self::from_lookup(|key| match env::var(key) {
            Ok(value) => Some(value),
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => Some(String::new()),
        })
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, AppError> {
        let setting = |key: &str, default: u32, max: u32| match lookup(key) {
            None => Ok(default),
            Some(value) => value
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|value| (1..=max).contains(value))
                .ok_or_else(|| {
                    AppError::Config(format!("{key} must be an integer from 1 to {max}"))
                }),
        };
        Ok(Self {
            batch_turns: setting("ATHANOR_GIGA_BATCH_TURNS", 8, GIGA_MAX_BATCH_TURNS)?,
            batch_wait_seconds: setting(
                "ATHANOR_GIGA_BATCH_WAIT_SECONDS",
                120,
                GIGA_MAX_BATCH_WAIT_SECONDS,
            )?,
            concurrency: setting("ATHANOR_GIGA_CONCURRENCY", 4, GIGA_MAX_CONCURRENCY)? as usize,
        })
    }
}

pub struct GigaWorkerHandle {
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl GigaWorkerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        if let Err(error) = self.task.await {
            tracing::error!(
                panicked = error.is_panic(),
                cancelled = error.is_cancelled(),
                "giga_worker_shutdown_failed"
            );
        }
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

/// Classifies one claimed turn job under the hard job deadline. After the
/// deadline the attempt is settled only while this claim still owns its lease.
async fn run_claimed_job(
    pool: PgPool,
    config: Arc<Config>,
    classifier: Arc<GigaClassifierConfig>,
    enablement: GigaEnablement,
    claim: GigaEventClaimReceipt,
) {
    let job = giga_process_with_enablement(&pool, &config, &claim, enablement, Some(&*classifier));
    match tokio::time::timeout(GIGA_JOB_DEADLINE, job).await {
        Ok(Ok(_)) => return,
        Ok(Err(error)) => {
            tracing::warn!(operation = "giga_worker", error = %error);
            return;
        }
        Err(_) => {}
    }
    match tokio::time::timeout(
        GIGA_SETTLE_DEADLINE,
        giga_event_deadline_settle(&pool, &claim),
    )
    .await
    {
        Ok(Ok(Some(receipt))) => tracing::warn!(
            operation = "giga_worker_deadline",
            error_class = "GigaWorkerDeadlineExceeded",
            queue_state = ?receipt.queue_state(),
            attempt_count = receipt.attempt_count(),
            "GIGA job exceeded its deadline; attempt settled"
        ),
        Ok(Ok(None)) => tracing::warn!(
            operation = "giga_worker_deadline",
            error_class = "GigaWorkerDeadlineExceeded",
            "GIGA job exceeded its deadline after its lease was already settled"
        ),
        Ok(Err(error)) => tracing::warn!(operation = "giga_worker_deadline", error = %error),
        Err(_) => tracing::warn!(
            operation = "giga_worker_deadline",
            error_class = "GigaWorkerDeadlineExceeded",
            "GIGA deadline settlement timed out"
        ),
    }
}

fn report_job(joined: Result<(), JoinError>) {
    if let Err(error) = joined {
        if error.is_panic() {
            tracing::error!(operation = "giga_worker", "giga_job_panicked");
        }
    }
}

async fn giga_worker_loop(
    pool: PgPool,
    config: Config,
    classifier: GigaClassifierConfig,
    room: RoomKey,
    enablement: GigaEnablement,
    settings: GigaQueueSettings,
    mut shutdown: watch::Receiver<bool>,
) {
    let config = Arc::new(config);
    let classifier = Arc::new(classifier);
    // Every claim gets its own worker id, so a job can only ever finish or
    // settle the exact lease it was granted.
    let worker_id = format!("rust-hippocampus:{}", Uuid::new_v4());
    let mut claims: u64 = 0;
    let mut jobs = JoinSet::new();
    'admission: loop {
        if *shutdown.borrow() {
            break;
        }
        if !classifier.grant_is_current().await {
            tracing::warn!(
                operation = "giga_worker",
                error_class = "GigaTypesafeConfigurationError",
                "GIGA worker stopped admission: room grant changed"
            );
            break;
        }
        if let Err(error) = giga_turn_release(
            &pool,
            &room,
            settings.batch_turns,
            settings.batch_wait_seconds,
        )
        .await
        {
            tracing::warn!(operation = "giga_worker_release", error = %error);
        }
        while jobs.len() < settings.concurrency {
            claims += 1;
            let request = match GigaEventClaimRequest::new(
                room.clone(),
                format!("{worker_id}:{claims}"),
                GIGA_LEASE_SECONDS,
            ) {
                Ok(request) => request,
                Err(error) => {
                    tracing::warn!(operation = "giga_worker", error = %error);
                    break 'admission;
                }
            };
            let claim = tokio::select! {
                _ = shutdown.changed() => break 'admission,
                claim = giga_event_claim(&pool, request) => claim,
            };
            match claim {
                Ok(claim) if claim.event().is_some() => {
                    jobs.spawn(run_claimed_job(
                        pool.clone(),
                        Arc::clone(&config),
                        Arc::clone(&classifier),
                        enablement,
                        claim,
                    ));
                }
                Ok(_) => break,
                Err(error) => {
                    tracing::warn!(operation = "giga_worker_claim", error = %error);
                    break;
                }
            }
        }
        tokio::select! {
            _ = shutdown.changed() => break,
            Some(joined) = jobs.join_next() => report_job(joined),
            _ = tokio::time::sleep(GIGA_POLL_INTERVAL) => {}
        }
    }
    // Admission has stopped; cancel and join every owned job before returning.
    jobs.abort_all();
    while let Some(joined) = jobs.join_next().await {
        report_job(joined);
    }
}

pub fn spawn_giga_worker(
    pool: &PgPool,
    config: &Config,
) -> Result<Option<GigaWorkerHandle>, AppError> {
    if !claim_owner_enabled() {
        return Ok(None);
    }
    let classifier = match GigaClassifierConfig::from_env(config) {
        Ok(classifier) => classifier,
        Err(_) => {
            tracing::warn!(
                error_class = "GigaTypesafeConfigurationError",
                operation = "giga_worker_bootstrap",
                "giga_classifier_unavailable"
            );
            return Ok(None);
        }
    };
    spawn_room_giga_worker(pool, config, GigaEnablement::from_env(), classifier)
}
pub fn spawn_room_giga_worker(
    pool: &PgPool,
    config: &Config,
    enablement: GigaEnablement,
    classifier: GigaClassifierConfig,
) -> Result<Option<GigaWorkerHandle>, AppError> {
    if !enablement.classifier_enabled() {
        return Ok(None);
    }
    let room = config
        .giga_source_room
        .as_deref()
        .ok_or_else(|| AppError::Config("enabled GIGA worker requires a source room".into()))?;
    let room = RoomKey::new(room)
        .map_err(|error| AppError::Config(format!("invalid GIGA source room: {error}")))?;
    let settings = GigaQueueSettings::from_env()?;
    let (shutdown, receiver) = watch::channel(false);
    let task = tokio::spawn(giga_worker_loop(
        pool.clone(),
        config.clone(),
        classifier,
        room,
        enablement,
        settings,
        receiver,
    ));
    Ok(Some(GigaWorkerHandle { shutdown, task }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(values: &[(&str, &str)]) -> Result<GigaQueueSettings, AppError> {
        GigaQueueSettings::from_lookup(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn queue_settings_default_and_accept_their_bounds() {
        assert_eq!(
            settings(&[]).unwrap(),
            GigaQueueSettings {
                batch_turns: 8,
                batch_wait_seconds: 120,
                concurrency: 4,
            }
        );
        assert_eq!(
            settings(&[
                ("ATHANOR_GIGA_BATCH_TURNS", "64"),
                ("ATHANOR_GIGA_BATCH_WAIT_SECONDS", "1"),
                ("ATHANOR_GIGA_CONCURRENCY", " 16 "),
            ])
            .unwrap(),
            GigaQueueSettings {
                batch_turns: 64,
                batch_wait_seconds: 1,
                concurrency: 16,
            }
        );
    }

    #[test]
    fn queue_settings_refuse_out_of_range_or_malformed_values() {
        for (key, value) in [
            ("ATHANOR_GIGA_BATCH_TURNS", "0"),
            ("ATHANOR_GIGA_BATCH_TURNS", "65"),
            ("ATHANOR_GIGA_BATCH_WAIT_SECONDS", "0"),
            ("ATHANOR_GIGA_BATCH_WAIT_SECONDS", "3601"),
            ("ATHANOR_GIGA_CONCURRENCY", "0"),
            ("ATHANOR_GIGA_CONCURRENCY", "17"),
            ("ATHANOR_GIGA_CONCURRENCY", "four"),
            ("ATHANOR_GIGA_BATCH_TURNS", ""),
            ("ATHANOR_GIGA_BATCH_TURNS", "-8"),
        ] {
            assert!(
                matches!(settings(&[(key, value)]), Err(AppError::Config(_))),
                "{key}={value:?} must be refused"
            );
        }
    }
}
