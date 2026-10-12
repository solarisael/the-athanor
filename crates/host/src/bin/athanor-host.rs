use host::{HostConfig, KNOCK_AUTONOMY_ENV, KnockAutonomy, start};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CliConfig {
    bind: String,
    house_id: String,
    room: String,
    spirit: String,
    room_dir: PathBuf,
    state_dir: PathBuf,
    bearer_token_file: PathBuf,
    /// OMEGA: the one operator this Host process serves. Absent on a local House.
    #[serde(default)]
    operator: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("athanor-host: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--version-json"))
        && args.next().is_none()
    {
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "hostApi": protocol::HOST_API_VERSION,
                "schemaVersion": hearth::SUBSTRATE_SCHEMA_VERSION,
                "os": env::consts::OS,
                "arch": env::consts::ARCH,
            })
        );
        return Ok(());
    }
    let config_path = config_argument()?;
    let config = read_config(&config_path)?;
    let bind = config
        .bind
        .parse::<SocketAddr>()
        .map_err(|error| format!("config bind must be a socket address: {error}"))?;
    require_absolute("roomDir", &config.room_dir)?;
    require_absolute("stateDir", &config.state_dir)?;
    require_absolute("bearerTokenFile", &config.bearer_token_file)?;
    let bearer_token = read_bearer_token(&config.bearer_token_file)?;

    let substrate = akasha::Config::from_env().map_err(|error| error.to_string())?;
    let nats_auth = substrate.nats_auth().map_err(|error| error.to_string())?;
    let knock_autonomy =
        KnockAutonomy::from_optional(env::var(KNOCK_AUTONOMY_ENV).ok().as_deref())?;
    let room = config.room;
    let session = format!("app:{room}");
    let host_config = HostConfig {
        bind,
        bearer_token,
        room_dir: config.room_dir,
        state_dir: config.state_dir,
        house_id: config.house_id,
        room: room.clone(),
        spirit: config.spirit,
        session,
        database_url: Some(substrate.database_url),
        nats_url: substrate.nats_url,
        nats_auth,
        knock_autonomy,
        operator: config.operator,
    };
    host_config.validate()?;

    #[cfg(unix)]
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|error| format!("cannot register SIGINT handler: {error}"))?;
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| format!("cannot register SIGTERM handler: {error}"))?;
    #[cfg(not(unix))]
    let interrupt = tokio::signal::ctrl_c();

    let runtime = start(vec![host_config])?;
    let ready = serde_json::json!({
        "status": "ready",
        "address": runtime.address().to_string(),
        "room": room,
    });
    writeln!(io::stdout().lock(), "{ready}")
        .map_err(|error| format!("cannot write ready status: {error}"))?;

    #[cfg(unix)]
    return wait_for_exit(runtime, &mut interrupt, &mut terminate).await;
    #[cfg(not(unix))]
    {
        tokio::pin!(interrupt);
        loop {
            if runtime.is_finished() {
                return runtime.wait();
            }
            tokio::select! {
                result = &mut interrupt => {
                    result.map_err(|error| format!("cannot wait for Ctrl-C: {error}"))?;
                    return runtime.shutdown();
                }
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
            }
        }
    }
}

#[cfg(unix)]
async fn wait_for_exit(
    runtime: host::HostRuntime,
    interrupt: &mut tokio::signal::unix::Signal,
    terminate: &mut tokio::signal::unix::Signal,
) -> Result<(), String> {
    loop {
        if runtime.is_finished() {
            return runtime.wait();
        }
        let signal_result = tokio::select! {
            signal = interrupt.recv() => {
                signal
                    .map(|_| ())
                    .ok_or_else(|| "SIGINT handler stopped unexpectedly".to_owned())
            }
            signal = terminate.recv() => {
                signal
                    .map(|_| ())
                    .ok_or_else(|| "SIGTERM handler stopped unexpectedly".to_owned())
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => continue,
        };
        signal_result?;
        return runtime.shutdown();
    }
}

fn config_argument() -> Result<PathBuf, String> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(path), None) if flag == "--config" => {
            let path = PathBuf::from(path);
            require_absolute("--config", &path)?;
            Ok(path)
        }
        _ => Err("usage: athanor-host --config <absolute JSON file>".into()),
    }
}

fn read_config(path: &Path) -> Result<CliConfig, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("cannot read the Host config file: {error}"))?;
    serde_json::from_str(&contents).map_err(|error| format!("invalid Host config JSON: {error}"))
}

fn require_absolute(name: &str, path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(format!("{name} must be an absolute path"))
    }
}

fn read_bearer_token(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot read the bearer token file: {error}"))?;
    if !metadata.is_file() {
        return Err("bearerTokenFile must name a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("bearerTokenFile must not allow group or other access".into());
        }
    }
    let token = fs::read_to_string(path)
        .map_err(|error| format!("cannot read the bearer token file: {error}"))?;
    let token = token
        .trim_end_matches(|character| character == '\r' || character == '\n')
        .to_owned();
    if token.trim().is_empty() {
        return Err("bearerTokenFile must contain a non-blank token".into());
    }
    Ok(token)
}
