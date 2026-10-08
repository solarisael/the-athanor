use super::enablement::{GigaEnablement, claim_owner_enabled};
use super::process::giga_process_with_enablement;
use crate::{AppError, Config, giga::giga_event_claim};
use hearth::{GigaEventClaimRequest, RoomKey};
use sqlx::PgPool;
use std::{sync::LazyLock, time::Duration};
use tokio::{sync::watch, task::JoinHandle};
use uuid::Uuid;

const GIGA_LEASE_SECONDS: u32 = 300;
const GIGA_POLL_INTERVAL: Duration = Duration::from_secs(1);

static GIGA_WORKER_ID: LazyLock<String> =
    LazyLock::new(|| format!("rust-hippocampus:{}", Uuid::new_v4()));

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

pub(super) async fn giga_worker_loop(
    pool: PgPool,
    config: Config,
    room: RoomKey,
    enablement: GigaEnablement,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            return;
        }
        let request = match GigaEventClaimRequest::new(
            room.clone(),
            GIGA_WORKER_ID.to_string(),
            GIGA_LEASE_SECONDS,
        ) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(operation = "giga_worker", error = %error);
                return;
            }
        };
        let claim = tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                return;
            }
            claim = giga_event_claim(&pool, request) => claim,
        };
        match claim {
            Ok(claim) if claim.event().is_some() => {
                let processed = tokio::select! {
                    changed = shutdown.changed() => {
                        let _ = changed;
                        return;
                    }
                    processed = giga_process_with_enablement(&pool, &config, &claim, enablement) => processed,
                };
                if let Err(error) = processed {
                    tracing::warn!(operation = "giga_worker", error = %error);
                }
                continue;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(operation = "giga_worker_claim", error = %error);
            }
        }
        tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                return;
            }
            _ = tokio::time::sleep(GIGA_POLL_INTERVAL) => {}
        }
    }
}

pub fn spawn_giga_worker(
    pool: &PgPool,
    config: &Config,
) -> Result<Option<GigaWorkerHandle>, AppError> {
    if !claim_owner_enabled() {
        return Ok(None);
    }
    spawn_room_giga_worker(pool, config, GigaEnablement::from_env())
}

pub fn spawn_room_giga_worker(
    pool: &PgPool,
    config: &Config,
    enablement: GigaEnablement,
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
    let (shutdown, receiver) = watch::channel(false);
    let task = tokio::spawn(giga_worker_loop(
        pool.clone(),
        config.clone(),
        room,
        enablement,
        receiver,
    ));
    Ok(Some(GigaWorkerHandle { shutdown, task }))
}
