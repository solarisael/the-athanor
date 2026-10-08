use super::substrate_config;
use crate::HostConfig;
use akasha::{AppError, GigaEnablement, GigaWorkerHandle};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct GigaRoomWorker {
    enablement: Option<GigaEnablement>,
    worker: Option<GigaWorkerHandle>,
    closing: bool,
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
        let running = self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished());
        if self.enablement == Some(enablement) && running == enablement.classifier_enabled() {
            return Ok(self.status());
        }
        if let Some(worker) = self.worker.take() {
            worker.shutdown().await;
        }
        self.enablement = Some(enablement);
        if enablement.classifier_enabled() {
            let pool = pool.ok_or_else(|| {
                AppError::Config("GIGA requires the bound Host AKASHA runtime".into())
            })?;
            akasha::Config::validate_pool(pool).await?;
            let config = substrate_config(host)?;
            self.worker = akasha::spawn_room_giga_worker(pool, &config, enablement)?;
        }
        Ok(self.status())
    }

    fn status(&self) -> Value {
        let enablement = self.enablement();
        json!({
            "ok": true,
            "capture_enabled": enablement.capture_enabled(),
            "classifier_enabled": enablement.classifier_enabled(),
            "worker_running": self.worker.as_ref().is_some_and(|worker| !worker.is_finished()),
        })
    }

    async fn close(&mut self) {
        self.closing = true;
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
                .set_enablement(&host, pool.as_ref(), GigaEnablement::from_env())
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

pub(crate) async fn admit_ingest(
    host: &HostConfig,
    pool: Option<&PgPool>,
    worker: &Mutex<GigaRoomWorker>,
    enablement: Option<GigaEnablement>,
) -> Result<(), AppError> {
    let mut worker = worker.lock().await;
    let enablement = enablement.unwrap_or_else(|| worker.enablement());
    if enablement.replay_mode {
        return Err(AppError::Refusal {
            code: "giga_replay_refused",
            message: "replay cannot ingest GIGA events",
        });
    }
    worker.set_enablement(host, pool, enablement).await?;
    if !enablement.capture_enabled() {
        return Err(AppError::Refusal {
            code: "giga_disabled",
            message: "GIGA capture is disabled",
        });
    }
    Ok(())
}
