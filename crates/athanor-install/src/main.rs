use anyhow::{Context, Result, bail};
use athanor_install::{
    boundaries::{NativeFileSystem, OsSecrets, ScServiceManager},
    doctor,
    installer::{
        CurrentRelease, HouseInstallConfig, InstallRequest, Installer, OperatorIntegration,
    },
    layout::InstallLayout,
    manifest::ReleaseManifest,
    native_runtime::NativeRuntimeControl,
    omp::ClientProjection,
    service,
};
use std::{env, fs, path::PathBuf, process::Command};

fn value(arguments: &[String], flag: &str) -> Result<String> {
    let index = arguments
        .iter()
        .position(|argument| argument == flag)
        .with_context(|| format!("{flag} is required"))?;
    arguments
        .get(index + 1)
        .cloned()
        .with_context(|| format!("{flag} requires a value"))
}

fn optional_value(arguments: &[String], flag: &str) -> Result<Option<String>> {
    arguments
        .iter()
        .position(|argument| argument == flag)
        .map(|index| {
            arguments
                .get(index + 1)
                .cloned()
                .with_context(|| format!("{flag} requires a value"))
        })
        .transpose()
}

fn layout() -> Result<InstallLayout> {
    let program_files =
        PathBuf::from(env::var_os("ProgramFiles").context("ProgramFiles is unavailable")?);
    let program_data =
        PathBuf::from(env::var_os("ProgramData").context("ProgramData is unavailable")?);
    Ok(InstallLayout::new(&program_files, &program_data))
}

fn main() -> Result<()> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let command = arguments.first().map(String::as_str).unwrap_or("help");
    if command == "service" {
        return service::dispatch();
    }
    if matches!(command, "help" | "--help" | "-h") {
        println!(
            "Athanor native runtime manager\n\nCommands:\n  install --staging DIR --manifest FILE [--external-database-file FILE] [--house-config-file FILE] [--omp-config FILE --client-config FILE --operator-principal NAME]\n  update --staging DIR --manifest FILE [same options]\n  gui [--room ROOM]\n  doctor\n  rollback\n  uninstall\n  purge --confirm-data-loss\n  service"
        );
        return Ok(());
    }

    let layout = layout()?;
    let fs_boundary = NativeFileSystem;
    let services = ScServiceManager;
    let runtime = NativeRuntimeControl {
        layout: layout.clone(),
    };
    let secrets = OsSecrets;
    let installer = Installer {
        fs: &fs_boundary,
        services: &services,
        runtime: &runtime,
        secrets: &secrets,
        layout: layout.clone(),
    };
    match command {
        "install" | "update" => {
            let staging = PathBuf::from(value(&arguments, "--staging")?);
            let manifest_path = PathBuf::from(value(&arguments, "--manifest")?);
            let manifest: ReleaseManifest = serde_json::from_slice(
                &fs::read(&manifest_path)
                    .with_context(|| format!("read {}", manifest_path.display()))?,
            )?;
            let external_database_url = arguments
                .iter()
                .position(|argument| argument == "--external-database-file")
                .map(|index| {
                    let file = arguments
                        .get(index + 1)
                        .context("--external-database-file requires a value")?;
                    Ok::<_, anyhow::Error>(fs::read_to_string(file)?.trim().to_owned())
                })
                .transpose()?;
            let house_config = arguments
                .iter()
                .position(|argument| argument == "--house-config-file")
                .map(|index| {
                    let file = arguments
                        .get(index + 1)
                        .context("--house-config-file requires a value")?;
                    Ok::<_, anyhow::Error>(serde_json::from_slice::<HouseInstallConfig>(
                        &fs::read(file)?,
                    )?)
                })
                .transpose()?;
            let operator_integration = match (
                optional_value(&arguments, "--omp-config")?,
                optional_value(&arguments, "--client-config")?,
                optional_value(&arguments, "--operator-principal")?,
            ) {
                (None, None, None) => None,
                (Some(omp_config), Some(client_config), Some(operator_principal)) => {
                    Some(OperatorIntegration {
                        omp_config_path: PathBuf::from(omp_config),
                        client_config_path: PathBuf::from(client_config),
                        operator_principal,
                    })
                }
                _ => bail!(
                    "--omp-config, --client-config, and --operator-principal must be supplied together"
                ),
            };
            let outcome = installer.install(InstallRequest {
                staging,
                manifest,
                external_database_url,
                house_config,
                operator_integration,
            })?;
            println!(
                "{}",
                serde_json::to_string(
                    &serde_json::json!({"ok": true, "version": outcome.version, "upgradedFrom": outcome.upgraded_from, "legacyImported": outcome.legacy_imported, "ompRegistered": outcome.omp_registered})
                )?
            );
        }
        "gui" => {
            let user_profile =
                PathBuf::from(env::var_os("USERPROFILE").context("USERPROFILE is unavailable")?);
            let client_path = user_profile.join(".omp/agent/athanor/client.json");
            let client: ClientProjection = serde_json::from_slice(
                &fs::read(&client_path)
                    .with_context(|| format!("read {}", client_path.display()))?,
            )?;
            client.validate()?;
            let room = optional_value(&arguments, "--room")?
                .unwrap_or_else(|| client.default_room.clone());
            let endpoint = client
                .endpoints
                .get(&room)
                .with_context(|| format!("installed Athanor has no endpoint for room {room:?}"))?;
            let current: CurrentRelease = serde_json::from_slice(&fs::read(layout.current())?)?;
            let version_root = layout.version(&current.version);
            let child = Command::new(version_root.join("bin/athanor-gui.exe"))
                .args([
                    "--path",
                    &version_root.join("runtime/godot").display().to_string(),
                ])
                .env("ATHANOR_HOST_TOKEN", &client.host_token)
                .env("ATHANOR_HOST_HOUSE_ID", &client.house_id)
                .env("ATHANOR_HOST_WS_URL", &endpoint.url)
                .env("ATHANOR_HOST_ROOM", &room)
                .env("ATHANOR_HOST_SPIRIT", &endpoint.spirit)
                .spawn()
                .context("launch installed Godot client")?;
            println!(
                "{}",
                serde_json::json!({"ok": true, "room": room, "pid": child.id()})
            );
        }
        "doctor" => {
            let report = doctor(&fs_boundary, &services, &layout)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.ok {
                bail!("doctor found one or more failed checks");
            }
        }
        "rollback" => println!("{}", serde_json::to_string_pretty(&installer.rollback()?)?),
        "uninstall" => {
            installer.uninstall()?;
            println!("{{\"ok\":true,\"dataPreserved\":true}}");
        }
        "purge" => {
            installer.purge(
                arguments
                    .iter()
                    .any(|argument| argument == "--confirm-data-loss"),
            )?;
            println!("{{\"ok\":true,\"dataPreserved\":false}}");
        }
        unknown => bail!("unknown command {unknown:?}; run athanor-manage help"),
    }
    Ok(())
}
