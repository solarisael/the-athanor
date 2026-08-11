use crate::{
    boundaries::{FileSystem, RuntimeControl, SecretSource, ServiceManager},
    layout::{InstallLayout, LEGACY_NAMES, SERVICE_DISPLAY_NAME, SERVICE_NAME},
    manifest::ReleaseManifest,
    omp::{ClientEndpoint, ClientProjection, register_extension, unregister_extension},
    supervisor::HostRoomConfig,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct OperatorIntegration {
    pub omp_config_path: PathBuf,
    pub client_config_path: PathBuf,
    pub operator_principal: String,
}

#[derive(Clone, Debug)]
pub struct InstallRequest {
    pub staging: PathBuf,
    pub manifest: ReleaseManifest,
    pub external_database_url: Option<String>,
    pub house_config: Option<HouseInstallConfig>,
    pub operator_integration: Option<OperatorIntegration>,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HouseInstallConfig {
    pub house_id: String,
    pub rooms_root: PathBuf,
    pub operator_state_root: PathBuf,
    pub default_room: String,
    pub rooms: Vec<HostRoomConfig>,
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
    house_id: String,
    rooms_root: PathBuf,
    operator_state_root: PathBuf,
    default_room: String,
    rooms: Vec<HostRoomConfig>,
    #[serde(default)]
    omp_config_path: Option<PathBuf>,
    #[serde(default)]
    client_config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSecrets {
    host_token: String,
    postgres_password: String,
    external_database_url: Option<String>,
}

impl HouseInstallConfig {
    fn validate(&self) -> Result<()> {
        if self.house_id.trim().is_empty() {
            bail!("houseId must not be empty");
        }
        if !self.rooms_root.is_absolute() || !self.operator_state_root.is_absolute() {
            bail!("roomsRoot and operatorStateRoot must be absolute");
        }
        if self.rooms.is_empty() {
            bail!("at least one room is required");
        }
        let mut rooms = std::collections::BTreeSet::new();
        let mut ports = std::collections::BTreeSet::new();
        for room in &self.rooms {
            let safe_room = !room.room.is_empty()
                && room.room != "house"
                && !room.room.starts_with('-')
                && !room.room.ends_with('-')
                && !room.room.contains("--")
                && room
                    .room
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
            if !safe_room || room.spirit.trim().is_empty() {
                bail!("room identity is invalid for {:?}", room.room);
            }
            if !rooms.insert(room.room.clone()) {
                bail!("duplicate room {:?}", room.room);
            }
            if !ports.insert(room.port) || matches!(room.port, 4222 | 5432) {
                bail!("room port {} is duplicate or reserved", room.port);
            }
        }
        if !rooms.contains(&self.default_room) {
            bail!("defaultRoom must name one configured room");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallOutcome {
    pub version: String,
    pub upgraded_from: Option<String>,
    pub legacy_imported: bool,
    pub omp_registered: bool,
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
        let house = self.resolve_house_config(&request)?;
        house.validate()?;
        self.preflight(&request, &house)?;

        self.fs.create_dir_all(&self.layout.versions())?;
        self.fs.create_dir_all(&self.layout.backups())?;
        self.fs.create_dir_all(&self.layout.logs())?;
        self.fs.create_dir_all(&self.layout.nats_data())?;
        self.fs.create_dir_all(&self.layout.host_state())?;
        self.fs.restrict_acl(&self.layout.data)?;
        for room in &house.rooms {
            let room_dir = house.rooms_root.join(&room.room);
            let state_path = room_dir.join(".omp/runtime/solarisael-house-state.json");
            if !self.fs.exists(&state_path) {
                if house.rooms_root != self.layout.rooms() {
                    bail!(
                        "room {:?} has no room-state file at {}",
                        room.room,
                        state_path.display()
                    );
                }
                self.fs.create_dir_all(state_path.parent().unwrap())?;
                self.fs.write_atomic(
                    &state_path,
                    &serde_json::to_vec_pretty(&serde_json::json!({
                        "version": 1,
                        "operator": "operator",
                        "agentName": room.spirit,
                        "embodiedSpirit": room.spirit,
                        "room": room.room,
                        "recallPolicy": {
                            "requestedMode": "auto",
                            "resolvedMode": "conversation",
                            "resolutionReason": "default"
                        }
                    }))?,
                )?;
            }
            self.fs
                .create_dir_all(&self.layout.host_state().join(&room.room))?;
        }
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

        let mut backup = if current.is_some() {
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
        self.fs.remove_tree(&target)?;
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
        let manager_source = target.join(&manager.path);
        let manager_target = self.layout.manager();
        let manager_is_current = self.fs.exists(&manager_target)
            && self.fs.read(&manager_target)? == self.fs.read(&manager_source)?;
        if !manager_is_current {
            self.fs.copy(&manager_source, &manager_target)?;
        }
        self.write_configuration(&request, &house)?;

        let mut next = CurrentRelease {
            version: request.manifest.version.clone(),
            previous_version: current.as_ref().map(|value| value.version.clone()),
            rollback_backup: backup.clone(),
        };
        self.write_current(&next)?;
        if current.is_none() && request.external_database_url.is_some() {
            backup = match self.runtime.backup_database(&self.layout.backups()) {
                Ok(backup) => backup,
                Err(error) => {
                    self.fs.remove_file(&self.layout.current())?;
                    self.fs.remove_tree(&target)?;
                    return Err(error)
                        .context("first external install backup failed before database migration");
                }
            };
            next.rollback_backup = backup.clone();
            self.write_current(&next)?;
        }
        if let Err(error) = self.runtime.migrate_database() {
            if let Some(previous) = &current {
                self.write_current(previous)?;
                if let Some(backup) = &backup {
                    self.runtime.restore_database(backup)?;
                }
                self.services.start(SERVICE_NAME).ok();
            } else {
                if let Some(backup) = &backup {
                    self.runtime.restore_database(backup)?;
                }
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
                if let Some(backup) = &backup {
                    self.runtime.restore_database(backup)?;
                }
                self.services.remove(SERVICE_NAME).ok();
                self.fs.remove_file(&self.layout.current())?;
                self.fs.remove_tree(&target)?;
            }
            return Err(error).context("new release failed readiness; rolled back current pointer");
        }
        let omp_registered = self.write_operator_integration(&request, &house)?;
        Ok(InstallOutcome {
            version: request.manifest.version,
            upgraded_from: current.map(|value| value.version),
            legacy_imported,
            omp_registered,
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
        let integration = if self.fs.exists(&self.layout.config()) {
            serde_json::from_slice::<RuntimeConfig>(&self.fs.read(&self.layout.config())?)
                .ok()
                .and_then(|config| config.omp_config_path.zip(config.client_config_path))
        } else {
            None
        };
        self.services.stop(SERVICE_NAME)?;
        self.services.remove(SERVICE_NAME)?;
        if let Some((omp_config, client_config)) = integration {
            if self.fs.exists(&omp_config) {
                let text = String::from_utf8(self.fs.read(&omp_config)?)?;
                self.fs.write_atomic(
                    &omp_config,
                    unregister_extension(&text, &self.layout.omp_loader()).as_bytes(),
                )?;
            }
            if let Some(parent) = client_config.parent() {
                self.fs.remove_tree(parent)?;
            }
        }
        self.fs.remove_tree(&self.layout.program)
    }

    pub fn purge(&self, confirmed: bool) -> Result<()> {
        if !confirmed {
            bail!("purge requires --confirm-data-loss");
        }
        self.uninstall()?;
        self.fs.remove_tree(&self.layout.data)
    }

    fn preflight(&self, request: &InstallRequest, house: &HouseInstallConfig) -> Result<()> {
        for artifact in &request.manifest.artifacts {
            let source = request.staging.join(&artifact.path);
            let bytes = self
                .fs
                .read(&source)
                .with_context(|| format!("read staged artifact {}", artifact.path))?;
            request.manifest.verify_bytes(artifact, &bytes)?;
        }
        if house.rooms_root != self.layout.rooms() {
            for room in &house.rooms {
                let state = house
                    .rooms_root
                    .join(&room.room)
                    .join(".omp/runtime/solarisael-house-state.json");
                if !self.fs.exists(&state) {
                    bail!(
                        "configured room {:?} has no room-state file at {}",
                        room.room,
                        state.display()
                    );
                }
            }
        }
        if let Some(integration) = &request.operator_integration {
            if !integration.omp_config_path.is_absolute()
                || !integration.client_config_path.is_absolute()
                || integration.operator_principal.trim().is_empty()
            {
                bail!("OMP integration paths must be absolute and operator principal non-empty");
            }
            let config = self
                .fs
                .read(&integration.omp_config_path)
                .context("read operator OMP configuration")?;
            String::from_utf8(config).context("operator OMP configuration must be UTF-8")?;
            if !self.fs.exists(&self.layout.omp_loader()) {
                bail!(
                    "stable OMP loader is missing at {}",
                    self.layout.omp_loader().display()
                );
            }
            let client_dir = integration
                .client_config_path
                .parent()
                .context("OMP client projection has no parent directory")?;
            self.fs.create_dir_all(client_dir)?;
            self.fs
                .restrict_user_acl(client_dir, &integration.operator_principal)?;
        }
        Ok(())
    }

    fn resolve_house_config(&self, request: &InstallRequest) -> Result<HouseInstallConfig> {
        if let Some(house) = &request.house_config {
            return Ok(house.clone());
        }
        if self.fs.exists(&self.layout.config()) {
            let config: RuntimeConfig =
                serde_json::from_slice(&self.fs.read(&self.layout.config())?)
                    .context("parse existing runtime configuration")?;
            return Ok(HouseInstallConfig {
                house_id: config.house_id,
                rooms_root: config.rooms_root,
                operator_state_root: config.operator_state_root,
                default_room: config.default_room,
                rooms: config.rooms,
            });
        }
        Ok(HouseInstallConfig {
            house_id: "local".into(),
            rooms_root: self.layout.rooms(),
            operator_state_root: self.layout.data.join("state"),
            default_room: "home".into(),
            rooms: vec![HostRoomConfig {
                room: "home".into(),
                spirit: "Athanor".into(),
                port: 8787,
            }],
        })
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

    fn write_configuration(
        &self,
        request: &InstallRequest,
        house: &HouseInstallConfig,
    ) -> Result<()> {
        let existing_external = if self.fs.exists(&self.layout.config()) {
            serde_json::from_slice::<RuntimeConfig>(&self.fs.read(&self.layout.config())?)
                .map(|config| config.database_mode == "external")
                .unwrap_or(false)
        } else {
            false
        };
        let external = request.external_database_url.is_some() || existing_external;
        let default_port = house
            .rooms
            .iter()
            .find(|room| room.room == house.default_room)
            .map(|room| room.port)
            .context("default room is not configured")?;
        let config = RuntimeConfig {
            database_mode: if external { "external" } else { "managed" }.into(),
            database_host: "127.0.0.1".into(),
            database_port: 5432,
            nats_host: "127.0.0.1".into(),
            nats_port: 4222,
            host_health: format!("http://127.0.0.1:{default_port}/health"),
            schema_version: request.manifest.schema_version,
            house_id: house.house_id.clone(),
            rooms_root: house.rooms_root.clone(),
            operator_state_root: house.operator_state_root.clone(),
            default_room: house.default_room.clone(),
            rooms: house.rooms.clone(),
            omp_config_path: request
                .operator_integration
                .as_ref()
                .map(|integration| integration.omp_config_path.clone()),
            client_config_path: request
                .operator_integration
                .as_ref()
                .map(|integration| integration.client_config_path.clone()),
        };
        self.fs
            .write_atomic(&self.layout.config(), &serde_json::to_vec_pretty(&config)?)?;
        let secrets_path = self.layout.secrets();
        let secrets_dir = secrets_path
            .parent()
            .context("runtime secret has no parent directory")?;
        self.fs.create_dir_all(secrets_dir)?;
        self.fs.restrict_acl(secrets_dir)?;
        if self.fs.exists(&self.layout.secrets()) {
            self.fs.restrict_acl(&self.layout.secrets())?;
        }
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
        self.fs.restrict_acl(&self.layout.secrets())
    }

    fn write_operator_integration(
        &self,
        request: &InstallRequest,
        house: &HouseInstallConfig,
    ) -> Result<bool> {
        let Some(integration) = &request.operator_integration else {
            return Ok(false);
        };
        if !self.fs.exists(&self.layout.omp_loader()) {
            bail!(
                "stable OMP loader is missing at {}",
                self.layout.omp_loader().display()
            );
        }
        let secrets: RuntimeSecrets =
            serde_json::from_slice(&self.fs.read(&self.layout.secrets())?)?;
        let endpoints = house
            .rooms
            .iter()
            .map(|room| {
                (
                    room.room.clone(),
                    ClientEndpoint {
                        url: format!("ws://127.0.0.1:{}/athanor/v1/ws", room.port),
                        spirit: room.spirit.clone(),
                    },
                )
            })
            .collect();
        let client = ClientProjection {
            format: 1,
            house_id: house.house_id.clone(),
            host_token: secrets.host_token,
            state_root: house.operator_state_root.display().to_string(),
            default_room: house.default_room.clone(),
            endpoints,
        };
        client.validate()?;
        let client_dir = integration
            .client_config_path
            .parent()
            .context("OMP client projection has no parent directory")?;
        self.fs.create_dir_all(client_dir)?;
        self.fs
            .restrict_user_acl(client_dir, &integration.operator_principal)?;
        self.fs.write_atomic(
            &integration.client_config_path,
            &serde_json::to_vec_pretty(&client)?,
        )?;
        self.fs.restrict_user_acl(
            &integration.client_config_path,
            &integration.operator_principal,
        )?;

        let config = String::from_utf8(self.fs.read(&integration.omp_config_path)?)?;
        let updated = register_extension(&config, &self.layout.omp_loader());
        if let Err(error) = self
            .fs
            .write_atomic(&integration.omp_config_path, updated.as_bytes())
        {
            self.fs.remove_tree(client_dir).ok();
            return Err(error).context("register stable OMP loader");
        }
        Ok(true)
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
