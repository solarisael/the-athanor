//! The default path of `athanor.exe`.
//!
//! One process holds the GUI window and every harness handle, so the operator
//! closing Athanor also closes what Athanor started. `athanor-manage` keeps the
//! installer commands; this is the door a person opens.

use crate::{
    boundaries::OsSecrets,
    harness::{
        CONTROL_ADDR_ENV, CONTROL_TOKEN_ENV, ControlServer, HarnessOwner, HarnessRegistry,
        control_token, registry_path,
    },
    installer::CurrentRelease,
    layout::InstallLayout,
    omp::ClientProjection,
};
use anyhow::{Context, Result, bail};
use interactive_process::JobOwnedCommand;
use std::{env, fs, path::PathBuf, sync::Arc};

pub struct InstalledGui {
    pub executable: PathBuf,
    pub project: PathBuf,
}

pub fn installed_gui(layout: &InstallLayout) -> Result<InstalledGui> {
    let pointer = layout.current();
    let current: CurrentRelease = serde_json::from_slice(
        &fs::read(&pointer).with_context(|| format!("read {}", pointer.display()))?,
    )?;
    let version_root = layout.version(&current.version);
    Ok(InstalledGui {
        executable: version_root.join("bin/athanor-gui.exe"),
        project: version_root.join("runtime/godot"),
    })
}

pub fn installed_client() -> Result<ClientProjection> {
    let user_profile =
        PathBuf::from(env::var_os("USERPROFILE").context("USERPROFILE is unavailable")?);
    let path = user_profile.join(".omp/agent/athanor/client.json");
    let client: ClientProjection = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    client.validate()?;
    Ok(client)
}

/// Blocks for the whole life of the GUI: this call is the app session, and the
/// harness children die with it.
pub fn run(room: Option<String>) -> Result<()> {
    let layout = InstallLayout::from_environment()?;
    let client = installed_client()?;
    let room = room.unwrap_or_else(|| client.default_room.clone());
    let endpoint = client
        .endpoints
        .get(&room)
        .with_context(|| format!("installed Athanor has no endpoint for room {room:?}"))?;
    let registry = registry_path(&layout);
    let owner = Arc::new(HarnessOwner::new(
        HarnessRegistry::load(&registry)?,
        control_token(&OsSecrets)?,
        layout.program.clone(),
        PathBuf::from(&client.state_root),
    ));
    let control = ControlServer::bind(Arc::clone(&owner))?;
    let gui = installed_gui(&layout)?;
    let project = gui.project.display().to_string();
    let mut child = JobOwnedCommand::new(&gui.executable)
        .args(["--path", project.as_str()])
        .env("ATHANOR_HOST_TOKEN", &client.host_token)
        .env("ATHANOR_HOST_HOUSE_ID", &client.house_id)
        .env("ATHANOR_HOST_WS_URL", &endpoint.url)
        .env("ATHANOR_HOST_ROOM", &room)
        .env("ATHANOR_HOST_SPIRIT", &endpoint.spirit)
        .env(CONTROL_ADDR_ENV, control.address.to_string())
        .env(CONTROL_TOKEN_ENV, owner.token())
        .spawn()
        .context("launch installed Godot client")?;
    println!(
        "{}",
        serde_json::json!({
            "ok": true,
            "room": room,
            "pid": child.id(),
            "controlAddress": control.address.to_string(),
            "registry": registry.display().to_string(),
            "harnesses": owner.registry().len(),
        })
    );
    let status = child.wait().context("wait for the Athanor GUI")?;
    owner.shutdown();
    if !status.success() {
        bail!("the Athanor GUI exited with {status}");
    }
    Ok(())
}
