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

pub(crate) fn installed_runtime(layout: &InstallLayout) -> Result<(RuntimeConfig, RuntimeSecrets)> {
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

#[cfg(windows)]
static CONSOLE_OWNER: std::sync::OnceLock<Arc<HarnessOwner>> = std::sync::OnceLock::new();

#[cfg(windows)]
unsafe extern "system" fn console_control(event: u32) -> i32 {
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    };
    if !matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT) {
        return 0;
    }
    if let Some(owner) = CONSOLE_OWNER.get() {
        owner.shutdown();
    }
    std::process::exit(0);
}

struct RunOwner(Arc<HarnessOwner>);

impl Drop for RunOwner {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

pub fn run() -> Result<()> {
    let layout = InstallLayout::from_environment()?;
    let (config, secrets) = installed_runtime(&layout)?;
    crate::service::ensure_running(&config)?;
    let registry = registry_path(&layout);
    let owner = Arc::new(HarnessOwner::new(
        HarnessRegistry::load(&registry)?,
        control_token(&OsSecrets)?,
    ));
    let _run_owner = RunOwner(Arc::clone(&owner));
    #[cfg(windows)]
    {
        CONSOLE_OWNER.set(Arc::clone(&owner))
            .map_err(|_| anyhow::anyhow!("Athanor console owner is already registered"))?;
        if unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_control), 1) } == 0 {
            anyhow::bail!("Athanor SetConsoleCtrlHandler failed: {}", unsafe { windows_sys::Win32::Foundation::GetLastError() });
        }
    }
    let control = ControlServer::bind(Arc::clone(&owner))?;
    let host =
        host::start(host_configs(&layout, &config, &secrets)?).map_err(anyhow::Error::msg)?;
    let mut harnesses_started = Vec::new();
    let mut harnesses_failed = Vec::new();
    for id in owner.registry().auto_start_ids() {
        match owner.start(id) {
            Ok(_) => harnesses_started.push(id),
            Err(error) => harnesses_failed.push(serde_json::json!({"id": id, "reason": format!("{error:#}")})),
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "pid": std::process::id(),
            "hostAddress": host.address().to_string(),
            "controlAddress": control.address.to_string(),
            "registry": registry.display().to_string(),
            "harnesses": owner.registry().len(),
            "harnessesStarted": harnesses_started,
            "harnessesFailed": harnesses_failed,
            "defaultRoom": config.default_room,
            "rooms": config.rooms,
        })
    );
    io::stdout().flush().context("report Athanor readiness")?;

    // The House owns the harnesses it starts, not sessions started elsewhere.
    // RunOwner and the console hook stop only this owner's children.
    host.wait().map_err(anyhow::Error::msg)
}
