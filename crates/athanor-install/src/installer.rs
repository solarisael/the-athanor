use crate::{
    boundaries::{FileSystem, RuntimeControl, SecretSource, ServiceManager},
    layout::{InstallLayout, LEGACY_NAMES, SERVICE_DISPLAY_NAME, SERVICE_NAME},
    manifest::ReleaseManifest,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct InstallRequest {
    pub staging: PathBuf,
    pub manifest: ReleaseManifest,
    pub external_database_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentRelease {
    pub version: String,
    pub previous_version: Option<String>,
    #[serde(default)]
    pub rollback_backup: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct RuntimeConfig {
    database_mode: String,
    database_host: String,
    database_port: u16,
    nats_host: String,
    nats_port: u16,
    host_health: String,
    schema_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSecrets {
    host_token: String,
    postgres_password: String,
    external_database_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallOutcome {
    pub version: String,
    pub upgraded_from: Option<String>,
    pub legacy_imported: bool,
}

pub struct Installer<'a, F, S, R, G> {
    pub fs: &'a F,
    pub services: &'a S,
    pub runtime: &'a R,
    pub secrets: &'a G,
    pub layout: InstallLayout,
}

impl<F: FileSystem, S: ServiceManager, R: RuntimeControl, G: SecretSource>
    Installer<'_, F, S, R, G>
{
    pub fn install(&self, request: InstallRequest) -> Result<InstallOutcome> {
        request.manifest.validate()?;
        self.preflight(&request)?;

        self.fs.create_dir_all(&self.layout.versions())?;
        self.fs.create_dir_all(&self.layout.backups())?;
        self.fs.create_dir_all(&self.layout.logs())?;
        self.fs.create_dir_all(&self.layout.nats_data())?;
        self.fs.create_dir_all(&self.layout.rooms().join("home"))?;
        self.fs.create_dir_all(&self.layout.host_state())?;
        if request.external_database_url.is_none() {
            self.fs.create_dir_all(&self.layout.postgres_data())?;
        }

        let legacy_imported = self.import_legacy_once()?;
        let current = self.read_current()?;
        if current
            .as_ref()
            .is_some_and(|value| value.version == request.manifest.version)
        {
            bail!("release {} is already installed", request.manifest.version);
        }

        let backup = if current.is_some() {
            self.runtime.backup_database(&self.layout.backups())?
        } else {
            None
        };
        if self.services.is_installed(SERVICE_NAME)? {
            self.services.stop(SERVICE_NAME)?;
        }
        let target = self.layout.version(&request.manifest.version);
        let staging_target = self
            .layout
            .versions()
            .join(format!(".{}.staging", request.manifest.version));
        self.fs.remove_tree(&staging_target)?;
        self.fs.create_dir_all(&staging_target)?;
        for artifact in &request.manifest.artifacts {
            self.fs.copy(
                &request.staging.join(&artifact.path),
                &staging_target.join(&artifact.path),
            )?;
        }
        self.fs.write_atomic(
            &staging_target.join("release-manifest.json"),
            &serde_json::to_vec_pretty(&request.manifest)?,
        )?;
        self.fs.rename(&staging_target, &target)?;

        let manager = request
            .manifest
            .artifacts
            .iter()
            .find(|artifact| {
                artifact.component == "installer" || artifact.path.ends_with("athanor-manage.exe")
            })
            .context("release has no athanor-manage.exe installer artifact")?;
        self.fs
            .copy(&target.join(&manager.path), &self.layout.manager())?;
        self.write_configuration(&request)?;

        let next = CurrentRelease {
            version: request.manifest.version.clone(),
            previous_version: current.as_ref().map(|value| value.version.clone()),
            rollback_backup: backup.clone(),
        };
        self.write_current(&next)?;
        if let Err(error) = self.runtime.migrate_database() {
            if let Some(previous) = &current {
                self.write_current(previous)?;
                if let Some(backup) = &backup {
                    self.runtime.restore_database(backup)?;
                }
                self.services.start(SERVICE_NAME).ok();
            } else {
                self.fs.remove_file(&self.layout.current())?;
                self.fs.remove_tree(&target)?;
            }
            return Err(error).context("database migration failed; activation was rolled back");
        }

        self.services.install_or_update(
            SERVICE_NAME,
            SERVICE_DISPLAY_NAME,
            &self.layout.manager(),
        )?;
        if let Err(error) = self
            .services
            .start(SERVICE_NAME)
            .and_then(|_| self.runtime.wait_ready())
        {
            self.services.stop(SERVICE_NAME).ok();
            if let Some(previous) = &current {
                self.write_current(previous)?;
                if let Some(backup) = &backup {
                    self.runtime.restore_database(backup)?;
                }
                self.services.start(SERVICE_NAME).ok();
            } else {
                self.services.remove(SERVICE_NAME).ok();
                self.fs.remove_file(&self.layout.current())?;
                self.fs.remove_tree(&target)?;
            }
            return Err(error).context("new release failed readiness; rolled back current pointer");
        }
        Ok(InstallOutcome {
            version: request.manifest.version,
            upgraded_from: current.map(|value| value.version),
            legacy_imported,
        })
    }

    pub fn rollback(&self) -> Result<CurrentRelease> {
        let current = self
            .read_current()?
            .context("there is no installed release")?;
        let previous = current
            .previous_version
            .clone()
            .context("there is no retained rollback release")?;
        let previous_backup = current
            .rollback_backup
            .clone()
            .context("rollback release has no pre-upgrade database backup")?;
        if !self.fs.exists(&self.layout.version(&previous)) {
            bail!("rollback release {previous} is not retained");
        }
        let undo_backup = self.runtime.backup_database(&self.layout.backups())?;
        self.services.stop(SERVICE_NAME)?;
        let rolled_back = CurrentRelease {
            version: previous,
            previous_version: Some(current.version.clone()),
            rollback_backup: undo_backup.clone(),
        };
        self.write_current(&rolled_back)?;
        if let Err(error) = self.runtime.restore_database(&previous_backup) {
            self.write_current(&current)?;
            if let Some(backup) = &undo_backup {
                self.runtime.restore_database(backup)?;
            }
            self.services.start(SERVICE_NAME).ok();
            return Err(error).context("rollback database restore failed; restored newer release");
        }
        if let Err(error) = self
            .services
            .start(SERVICE_NAME)
            .and_then(|_| self.runtime.wait_ready())
        {
            self.services.stop(SERVICE_NAME).ok();
            self.write_current(&current)?;
            if let Some(backup) = &undo_backup {
                self.runtime.restore_database(backup)?;
            }
            self.services.start(SERVICE_NAME).ok();
            return Err(error).context("rollback release was not ready; restored newer release");
        }
        Ok(rolled_back)
    }

    pub fn uninstall(&self) -> Result<()> {
        self.services.stop(SERVICE_NAME)?;
        self.services.remove(SERVICE_NAME)?;
        self.fs.remove_tree(&self.layout.program)
    }

    pub fn purge(&self, confirmed: bool) -> Result<()> {
        if !confirmed {
            bail!("purge requires --confirm-data-loss");
        }
        self.uninstall()?;
        self.fs.remove_tree(&self.layout.data)
    }

    fn preflight(&self, request: &InstallRequest) -> Result<()> {
        for artifact in &request.manifest.artifacts {
            let source = request.staging.join(&artifact.path);
            let bytes = self
                .fs
                .read(&source)
                .with_context(|| format!("read staged artifact {}", artifact.path))?;
            request.manifest.verify_bytes(artifact, &bytes)?;
        }
        Ok(())
    }

    fn read_current(&self) -> Result<Option<CurrentRelease>> {
        if !self.fs.exists(&self.layout.current()) {
            return Ok(None);
        }
        Ok(Some(
            serde_json::from_slice(&self.fs.read(&self.layout.current())?)
                .context("parse current release pointer")?,
        ))
    }

    fn write_current(&self, current: &CurrentRelease) -> Result<()> {
        self.fs
            .write_atomic(&self.layout.current(), &serde_json::to_vec_pretty(current)?)
    }

    fn write_configuration(&self, request: &InstallRequest) -> Result<()> {
        let existing_external = if self.fs.exists(&self.layout.config()) {
            serde_json::from_slice::<RuntimeConfig>(&self.fs.read(&self.layout.config())?)
                .map(|config| config.database_mode == "external")
                .unwrap_or(false)
        } else {
            false
        };
        let external = request.external_database_url.is_some() || existing_external;
        let config = RuntimeConfig {
            database_mode: if external { "external" } else { "managed" }.into(),
            database_host: "127.0.0.1".into(),
            database_port: 5432,
            nats_host: "127.0.0.1".into(),
            nats_port: 4222,
            host_health: "http://127.0.0.1:8787/health".into(),
            schema_version: request.manifest.schema_version,
        };
        self.fs
            .write_atomic(&self.layout.config(), &serde_json::to_vec_pretty(&config)?)?;
        if !self.fs.exists(&self.layout.secrets()) {
            let mut host = [0_u8; 32];
            let mut database = [0_u8; 32];
            self.secrets.fill(&mut host)?;
            self.secrets.fill(&mut database)?;
            let secrets = RuntimeSecrets {
                host_token: hex::encode(host),
                postgres_password: hex::encode(database),
                external_database_url: request.external_database_url.clone(),
            };
            self.fs.write_atomic(
                &self.layout.secrets(),
                &serde_json::to_vec_pretty(&secrets)?,
            )?;
        } else if let Some(url) = &request.external_database_url {
            let mut secrets: RuntimeSecrets =
                serde_json::from_slice(&self.fs.read(&self.layout.secrets())?)?;
            secrets.external_database_url = Some(url.clone());
            self.fs.write_atomic(
                &self.layout.secrets(),
                &serde_json::to_vec_pretty(&secrets)?,
            )?;
        }
        self.fs.restrict_acl(&self.layout.data)?;
        self.fs.restrict_acl(&self.layout.secrets())
    }

    fn import_legacy_once(&self) -> Result<bool> {
        if self
            .fs
            .exists(&self.layout.legacy_backup().join("imported.json"))
        {
            return Ok(false);
        }
        let parent = self
            .layout
            .program
            .parent()
            .context("product directory has no parent")?;
        let found = LEGACY_NAMES
            .iter()
            .map(|name| parent.join(name))
            .find(|path| self.fs.exists(path));
        let Some(source) = found else {
            return Ok(false);
        };
        self.fs.create_dir_all(&self.layout.legacy_backup())?;
        self.runtime
            .import_legacy(&source, &self.layout.legacy_backup())?;
        self.fs.write_atomic(
            &self.layout.legacy_backup().join("imported.json"),
            br#"{"bounded":true}"#,
        )?;
        Ok(true)
    }
}
