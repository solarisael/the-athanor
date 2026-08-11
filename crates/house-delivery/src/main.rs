use anyhow::{Context, Result, bail};
use athanor_house_delivery::{CONFIGURATION_CONTRACT, DeliveryService};
use serde_json::json;
use std::{env, time::Duration};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const DEFAULT_NATS_URL: &str = "nats://127.0.0.1:4222";
const ONCE_WAIT: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let command = env::args().nth(1).unwrap_or_else(|| "help".to_owned());
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{CONFIGURATION_CONTRACT}");
        return Ok(());
    }

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let nats_url = env::var("SOLARISAEL_NATS_URL").unwrap_or_else(|_| DEFAULT_NATS_URL.to_owned());
    let lease_owner = match env::var("SOLARISAEL_DELIVERY_INSTANCE_ID") {
        Ok(value) => value
            .parse::<Uuid>()
            .context("SOLARISAEL_DELIVERY_INSTANCE_ID must be a UUID")?,
        Err(env::VarError::NotPresent) => Uuid::new_v4(),
        Err(error) => return Err(error.into()),
    };
    let service = DeliveryService::connect(&database_url, &nats_url, lease_owner).await?;

    match command.as_str() {
        "configure" => print_json(json!({
            "ok": true,
            "authority": "postgresql",
            "delivery": "nats-jetstream",
            "stream": athanor_house_delivery::broker::STREAM_NAME,
            "subject": athanor_house_delivery::broker::SUBJECT,
            "consumer": athanor_house_delivery::broker::CONSUMER_NAME,
        }))?,
        "publish-once" => print_json(serde_json::to_value(service.publish_once().await?)?)?,
        "consume-once" => print_json(serde_json::to_value(
            service.consume_once(ONCE_WAIT).await?,
        )?)?,
        "once" => print_json(serde_json::to_value(service.once(ONCE_WAIT).await?)?)?,
        "health" => print_json(serde_json::to_value(service.health().await?)?)?,
        "run" => {
            tracing::info!(
                instance_id = %lease_owner,
                stream = athanor_house_delivery::broker::STREAM_NAME,
                subject = athanor_house_delivery::broker::SUBJECT,
                "starting PostgreSQL-authoritative boat.ready delivery"
            );
            tokio::select! {
                result = service.run() => result?,
                signal = tokio::signal::ctrl_c() => signal.context("listen for shutdown signal")?,
            }
        }
        unknown => bail!("unknown command {unknown:?}. {CONFIGURATION_CONTRACT}"),
    }
    Ok(())
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}
