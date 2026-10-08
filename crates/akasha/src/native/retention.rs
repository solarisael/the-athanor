use crate as akasha;
use crate::Config;
use chrono::{DateTime, Timelike, Utc};

const RETENTION_INITIAL_DELAY: std::time::Duration = std::time::Duration::from_secs(5 * 60);
const RETENTION_CADENCE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

pub fn retention_schedule() -> (std::time::Duration, std::time::Duration) {
    (RETENTION_INITIAL_DELAY, RETENTION_CADENCE)
}

fn retention_cutoff(now: DateTime<Utc>) -> DateTime<Utc> {
    // Raw expiry already includes retention; the sweep must not subtract it again.
    now.with_second(0)
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

pub async fn run(shared_pool: Option<sqlx::PgPool>, house_id: &str) {
    // This is idempotent maintenance, not a heartbeat or monitor: retention
    // receipts make scheduled sweeps replay-safe without asserting liveness.
    let mut binding = akasha::insula_writer::system_binding();
    binding.house_id = house_id.to_owned();
    let (initial_delay, cadence) = retention_schedule();
    tokio::time::sleep(initial_delay).await;
    let mut ticker = tokio::time::interval(cadence);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // enough: fixed 24h cadence; Docket standing-intent scheduling when it exists
    loop {
        ticker.tick().await;
        let pool = if let Some(pool) = shared_pool.as_ref() {
            Config::validate_pool(pool).await.map(|()| pool.clone())
        } else {
            match Config::from_env() {
                Ok(config) => config.pool().await,
                Err(error) => Err(error),
            }
        };
        let pool = match pool {
            Ok(pool) => pool,
            Err(error) => {
                tracing::warn!(
                    error_class = error.insula_class(),
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

        let span =
            akasha::insula_writer::start_span(&binding, "akasha", "substrate", "retention_sweep");
        match akasha::run_retention(
            &pool,
            house_id,
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
}
