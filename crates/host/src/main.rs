use host::{HostConfig, run};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("host=info")),
        )
        .with_target(false)
        .init();
    let result = HostConfig::from_env().and_then(|config| {
        config.validate()?;
        Ok(config)
    });
    let config = match result {
        Ok(config) => config,
        Err(reason) => {
            eprintln!("house-host startup refused: {reason}");
            std::process::exit(2);
        }
    };
    if let Err(reason) = run(config).await {
        eprintln!("house-host failed: {reason}");
        std::process::exit(1);
    }
}
