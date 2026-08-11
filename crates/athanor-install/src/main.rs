use anyhow::{Context, Result, bail};
use athanor_install::{
    boundaries::{NativeFileSystem, OsSecrets, ScServiceManager},
    doctor,
    installer::{InstallRequest, Installer},
    layout::InstallLayout,
    manifest::ReleaseManifest,
    native_runtime::NativeRuntimeControl,
    service,
};
use std::{env, fs, path::PathBuf};

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
            "Athanor native runtime manager\n\nCommands:\n  install --staging DIR --manifest FILE [--external-database-file FILE]\n  update --staging DIR --manifest FILE [--external-database-file FILE]\n  doctor\n  rollback\n  uninstall\n  purge --confirm-data-loss\n  service"
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
            let outcome = installer.install(InstallRequest {
                staging,
                manifest,
                external_database_url,
            })?;
            println!(
                "{}",
                serde_json::to_string(
                    &serde_json::json!({"ok": true, "version": outcome.version, "upgradedFrom": outcome.upgraded_from, "legacyImported": outcome.legacy_imported})
                )?
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
