use crate::{
    boundaries::{FileSystem, OperationLock, RuntimeControl, SecretSource, ServiceManager},
    component::{
        COMPONENT_FORMAT, COMPONENT_MANIFEST, ComponentManifest, ComponentPointer,
        read_verified_component, valid_release_id,
    },
    contract::LOOPBACK_HOST,
    endpoints::{MANAGED_DATABASE_PORT, MANAGED_NATS_PORT},
    layout::{InstallLayout, LEGACY_NAMES, SERVICE_DISPLAY_NAME, SERVICE_NAME, safe_version},
    manifest::ReleaseManifest,
    omp::{ClientEndpoint, ClientProjection, register_extension, unregister_extension},
    supervisor::HostRoomConfig,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct FileSnapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

struct InstallRollback {
    files: Vec<FileSnapshot>,
    database_backup: Option<PathBuf>,
    database_restore_uses_attempted_config: bool,
    target: PathBuf,
    staging_target: PathBuf,
    component_target: PathBuf,
    component_target_existed: bool,
    service_was_installed: bool,
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
        let _operation = OperationLock::acquire()?;
        request.manifest.validate()?;
        let house = self.resolve_house_config(&request)?;
        house.validate()?;
        self.preflight(&request, &house)?;

        let current = self.read_current()?;
        if current
            .as_ref()
            .is_some_and(|value| value.version == request.manifest.version)
        {
            bail!("release {} is already installed", request.manifest.version);
        }
        let fallback_root = request.staging.join("components/omp-adapter");
        let fallback_component = read_verified_component(self.fs, &fallback_root)?;
        let mut rollback = self.capture_install_rollback(
            &request,
            &house,
            &fallback_component,
            current.is_some(),
        )?;

        let attempted = (|| -> Result<InstallOutcome> {
            self.fs.create_dir_all(&self.layout.versions())?;
            self.fs.create_dir_all(&self.layout.backups())?;
            self.fs.create_dir_all(&self.layout.logs())?;
            self.fs.create_dir_all(&self.layout.nats_data())?;
            self.fs.create_dir_all(&self.layout.host_state())?;
            self.fs.restrict_acl(&self.layout.data)?;
            for room in &house.rooms {
                let room_dir = house.rooms_root.join(&room.room);
                let state_path = room_dir.join(".omp/runtime/athanor-house-state.json");
                if !self.fs.exists(&state_path) {
                    if house.rooms_root != self.layout.rooms() {
                        bail!(
                            "room {:?} has no room-state file at {}",
                            room.room,
                            state_path.display()
                        );
                    }
                    self.fs.create_dir_all(
                        state_path
                            .parent()
                            .context("generated room state has no parent")?,
                    )?;
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

            if current.is_some() {
                rollback.database_backup = self.runtime.backup_database(&self.layout.backups())?;
            }
            let legacy_imported = self.import_legacy_once()?;
            let (_, component_next) =
                self.component_transition_for(&request.manifest, &fallback_root)?;

            if rollback.service_was_installed {
                self.services.stop(SERVICE_NAME)?;
            }
            let staging_target = &rollback.staging_target;
            self.fs.remove_tree(&rollback.target)?;
            self.fs.remove_tree(staging_target)?;
            self.fs.create_dir_all(staging_target)?;
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
            self.verify_native_release(staging_target, &request.manifest)?;
            self.fs.rename(staging_target, &rollback.target)?;
            self.verify_native_release(&rollback.target, &request.manifest)?;

            let manager = request
                .manifest
                .artifacts
                .iter()
                .find(|artifact| artifact.path.ends_with("athanor-manage.exe"))
                .context("release has no athanor-manage.exe installer artifact")?;
            let manager_source = rollback.target.join(&manager.path);
            let manager_target = self.layout.manager();
            if self.fs.exists(&manager_target) {
                self.fs
                    .validate_regular_file(&self.layout.program, &manager_target)?;
            }
            let manager_is_current = self.fs.exists(&manager_target)
                && self.fs.read(&manager_target)? == self.fs.read(&manager_source)?;
            if !manager_is_current {
                self.fs.copy(&manager_source, &manager_target)?;
            }
            self.write_configuration(&request, &house)?;
            if current.is_none() && request.external_database_url.is_some() {
                rollback.database_backup = self.runtime.backup_database(&self.layout.backups())?;
            }

            let next = CurrentRelease {
                version: request.manifest.version.clone(),
                previous_version: current.as_ref().map(|value| value.version.clone()),
                rollback_backup: rollback.database_backup.clone(),
            };
            self.write_current(&next)?;
            if let Some(component) = &component_next {
                self.write_component_pointer(component)?;
            }
            self.runtime.migrate_database()?;
            self.services.install_or_update(
                SERVICE_NAME,
                SERVICE_DISPLAY_NAME,
                &self.layout.manager(),
            )?;
            self.services.start(SERVICE_NAME)?;
            self.runtime.wait_ready()?;
            let omp_registered = self.write_operator_integration(&request, &house)?;
            Ok(InstallOutcome {
                version: request.manifest.version.clone(),
                upgraded_from: current.as_ref().map(|value| value.version.clone()),
                legacy_imported,
                omp_registered,
                warnings: Vec::new(),
            })
        })();

        match attempted {
            Ok(mut outcome) => {
                let active = self
                    .read_current()?
                    .context("successful install did not retain a native release pointer")?;
                if let Err(error) = self.prune_native_releases(&active) {
                    outcome
                        .warnings
                        .push(format!("native release cleanup deferred: {error:#}"));
                }
                if let Some(component) = self.read_component_pointer()?
                    && let Err(error) = self.prune_component_releases(&component)
                {
                    outcome
                        .warnings
                        .push(format!("OMP adapter cleanup deferred: {error:#}"));
                }
                Ok(outcome)
            }
            Err(error) => {
                let original = format!("{error:#}");
                let restoration = self.restore_failed_install(&rollback);
                match restoration {
                    Ok(()) => Err(anyhow::anyhow!(
                        "install failed and the prior installation was restored: {original}"
                    )),
                    Err(restoration) => Err(anyhow::anyhow!(
                        "install failed: {original}; restoration failures: {restoration:#}"
                    )),
                }
            }
        }
    }

    pub fn rollback(&self) -> Result<CurrentRelease> {
        let _operation = OperationLock::acquire()?;
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
        let previous_manifest = self.read_verified_native_manifest(&previous)?;
        let (component_before, component_next) = self.component_transition_for(
            &previous_manifest,
            &self
                .layout
                .version(&previous)
                .join("components/omp-adapter"),
        )?;
        let undo_backup = self.runtime.backup_database(&self.layout.backups())?;
        self.services.stop(SERVICE_NAME)?;
        let rolled_back = CurrentRelease {
            version: previous,
            previous_version: Some(current.version.clone()),
            rollback_backup: undo_backup.clone(),
        };
        if let Err(error) = self.write_current(&rolled_back) {
            self.restore_activation_pointers(Some(&current), component_before.as_ref())?;
            self.services.start(SERVICE_NAME).ok();
            return Err(error).context(
                "native rollback activation failed; restored native and component pointers",
            );
        }
        if let Some(component) = &component_next {
            if let Err(error) = self.write_component_pointer(component) {
                self.restore_activation_pointers(Some(&current), component_before.as_ref())?;
                self.services.start(SERVICE_NAME).ok();
                return Err(error).context(
                    "OMP adapter rollback activation failed; restored native and component pointers",
                );
            }
        }
        if let Err(error) = self.runtime.restore_database(&previous_backup) {
            self.restore_activation_pointers(Some(&current), component_before.as_ref())?;
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
            self.restore_activation_pointers(Some(&current), component_before.as_ref())?;
            if let Some(backup) = &undo_backup {
                self.runtime.restore_database(backup)?;
            }
            self.services.start(SERVICE_NAME).ok();
            return Err(error).context("rollback release was not ready; restored newer release");
        }
        self.prune_native_releases(&rolled_back)?;
        if let Some(component) = self.read_component_pointer()? {
            self.prune_component_releases(&component)?;
        }
        Ok(rolled_back)
    }

    pub fn install_omp_adapter(&self, source: &Path) -> Result<ComponentPointer> {
        let _operation = OperationLock::acquire()?;
        let native = self
            .read_current()?
            .context("cannot install OMP adapter without an active native release")?;
        let native_manifest = self.read_native_manifest_metadata(&native.version)?;
        let component = read_verified_component(self.fs, source)?;
        self.require_component_compatibility(&component, &native_manifest)?;
        self.retain_component_release(source, &component)?;

        let previous = self.read_component_pointer()?;
        if let Some(active) = previous
            .as_ref()
            .filter(|pointer| pointer.release_id == component.release_id)
        {
            self.prune_component_releases(active)?;
            return Ok(active.clone());
        }
        let next = ComponentPointer {
            format: COMPONENT_FORMAT,
            release_id: component.release_id,
            previous_release_id: previous.as_ref().map(|pointer| pointer.release_id.clone()),
        };
        if let Err(error) = self.write_component_pointer(&next) {
            self.restore_component_pointer(previous.as_ref())?;
            return Err(error).context("OMP adapter activation failed; restored prior pointer");
        }
        self.prune_component_releases(&next)?;
        Ok(next)
    }

    pub fn rollback_omp_adapter(&self, release_id: Option<&str>) -> Result<ComponentPointer> {
        let _operation = OperationLock::acquire()?;
        let current = self
            .read_component_pointer()?
            .context("there is no active OMP adapter release")?;
        let target_release = release_id
            .map(str::to_owned)
            .or_else(|| current.previous_release_id.clone())
            .context("there is no retained OMP adapter rollback release")?;
        ComponentPointer {
            format: COMPONENT_FORMAT,
            release_id: target_release.clone(),
            previous_release_id: None,
        }
        .validate()?;
        if target_release == current.release_id {
            bail!("OMP adapter release {target_release} is already active");
        }

        let target_root = self.layout.omp_adapter_version(&target_release);
        let target = read_verified_component(self.fs, &target_root)
            .with_context(|| format!("OMP adapter rollback release {target_release} is invalid"))?;
        if target.release_id != target_release {
            bail!(
                "OMP adapter rollback target identity mismatch: pointer names {target_release}, manifest names {}",
                target.release_id
            );
        }
        let native = self
            .read_current()?
            .context("cannot roll back OMP adapter without an active native release")?;
        let native_manifest = self.read_native_manifest_metadata(&native.version)?;
        self.require_component_compatibility(&target, &native_manifest)?;

        let next = ComponentPointer {
            format: COMPONENT_FORMAT,
            release_id: target_release,
            previous_release_id: Some(current.release_id.clone()),
        };
        if let Err(error) = self.write_component_pointer(&next) {
            self.restore_component_pointer(Some(&current))?;
            return Err(error).context("OMP adapter rollback failed; restored prior pointer");
        }
        self.prune_component_releases(&next)?;
        Ok(next)
    }

    fn read_component_pointer(&self) -> Result<Option<ComponentPointer>> {
        let path = self.layout.omp_adapter_current();
        if !self.fs.exists(&path) {
            return Ok(None);
        }
        self.fs
            .validate_regular_file(&self.layout.omp_adapter(), &path)?;
        let pointer: ComponentPointer = serde_json::from_slice(&self.fs.read(&path)?)
            .with_context(|| format!("parse component pointer {}", path.display()))?;
        pointer.validate()?;
        let manifest = read_verified_component(
            self.fs,
            &self.layout.omp_adapter_version(&pointer.release_id),
        )
        .with_context(|| {
            format!(
                "active OMP adapter release {} is missing or invalid",
                pointer.release_id
            )
        })?;
        if manifest.release_id != pointer.release_id {
            bail!(
                "active OMP adapter pointer names {}, manifest names {}",
                pointer.release_id,
                manifest.release_id
            );
        }
        if let Some(previous) = &pointer.previous_release_id {
            let previous_manifest =
                read_verified_component(self.fs, &self.layout.omp_adapter_version(previous))
                    .with_context(|| {
                        format!("previous OMP adapter release {previous} is missing or invalid")
                    })?;
            if previous_manifest.release_id.as_str() != previous.as_str() {
                bail!(
                    "previous OMP adapter pointer names {}, manifest names {}",
                    previous,
                    previous_manifest.release_id
                );
            }
        }
        Ok(Some(pointer))
    }

    fn write_component_pointer(&self, pointer: &ComponentPointer) -> Result<()> {
        pointer.validate()?;
        self.fs.write_atomic(
            &self.layout.omp_adapter_current(),
            &serde_json::to_vec_pretty(pointer)?,
        )
    }

    fn restore_component_pointer(&self, pointer: Option<&ComponentPointer>) -> Result<()> {
        match pointer {
            Some(pointer) => self.write_component_pointer(pointer),
            None => self.fs.remove_file(&self.layout.omp_adapter_current()),
        }
    }

    fn read_native_manifest_metadata(&self, version: &str) -> Result<ReleaseManifest> {
        if !safe_version(version) {
            bail!("invalid native release version {version:?}");
        }
        let root = self.layout.version(version);
        let path = root.join("release-manifest.json");
        self.fs.validate_regular_file(&root, &path)?;
        let manifest: ReleaseManifest = serde_json::from_slice(&self.fs.read(&path)?)
            .with_context(|| format!("parse native release manifest {}", path.display()))?;
        manifest.validate()?;
        if manifest.version != version {
            bail!(
                "native release pointer names {version}, manifest names {}",
                manifest.version
            );
        }
        Ok(manifest)
    }

    fn read_verified_native_manifest(&self, version: &str) -> Result<ReleaseManifest> {
        let manifest = self.read_native_manifest_metadata(version)?;
        self.verify_native_release(&self.layout.version(version), &manifest)?;
        Ok(manifest)
    }

    fn require_component_compatibility(
        &self,
        component: &ComponentManifest,
        native: &ReleaseManifest,
    ) -> Result<()> {
        if !component.compatibility.matches_native(native) {
            bail!(
                "OMP adapter {} is incompatible with native release {}: adapter {}; native hostApi={}, substrateApi={}, deliveryApi={}, schemaVersion={}",
                component.release_id,
                native.version,
                component.compatibility.describe(),
                native.compatibility.host_api,
                native.compatibility.substrate_api,
                native.compatibility.delivery_api,
                native.schema_version,
            );
        }
        Ok(())
    }

    fn retain_component_release(&self, source: &Path, component: &ComponentManifest) -> Result<()> {
        let target = self.layout.omp_adapter_version(&component.release_id);
        if self.fs.exists(&target) {
            let retained = read_verified_component(self.fs, &target).with_context(|| {
                format!(
                    "retained OMP adapter release {} is invalid",
                    component.release_id
                )
            })?;
            if retained != *component {
                bail!(
                    "retained OMP adapter release {} differs from the source manifest",
                    component.release_id
                );
            }
            return Ok(());
        }

        self.fs
            .create_dir_all(&self.layout.omp_adapter_versions())?;
        let mut nonce = [0_u8; 12];
        getrandom::fill(&mut nonce)
            .map_err(|error| anyhow::anyhow!("generate component staging identity: {error}"))?;
        let pending = self.layout.omp_adapter_versions().join(format!(
            "{}.pending-{}",
            component.release_id,
            hex::encode(nonce)
        ));
        self.fs.remove_tree(&pending)?;
        self.fs.create_dir_all(&pending)?;
        let staged = (|| -> Result<()> {
            for artifact in &component.artifacts {
                self.fs
                    .copy(&source.join(&artifact.path), &pending.join(&artifact.path))?;
            }
            self.fs.copy(
                &source.join(COMPONENT_MANIFEST),
                &pending.join(COMPONENT_MANIFEST),
            )?;
            let verified = read_verified_component(self.fs, &pending)?;
            if verified != *component {
                bail!(
                    "staged OMP adapter release {} differs from the source manifest",
                    component.release_id
                );
            }
            self.fs.rename(&pending, &target)
        })();
        if staged.is_err() {
            self.fs.remove_tree(&pending).ok();
        }
        staged
    }

    fn prune_native_releases(&self, pointer: &CurrentRelease) -> Result<()> {
        self.read_verified_native_manifest(&pointer.version)?;
        if let Some(previous) = &pointer.previous_version {
            self.read_verified_native_manifest(previous)?;
        }

        let mut failures = Vec::new();
        for directory in self.fs.list_directories(&self.layout.versions())? {
            if directory.parent() != Some(self.layout.versions().as_path()) {
                continue;
            }
            let Some(version) = directory.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !safe_version(version)
                || version.eq_ignore_ascii_case(&pointer.version)
                || pointer
                    .previous_version
                    .as_deref()
                    .is_some_and(|previous| previous.eq_ignore_ascii_case(version))
            {
                continue;
            }
            if let Err(error) = self.fs.remove_tree(&directory) {
                failures.push(format!("{}: {error:#}", directory.display()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!(
                "failed to prune stale native releases: {}",
                failures.join("; ")
            )
        }
    }

    fn prune_component_releases(&self, pointer: &ComponentPointer) -> Result<()> {
        for release_id in std::iter::once(pointer.release_id.as_str())
            .chain(pointer.previous_release_id.as_deref())
        {
            let retained =
                read_verified_component(self.fs, &self.layout.omp_adapter_version(release_id))?;
            if retained.release_id != release_id {
                bail!(
                    "retained OMP adapter pointer names {release_id}, manifest names {}",
                    retained.release_id
                );
            }
        }

        let mut failures = Vec::new();
        for directory in self
            .fs
            .list_directories(&self.layout.omp_adapter_versions())?
        {
            if directory.parent() != Some(self.layout.omp_adapter_versions().as_path()) {
                continue;
            }
            let Some(release_id) = directory.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !valid_release_id(release_id)
                || release_id.eq_ignore_ascii_case(&pointer.release_id)
                || pointer
                    .previous_release_id
                    .as_deref()
                    .is_some_and(|previous| previous.eq_ignore_ascii_case(release_id))
            {
                continue;
            }
            if let Err(error) = self.fs.remove_tree(&directory) {
                failures.push(format!("{}: {error:#}", directory.display()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!(
                "failed to prune stale OMP adapter releases: {}",
                failures.join("; ")
            )
        }
    }

    fn component_transition_for(
        &self,
        native: &ReleaseManifest,
        fallback: &Path,
    ) -> Result<(Option<ComponentPointer>, Option<ComponentPointer>)> {
        let current = self.read_component_pointer()?;
        if let Some(pointer) = &current {
            let manifest = read_verified_component(
                self.fs,
                &self.layout.omp_adapter_version(&pointer.release_id),
            )?;
            if manifest.compatibility.matches_native(native) {
                return Ok((current, None));
            }
        }

        let component = read_verified_component(self.fs, fallback).with_context(|| {
            format!(
                "native release {} has no usable OMP adapter fallback at {}",
                native.version,
                fallback.display()
            )
        })?;
        self.require_component_compatibility(&component, native)?;
        self.retain_component_release(fallback, &component)?;
        let next = ComponentPointer {
            format: COMPONENT_FORMAT,
            release_id: component.release_id,
            previous_release_id: current.as_ref().map(|pointer| pointer.release_id.clone()),
        };
        Ok((current, Some(next)))
    }

    fn restore_native_pointer(&self, current: Option<&CurrentRelease>) -> Result<()> {
        match current {
            Some(current) => self.write_current(current),
            None => self.fs.remove_file(&self.layout.current()),
        }
    }
    fn restore_activation_pointers(
        &self,
        native: Option<&CurrentRelease>,
        component: Option<&ComponentPointer>,
    ) -> Result<()> {
        let mut failures = Vec::new();
        if let Err(error) = self.restore_native_pointer(native) {
            failures.push(format!("native pointer: {error:#}"));
        }
        if let Err(error) = self.restore_component_pointer(component) {
            failures.push(format!("component pointer: {error:#}"));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!("{}", failures.join("; "))
        }
    }

    fn snapshot_file(&self, path: PathBuf) -> Result<FileSnapshot> {
        let bytes = if self.fs.exists(&path) {
            Some(self.fs.read(&path)?)
        } else {
            None
        };
        Ok(FileSnapshot { path, bytes })
    }

    fn capture_install_rollback(
        &self,
        request: &InstallRequest,
        house: &HouseInstallConfig,
        fallback_component: &ComponentManifest,
        had_current_release: bool,
    ) -> Result<InstallRollback> {
        for (root, path) in [
            (self.layout.program.clone(), self.layout.current()),
            (self.layout.omp_adapter(), self.layout.omp_adapter_current()),
            (self.layout.program.clone(), self.layout.manager()),
            (self.layout.program.clone(), self.layout.omp_loader()),
        ] {
            if self.fs.exists(&path) {
                self.fs.validate_regular_file(&root, &path)?;
            }
        }
        let mut paths = vec![
            self.layout.current(),
            self.layout.omp_adapter_current(),
            self.layout.config(),
            self.layout.secrets(),
            self.layout.manager(),
            self.layout.omp_loader(),
        ];
        paths.push(self.layout.legacy_backup().join("imported.json"));
        paths.extend(house.rooms.iter().map(|room| {
            house
                .rooms_root
                .join(&room.room)
                .join(".omp/runtime/athanor-house-state.json")
        }));
        if let Some(integration) = &request.operator_integration {
            paths.push(integration.omp_config_path.clone());
            paths.push(integration.client_config_path.clone());
        }
        let files = paths
            .into_iter()
            .map(|path| self.snapshot_file(path))
            .collect::<Result<Vec<_>>>()?;
        let target = self.layout.version(&request.manifest.version);
        let staging_target = self
            .layout
            .versions()
            .join(format!(".{}.staging", request.manifest.version));
        let component_target = self
            .layout
            .omp_adapter_version(&fallback_component.release_id);
        Ok(InstallRollback {
            files,
            database_backup: None,
            target,
            staging_target,
            database_restore_uses_attempted_config: !had_current_release
                && request.external_database_url.is_some(),
            component_target_existed: self.fs.exists(&component_target),
            component_target,
            service_was_installed: self.services.is_installed(SERVICE_NAME)?,
        })
    }

    fn restore_file_snapshot(&self, snapshot: &FileSnapshot) -> Result<()> {
        match &snapshot.bytes {
            Some(bytes) => self.fs.write_atomic(&snapshot.path, bytes),
            None => self.fs.remove_file(&snapshot.path),
        }
    }

    fn restore_failed_install(&self, rollback: &InstallRollback) -> Result<()> {
        let mut failures = Vec::new();
        let mut attempt = |label: &str, result: Result<()>| {
            if let Err(error) = result {
                failures.push(format!("{label}: {error:#}"));
            }
        };

        attempt("stop failed service", self.services.stop(SERVICE_NAME));
        for snapshot in rollback.files.iter().filter(|snapshot| {
            snapshot.path == self.layout.current()
                || snapshot.path == self.layout.omp_adapter_current()
        }) {
            attempt(
                &format!("restore {}", snapshot.path.display()),
                self.restore_file_snapshot(snapshot),
            );
        }
        if rollback.database_restore_uses_attempted_config {
            if let Some(backup) = &rollback.database_backup {
                attempt(
                    "restore database backup",
                    self.runtime.restore_database(backup),
                );
            }
        }
        for snapshot in rollback.files.iter().filter(|snapshot| {
            snapshot.path == self.layout.config() || snapshot.path == self.layout.secrets()
        }) {
            attempt(
                &format!("restore {}", snapshot.path.display()),
                self.restore_file_snapshot(snapshot),
            );
        }
        if !rollback.database_restore_uses_attempted_config {
            if let Some(backup) = &rollback.database_backup {
                attempt(
                    "restore database backup",
                    self.runtime.restore_database(backup),
                );
            }
        }
        for snapshot in rollback.files.iter().filter(|snapshot| {
            snapshot.path != self.layout.current()
                && snapshot.path != self.layout.omp_adapter_current()
                && snapshot.path != self.layout.config()
                && snapshot.path != self.layout.secrets()
        }) {
            attempt(
                &format!("restore {}", snapshot.path.display()),
                self.restore_file_snapshot(snapshot),
            );
        }
        attempt(
            "remove failed native staging release",
            self.fs.remove_tree(&rollback.staging_target),
        );
        attempt(
            "remove failed native release",
            self.fs.remove_tree(&rollback.target),
        );
        if !rollback.component_target_existed {
            attempt(
                "remove failed component release",
                self.fs.remove_tree(&rollback.component_target),
            );
        }

        if rollback.service_was_installed {
            attempt(
                "restore service registration",
                self.services.install_or_update(
                    SERVICE_NAME,
                    SERVICE_DISPLAY_NAME,
                    &self.layout.manager(),
                ),
            );
            attempt("restart prior service", self.services.start(SERVICE_NAME));
            attempt("restore prior service readiness", self.runtime.wait_ready());
        } else {
            attempt(
                "remove newly installed service",
                self.services.remove(SERVICE_NAME),
            );
        }

        if failures.is_empty() {
            Ok(())
        } else {
            bail!("{}", failures.join("; "))
        }
    }

    fn verify_native_release(&self, root: &Path, manifest: &ReleaseManifest) -> Result<()> {
        let manifest_path = root.join("release-manifest.json");
        self.fs.validate_regular_file(root, &manifest_path)?;
        let retained: ReleaseManifest = serde_json::from_slice(&self.fs.read(&manifest_path)?)
            .with_context(|| {
                format!("parse native release manifest {}", manifest_path.display())
            })?;
        retained.validate()?;
        if retained != *manifest {
            bail!(
                "native release at {} differs from its declared manifest",
                root.display()
            );
        }
        for artifact in &manifest.artifacts {
            let path = root.join(&artifact.path);
            self.fs.validate_regular_file(root, &path)?;
            let bytes = self.fs.read(&path)?;
            manifest.verify_bytes(artifact, &bytes)?;
        }
        Ok(())
    }

    pub fn uninstall(&self) -> Result<()> {
        let _operation = OperationLock::acquire()?;
        self.uninstall_locked()
    }

    fn uninstall_locked(&self) -> Result<()> {
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
        let _operation = OperationLock::acquire()?;
        if !confirmed {
            bail!("purge requires --confirm-data-loss");
        }
        self.uninstall_locked()?;
        self.fs.remove_tree(&self.layout.data)
    }

    fn preflight(&self, request: &InstallRequest, house: &HouseInstallConfig) -> Result<()> {
        for artifact in &request.manifest.artifacts {
            let source = request.staging.join(&artifact.path);
            self.fs.validate_regular_file(&request.staging, &source)?;
            let bytes = self
                .fs
                .read(&source)
                .with_context(|| format!("read staged artifact {}", artifact.path))?;
            request.manifest.verify_bytes(artifact, &bytes)?;
        }
        let fallback_root = request.staging.join("components/omp-adapter");
        let fallback = read_verified_component(self.fs, &fallback_root).with_context(|| {
            format!(
                "native release {} has no valid OMP adapter fallback at {}",
                request.manifest.version,
                fallback_root.display()
            )
        })?;
        self.require_component_compatibility(&fallback, &request.manifest)?;
        if house.rooms_root != self.layout.rooms() {
            for room in &house.rooms {
                let state = house
                    .rooms_root
                    .join(&room.room)
                    .join(".omp/runtime/athanor-house-state.json");
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
            integration
                .client_config_path
                .parent()
                .context("OMP client projection has no parent directory")?;
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
        let path = self.layout.current();
        if !self.fs.exists(&path) {
            return Ok(None);
        }
        self.fs.validate_regular_file(&self.layout.program, &path)?;
        let current: CurrentRelease = serde_json::from_slice(&self.fs.read(&path)?)
            .context("parse current release pointer")?;
        if !safe_version(&current.version)
            || current
                .previous_version
                .as_ref()
                .is_some_and(|version| !safe_version(version) || version == &current.version)
        {
            bail!("current release pointer contains an invalid version lineage");
        }
        Ok(Some(current))
    }

    fn write_current(&self, current: &CurrentRelease) -> Result<()> {
        if !safe_version(&current.version)
            || current
                .previous_version
                .as_ref()
                .is_some_and(|version| !safe_version(version) || version == &current.version)
        {
            bail!("refusing to write an invalid current release pointer");
        }
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
            database_host: LOOPBACK_HOST.into(),
            database_port: MANAGED_DATABASE_PORT,
            nats_host: LOOPBACK_HOST.into(),
            nats_port: MANAGED_NATS_PORT,
            host_health: format!("http://{LOOPBACK_HOST}:{default_port}/health"),
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
                        url: crate::endpoints::host_ws_url(room.port),
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
        self.fs
            .write_atomic(&integration.omp_config_path, updated.as_bytes())
            .context("register stable OMP loader")?;
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
