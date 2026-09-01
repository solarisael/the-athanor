use crate::{
    boundaries::OsSecrets,
    harness::{ControlServer, HarnessOwner, HarnessRegistry, control_token, registry_path},
    installer::RuntimeSecrets,
    layout::InstallLayout,
    supervisor::RuntimeConfig,
};
use anyhow::{Context, Result};
use protocol::LOOPBACK_HOST;
use std::{
    env, fs,
    io::{self, Write},
    net::SocketAddr,
    sync::Arc,
};

fn installed_runtime(layout: &InstallLayout) -> Result<(RuntimeConfig, RuntimeSecrets)> {
    let config: RuntimeConfig = serde_json::from_slice(
        &fs::read(layout.config())
            .with_context(|| format!("read {}", layout.config().display()))?,
    )?;
    config.validate()?;
    let secrets = serde_json::from_slice(
        &fs::read(layout.secrets())
            .with_context(|| format!("read {}", layout.secrets().display()))?,
    )?;
    Ok((config, secrets))
}

fn host_configs(
    layout: &InstallLayout,
    config: &RuntimeConfig,
    secrets: &RuntimeSecrets,
) -> Result<Vec<host::HostConfig>> {
    let bind: SocketAddr = format!("{LOOPBACK_HOST}:{}", config.host_port).parse()?;
    let database_url = secrets.database_url();
    let nats_url = format!("nats://{}:{}", config.nats_host, config.nats_port);
    let knock_autonomy =
        host::KnockAutonomy::from_optional(env::var(host::KNOCK_AUTONOMY_ENV).ok().as_deref())
            .map_err(anyhow::Error::msg)?;
    Ok(config
        .rooms
        .iter()
        .map(|room| host::HostConfig {
            bind,
            bearer_token: secrets.host_token.clone(),
            room_dir: config.rooms_root.join(&room.room),
            state_dir: layout.host_state().join(&room.room),
            house_id: config.house_id.clone(),
            room: room.room.clone(),
            spirit: room.spirit.clone(),
            session: format!("app:{}", room.room),
            database_url: Some(database_url.clone()),
            nats_url: Some(nats_url.clone()),
            knock_autonomy: knock_autonomy.clone(),
        })
        .collect())
}

pub fn run() -> Result<()> {
    let layout = InstallLayout::from_environment()?;
    let (config, secrets) = installed_runtime(&layout)?;
    let registry = registry_path(&layout);
    let owner = Arc::new(HarnessOwner::new(
        HarnessRegistry::load(&registry)?,
        control_token(&OsSecrets)?,
        layout.program.clone(),
        config.operator_state_root.clone(),
    ));
    let control = ControlServer::bind(Arc::clone(&owner))?;
    let host =
        host::start(host_configs(&layout, &config, &secrets)?).map_err(anyhow::Error::msg)?;
    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "pid": std::process::id(),
            "hostAddress": host.address().to_string(),
            "controlAddress": control.address.to_string(),
            "registry": registry.display().to_string(),
            "harnesses": owner.registry().len(),
            "defaultRoom": config.default_room,
            "rooms": config.rooms,
        })
    );
    io::stdout().flush().context("report Athanor readiness")?;

    // Athanor exit must not stop OMP sessions, which remain peer-owned.
    host.wait().map_err(anyhow::Error::msg)
}
