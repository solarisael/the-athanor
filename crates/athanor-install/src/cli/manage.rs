//! The installation modes: install, update, adapter install and rollback,
//! doctor, rollback, uninstall, purge. Each takes the same four native
//! seams the installer always took; only the door moved.

use super::{optional_value, value};
use crate::{
    boundaries::{NativeFileSystem, OsSecrets, ScServiceManager},
    doctor,
    installer::{HouseInstallConfig, InstallRequest, Installer, OperatorIntegration},
    layout::InstallLayout,
    manifest::ReleaseManifest,
    native_runtime::NativeRuntimeControl,
};
use anyhow::{Context, Result, bail};
use std::{fs, path::PathBuf};

pub fn run(command: &str, arguments: &[String]) -> Result<()> {
    let layout = InstallLayout::from_environment()?;
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
            let staging = PathBuf::from(value(arguments, "--staging")?);
            // Lets the pre-upgrade backup run on the staged substrate, which
            // knows at least the installed lineage (see NativeRuntimeControl).
            // SAFETY: single-threaded at this point — set before any installer work spawns.
            unsafe { std::env::set_var("ATHANOR_INSTALL_STAGING_BIN", staging.join("bin")) };
            let manifest_path = PathBuf::from(value(arguments, "--manifest")?);
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
                optional_value(arguments, "--omp-config")?,
                optional_value(arguments, "--client-config")?,
                optional_value(arguments, "--operator-principal")?,
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
                    &serde_json::json!({"ok": true, "version": outcome.version, "upgradedFrom": outcome.upgraded_from, "legacyImported": outcome.legacy_imported, "ompRegistered": outcome.omp_registered, "warnings": outcome.warnings})
                )?
            );
        }
        "install-omp-adapter" => {
            let source = PathBuf::from(value(arguments, "--source")?);
            println!(
                "{}",
                serde_json::to_string_pretty(&installer.install_omp_adapter(&source)?)?
            );
        }
        "rollback-omp-adapter" => {
            let release_id = optional_value(arguments, "--release-id")?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &installer.rollback_omp_adapter(release_id.as_deref())?
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
        unknown => bail!("unknown mode {unknown:?}; run athanor help"),
    }
    Ok(())
}
