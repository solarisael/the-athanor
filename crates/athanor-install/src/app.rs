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
    io::{self, IsTerminal, Write},
    path::PathBuf,
    sync::Arc,
};

pub(crate) fn installed_runtime(layout: &InstallLayout) -> Result<(RuntimeConfig, RuntimeSecrets)> {
    let config: RuntimeConfig = serde_json::from_slice(
        &fs::read(layout.config())
            .with_context(|| format!("read {}", layout.config().display()))?,
    )?;
    config.validate()?;
    let secrets: RuntimeSecrets = serde_json::from_slice(
        &fs::read(layout.secrets())
            .with_context(|| format!("read {}", layout.secrets().display()))?,
    )?;
    if secrets.host_nats_auth.is_none() || secrets.akasha_nats_auth.is_none() {
        anyhow::bail!("installed NATS credentials are missing; run the canonical upgrade");
    }
    Ok((config, secrets))
}

/// Where the rooms of this install live. A room in the installation folder
/// `<dataRoot>/rooms/<room>` keeps its Host state in its `host/` folder. A
/// legacy room keeps `<roomsRoot>/<room>` and `<dataRoot>/state/host/<room>`.
struct RoomPlaces {
    installed: PathBuf,
    legacy: PathBuf,
    host_state: PathBuf,
    house: host::HouseConfig,
    knock_autonomy: host::KnockAutonomy,
}

impl RoomPlaces {
    fn config(&self, room: &str, spirit: &str, operator: Option<String>) -> host::HostConfig {
        let installed = self.installed.join(room);
        let (room_dir, state_dir) = if installed.is_dir() {
            let state_dir = installed.join(crate::cli::spirits::HOST_STATE);
            (installed, state_dir)
        } else {
            (self.legacy.join(room), self.host_state.join(room))
        };

        host::HostConfig {
            bind: self.house.bind,
            bearer_token: self.house.bearer_token.clone(),
            room_dir,
            state_dir,
            house_id: self.house.house_id.clone(),
            room: room.to_owned(),
            spirit: spirit.to_owned(),
            session: format!("app:{room}"),
            database_url: self.house.database_url.clone(),
            nats_url: self.house.nats_url.clone(),
            nats_auth: self.house.nats_auth.clone(),
            knock_autonomy: self.knock_autonomy,
            operator,
        }
    }
}

/// The House settings, and the spirit record as the source of the rooms. The
/// `rooms` of runtime.json only fill an empty record once.
pub(crate) fn host_rooms(
    layout: &InstallLayout,
    config: &RuntimeConfig,
    secrets: &RuntimeSecrets,
) -> Result<(host::HouseConfig, host::RoomSource)> {
    let house = host::HouseConfig {
        bind: format!("{LOOPBACK_HOST}:{}", config.host_port).parse()?,
        bearer_token: secrets.host_token.clone(),
        house_id: config.house_id.clone(),
        database_url: Some(secrets.database_url()),
        nats_url: Some(format!("nats://{}:{}", config.nats_host, config.nats_port)),
        nats_auth: secrets.host_nats_auth.clone(),
    };
    let places = RoomPlaces {
        installed: layout.rooms(),
        legacy: config.rooms_root.clone(),
        host_state: layout.host_state(),
        house: house.clone(),
        knock_autonomy: host::KnockAutonomy::from_optional(
            env::var(host::KNOCK_AUTONOMY_ENV).ok().as_deref(),
        )
        .map_err(anyhow::Error::msg)?,
    };
    let seed = config
        .rooms
        .iter()
        .map(|room| places.config(&room.room, &room.spirit, room.operator.clone()))
        .collect();
    let factory: host::RoomFactory = Arc::new(move |record: &host::SpiritRecord| {
        places.config(&record.room, &record.spirit, Some(record.operator.clone()))
    });

    Ok((house, host::RoomSource::Spirits { seed, factory }))
}

#[cfg(windows)]
static CONSOLE_OWNER: std::sync::OnceLock<Arc<HarnessOwner>> = std::sync::OnceLock::new();

#[cfg(windows)]
unsafe extern "system" fn console_control(event: u32) -> i32 {
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    };
    if !matches!(
        event,
        CTRL_C_EVENT
            | CTRL_BREAK_EVENT
            | CTRL_CLOSE_EVENT
            | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT
    ) {
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
    // The Host logs to stderr, which journald keeps for the systemd unit.
    // RUST_LOG overrides the default level.
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let layout = InstallLayout::from_environment()?;
    let (config, secrets) = installed_runtime(&layout)?;
    crate::service::ensure_running(&config)?;
    let registry = registry_path(&layout);
    let owner = Arc::new(
        HarnessOwner::new(
            HarnessRegistry::load(&registry)?,
            control_token(&OsSecrets)?,
        )
        .with_cutover_state_root(config.operator_state_root.clone()),
    );
    let _run_owner = RunOwner(Arc::clone(&owner));
    #[cfg(windows)]
    {
        CONSOLE_OWNER
            .set(Arc::clone(&owner))
            .map_err(|_| anyhow::anyhow!("Athanor console owner is already registered"))?;
        if unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_control), 1)
        } == 0
        {
            anyhow::bail!("Athanor SetConsoleCtrlHandler failed: {}", unsafe {
                windows_sys::Win32::Foundation::GetLastError()
            });
        }
    }
    let control = ControlServer::bind(Arc::clone(&owner))?;
    let (house, rooms) = host_rooms(&layout, &config, &secrets)?;
    let host = host::start(house, rooms).map_err(anyhow::Error::msg)?;
    let mut harnesses_started = Vec::new();
    let mut harnesses_failed = Vec::new();
    for id in owner.registry().auto_start_ids() {
        match owner.start(id) {
            Ok(_) => harnesses_started.push(id),
            Err(error) => {
                harnesses_failed.push(serde_json::json!({"id": id, "reason": format!("{error:#}")}))
            }
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
    // RunOwner and the console hook stop only this owner's children, and
    // release its keepers instead while a deployment holds a cutover.
    #[cfg(unix)]
    return wait_for_stop_signal(host);
    #[cfg(not(unix))]
    host.wait().map_err(anyhow::Error::msg)
}

/// systemd stops the House with SIGTERM and a terminal with SIGINT. Either one
/// shuts the Host down cleanly; `RunOwner` then stops this owner's harnesses.
#[cfg(unix)]
fn wait_for_stop_signal(host: host::HostRuntime) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build the stop-signal runtime")?;
    let stop_requested = runtime.block_on(async {
        let mut interrupt = signal(SignalKind::interrupt()).context("register SIGINT")?;
        let mut terminate = signal(SignalKind::terminate()).context("register SIGTERM")?;
        loop {
            if host.is_finished() {
                return Ok::<bool, anyhow::Error>(false);
            }
            tokio::select! {
                _ = interrupt.recv() => return Ok(true),
                _ = terminate.recv() => return Ok(true),
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
            }
        }
    })?;

    if stop_requested {
        host.shutdown().map_err(anyhow::Error::msg)
    } else {
        host.wait().map_err(anyhow::Error::msg)
    }
}
