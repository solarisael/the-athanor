use anyhow::{Context, Result, bail};
use athanor_install::{
    boundaries::{
        FileSystem, NativeFileSystem, OperationLock, RuntimeControl, SecretSource, ServiceManager,
    },
    component::{
        ComponentArtifact, ComponentCompatibility, ComponentManifest, ComponentPointer,
        read_verified_component,
    },
    doctor,
    installer::{
        CurrentRelease, HouseInstallConfig, InstallRequest, Installer, OperatorIntegration,
    },
    layout::{InstallLayout, SERVICE_NAME, safe_version},
    manifest::{Artifact, Compatibility, ReleaseManifest, RollbackContract},
    supervisor::HostRoomConfig,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct FakeFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    reads: RefCell<Vec<PathBuf>>,
    dirs: RefCell<BTreeSet<PathBuf>>,
    acls: RefCell<Vec<PathBuf>>,
    fail_atomic_once: RefCell<Option<PathBuf>>,
    fail_remove_tree_once: RefCell<Option<PathBuf>>,
}
impl FileSystem for FakeFs {
    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path) || self.dirs.borrow().contains(path)
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push(path.to_path_buf());
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
    fn list_directories(&self, path: &Path) -> Result<Vec<PathBuf>> {
        Ok(self
            .dirs
            .borrow()
            .iter()
            .filter(|directory| directory.parent() == Some(path))
            .cloned()
            .collect())
    }
    fn create_dir_all(&self, path: &Path) -> Result<()> {
        for ancestor in path.ancestors() {
            self.dirs.borrow_mut().insert(ancestor.into());
        }
        Ok(())
    }
    fn copy(&self, from: &Path, to: &Path) -> Result<()> {
        let bytes = self.read(from)?;
        self.files.borrow_mut().insert(to.into(), bytes);
        Ok(())
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        if self
            .fail_atomic_once
            .borrow()
            .as_ref()
            .is_some_and(|failed| failed == path)
        {
            self.fail_atomic_once.borrow_mut().take();
            bail!("injected atomic write failure for {}", path.display());
        }
        self.files.borrow_mut().insert(path.into(), bytes.into());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let moved = self
            .files
            .borrow()
            .iter()
            .filter(|(path, _)| path.starts_with(from))
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect::<Vec<_>>();
        for (path, bytes) in moved {
            self.files.borrow_mut().remove(&path);
            self.files
                .borrow_mut()
                .insert(to.join(path.strip_prefix(from)?), bytes);
        }
        self.dirs.borrow_mut().insert(to.into());
        Ok(())
    }
    fn remove_file(&self, path: &Path) -> Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }
    fn remove_tree(&self, path: &Path) -> Result<()> {
        if self
            .fail_remove_tree_once
            .borrow()
            .as_ref()
            .is_some_and(|failed| failed == path)
        {
            self.fail_remove_tree_once.borrow_mut().take();
            bail!("injected tree removal failure for {}", path.display());
        }
        self.files
            .borrow_mut()
            .retain(|candidate, _| !candidate.starts_with(path));
        self.dirs
            .borrow_mut()
            .retain(|candidate| !candidate.starts_with(path));
        Ok(())
    }
    fn restrict_acl(&self, path: &Path) -> Result<()> {
        self.acls.borrow_mut().push(path.into());
        Ok(())
    }
    fn restrict_user_acl(&self, path: &Path, _: &str) -> Result<()> {
        self.acls.borrow_mut().push(path.into());
        Ok(())
    }
}

#[derive(Default)]
struct FakeServices {
    installed: RefCell<bool>,
    events: RefCell<Vec<String>>,
}
impl ServiceManager for FakeServices {
    fn install_or_update(&self, name: &str, _: &str, _: &Path) -> Result<()> {
        *self.installed.borrow_mut() = true;
        self.events.borrow_mut().push(format!("install:{name}"));
        Ok(())
    }
    fn start(&self, name: &str) -> Result<()> {
        self.events.borrow_mut().push(format!("start:{name}"));
        Ok(())
    }
    fn stop(&self, name: &str) -> Result<()> {
        self.events.borrow_mut().push(format!("stop:{name}"));
        Ok(())
    }
    fn remove(&self, name: &str) -> Result<()> {
        *self.installed.borrow_mut() = false;
        self.events.borrow_mut().push(format!("remove:{name}"));
        Ok(())
    }
    fn is_installed(&self, _: &str) -> Result<bool> {
        Ok(*self.installed.borrow())
    }
}

#[derive(Default)]
struct FakeRuntime {
    events: RefCell<Vec<String>>,
    fail_ready_once: RefCell<bool>,
    fail_restore_once: RefCell<bool>,
}
impl RuntimeControl for FakeRuntime {
    fn backup_database(&self, directory: &Path) -> Result<Option<PathBuf>> {
        self.events.borrow_mut().push("backup".into());
        Ok(Some(directory.join("backup.manifest.json")))
    }
    fn import_legacy(&self, _: &Path, _: &Path) -> Result<()> {
        self.events.borrow_mut().push("legacy".into());
        Ok(())
    }
    fn migrate_database(&self) -> Result<()> {
        self.events.borrow_mut().push("migrate".into());
        Ok(())
    }
    fn restore_database(&self, _: &Path) -> Result<()> {
        self.events.borrow_mut().push("restore".into());
        if self.fail_restore_once.replace(false) {
            bail!("injected database restoration failure")
        } else {
            Ok(())
        }
    }
    fn wait_ready(&self) -> Result<()> {
        self.events.borrow_mut().push("ready".into());
        if self.fail_ready_once.replace(false) {
            bail!("not ready")
        } else {
            Ok(())
        }
    }
}
struct FixedSecrets;
impl SecretSource for FixedSecrets {
    fn fill(&self, bytes: &mut [u8]) -> Result<()> {
        bytes.fill(7);
        Ok(())
    }
}

fn component(bytes: &[u8]) -> ComponentManifest {
    let mut manifest = ComponentManifest {
        format: 1,
        component: "omp-adapter".into(),
        version: "0.9.3".into(),
        release_id: String::new(),
        compatibility: ComponentCompatibility {
            host_api: 1,
            substrate_api: 1,
            delivery_api: 1,
            schema_version: 18,
        },
        artifacts: vec![ComponentArtifact {
            path: "index.ts".into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            size: bytes.len() as u64,
        }],
    };
    manifest.release_id = manifest.computed_release_id();
    manifest
}

fn release(version: &str, bytes: &[u8]) -> ReleaseManifest {
    let adapter = b"adapter";
    let component_manifest = serde_json::to_vec_pretty(&component(adapter)).unwrap();
    ReleaseManifest {
        format: 1,
        product: "the-athanor".into(),
        version: version.into(),
        platform: "windows-x64".into(),
        schema_version: 18,
        compatibility: Compatibility {
            host_api: 1,
            substrate_api: 1,
            delivery_api: 1,
            godot_api: "4.7".into(),
            godot: "4.7.1-stable".into(),
            postgresql: "18.4-2".into(),
            pgvector: "0.8.6".into(),
            nats_server: "2.14.4".into(),
        },
        artifacts: vec![
            Artifact {
                component: "installer".into(),
                path: "bin/athanor-manage.exe".into(),
                sha256: hex::encode(Sha256::digest(bytes)),
                size: bytes.len() as u64,
                executable: true,
            },
            Artifact {
                component: "omp-adapter".into(),
                path: "components/omp-adapter/component-manifest.json".into(),
                sha256: hex::encode(Sha256::digest(&component_manifest)),
                size: component_manifest.len() as u64,
                executable: false,
            },
            Artifact {
                component: "omp-adapter".into(),
                path: "components/omp-adapter/index.ts".into(),
                sha256: hex::encode(Sha256::digest(adapter)),
                size: adapter.len() as u64,
                executable: false,
            },
        ],
        rollback: RollbackContract {
            database_restore_required: true,
            minimum_retained_versions: 2,
        },
    }
}

fn stage_release(fs: &FakeFs, staging: &Path, manager: &[u8]) {
    let adapter = b"adapter";
    fs.files
        .borrow_mut()
        .insert(staging.join("bin/athanor-manage.exe"), manager.to_vec());
    fs.files.borrow_mut().insert(
        staging.join("components/omp-adapter/component-manifest.json"),
        serde_json::to_vec_pretty(&component(adapter)).unwrap(),
    );
    fs.files.borrow_mut().insert(
        staging.join("components/omp-adapter/index.ts"),
        adapter.to_vec(),
    );
}

fn stage_component(fs: &FakeFs, root: &Path, bytes: &[u8]) -> ComponentManifest {
    let manifest = component(bytes);
    fs.dirs.borrow_mut().insert(root.to_path_buf());
    fs.files
        .borrow_mut()
        .insert(root.join("index.ts"), bytes.to_vec());
    fs.files.borrow_mut().insert(
        root.join("component-manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    );
    manifest
}

#[test]
fn install_verifies_before_mutation_and_uninstall_preserves_data() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let staging = PathBuf::from("C:/stage");
    let bytes = b"native-manager";
    stage_release(&fs, &staging, bytes);
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    let outcome = installer.install(InstallRequest {
        staging,
        manifest: release("1.0.0", bytes),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    assert_eq!(outcome.version, "1.0.0");
    assert!(fs.exists(&layout.secrets()));
    assert_eq!(runtime.events.borrow().as_slice(), ["migrate", "ready"]);
    installer.uninstall()?;
    assert!(!fs.exists(&layout.program));
    assert!(fs.exists(&layout.data));
    assert_eq!(
        services.events.borrow().last().unwrap(),
        &format!("remove:{SERVICE_NAME}")
    );
    Ok(())
}

#[test]
fn checksum_failure_does_not_create_install_directories() {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let staging = PathBuf::from("C:/stage");
    stage_release(&fs, &staging, b"tampered");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    assert!(
        installer
            .install(InstallRequest {
                staging,
                manifest: release("1.0.0", b"expected"),
                external_database_url: None,
                house_config: None,
                operator_integration: None,
            })
            .is_err()
    );
    assert!(!fs.exists(&layout.program));
    assert!(!fs.exists(&layout.data));
}

#[test]
fn purge_is_a_separate_confirmed_contract() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    fs.dirs.borrow_mut().insert(layout.program.clone());
    fs.dirs.borrow_mut().insert(layout.data.clone());
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    assert!(installer.purge(false).is_err());
    assert!(fs.exists(&layout.data));
    installer.purge(true)?;
    assert!(!fs.exists(&layout.data));
    Ok(())
}

#[test]
fn upgrade_records_database_backup_and_rollback_restores_it() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let one = PathBuf::from("C:/stage-one");
    let two = PathBuf::from("C:/stage-two");
    stage_release(&fs, &one, b"one");
    stage_release(&fs, &two, b"two");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: one,
        manifest: release("1.0.0", b"one"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    installer.install(InstallRequest {
        staging: two,
        manifest: release("2.0.0", b"two"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    let rolled_back = installer.rollback()?;
    assert_eq!(rolled_back.version, "1.0.0");
    assert!(
        runtime
            .events
            .borrow()
            .iter()
            .filter(|event| event.as_str() == "backup")
            .count()
            >= 2
    );
    assert!(
        runtime
            .events
            .borrow()
            .iter()
            .any(|event| event == "restore")
    );
    Ok(())
}

#[test]
fn native_retention_waits_for_activation_and_catches_up_to_the_pointer_pair() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let stages = [
        PathBuf::from("C:/stage-one"),
        PathBuf::from("C:/stage-two"),
        PathBuf::from("C:/stage-three"),
    ];
    for (stage, bytes) in stages.iter().zip([b"one".as_slice(), b"two", b"three"]) {
        stage_release(&fs, stage, bytes);
    }
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    for (stage, version, bytes) in [
        (&stages[0], "1.0.0", b"one".as_slice()),
        (&stages[1], "2.0.0", b"two".as_slice()),
    ] {
        installer.install(InstallRequest {
            staging: stage.clone(),
            manifest: release(version, bytes),
            external_database_url: None,
            house_config: None,
            operator_integration: None,
        })?;
    }

    let historical = layout.version("0.8.0");
    fs.dirs.borrow_mut().insert(historical.clone());
    *runtime.fail_ready_once.borrow_mut() = true;
    assert!(
        installer
            .install(InstallRequest {
                staging: stages[2].clone(),
                manifest: release("3.0.0", b"three"),
                external_database_url: None,
                house_config: None,
                operator_integration: None,
            })
            .is_err()
    );
    assert!(fs.exists(&historical));
    assert!(fs.exists(&layout.version("1.0.0")));
    assert!(fs.exists(&layout.version("2.0.0")));
    assert!(!fs.exists(&layout.version("3.0.0")));

    installer.install(InstallRequest {
        staging: stages[2].clone(),
        manifest: release("3.0.0", b"three"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    let pointer: CurrentRelease = serde_json::from_slice(&fs.read(&layout.current())?)?;
    assert_eq!(pointer.version, "3.0.0");
    assert_eq!(pointer.previous_version.as_deref(), Some("2.0.0"));
    assert!(fs.exists(&layout.version("3.0.0")));
    assert!(fs.exists(&layout.version("2.0.0")));
    assert!(!fs.exists(&layout.version("1.0.0")));
    assert!(!fs.exists(&historical));
    Ok(())
}

#[test]
fn native_cleanup_failure_is_reported_without_reverting_durable_pointers() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let stages = [
        PathBuf::from("C:/stage-one"),
        PathBuf::from("C:/stage-two"),
        PathBuf::from("C:/stage-three"),
    ];
    for (stage, bytes) in stages.iter().zip([b"one".as_slice(), b"two", b"three"]) {
        stage_release(&fs, stage, bytes);
    }
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    for (stage, version, bytes) in [
        (&stages[0], "1.0.0", b"one".as_slice()),
        (&stages[1], "2.0.0", b"two".as_slice()),
    ] {
        installer.install(InstallRequest {
            staging: stage.clone(),
            manifest: release(version, bytes),
            external_database_url: None,
            house_config: None,
            operator_integration: None,
        })?;
    }

    *fs.fail_remove_tree_once.borrow_mut() = Some(layout.version("1.0.0"));
    let error = installer
        .install(InstallRequest {
            staging: stages[2].clone(),
            manifest: release("3.0.0", b"three"),
            external_database_url: None,
            house_config: None,
            operator_integration: None,
        })
        .expect_err("stale release cleanup failure must be surfaced");
    assert!(format!("{error:#}").contains("failed to prune stale native releases"));
    let pointer: CurrentRelease = serde_json::from_slice(&fs.read(&layout.current())?)?;
    assert_eq!(pointer.version, "3.0.0");
    assert_eq!(pointer.previous_version.as_deref(), Some("2.0.0"));
    assert!(fs.exists(&layout.version("3.0.0")));
    assert!(fs.exists(&layout.version("2.0.0")));
    assert!(fs.exists(&layout.version("1.0.0")));
    Ok(())
}

#[test]
fn manifest_rejects_an_unpinned_godot_runtime() {
    let mut manifest = release("1.0.0", b"native-manager");
    manifest.compatibility.godot = "4.8-stable".into();

    assert!(manifest.validate().is_err());
}

#[test]
fn install_registers_one_stable_loader_and_uninstall_removes_only_owned_files() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let staging = PathBuf::from("C:/stage");
    stage_release(&fs, &staging, b"native-manager");
    fs.files
        .borrow_mut()
        .insert(layout.omp_loader(), b"stable loader".to_vec());
    let omp_config = PathBuf::from("C:/Users/Sol/.omp/agent/config.yml");
    fs.files.borrow_mut().insert(
        omp_config.clone(),
        b"theme: dark\nextensions:\n  - C:/repo/the-athanor/adapters/omp/index.ts\n  - C:/repo/the-athanor/adapters/omp/hygiene.ts\n  - C:/foreign/extension.ts\n".to_vec(),
    );
    let client_config = PathBuf::from("C:/Users/Sol/.omp/agent/athanor/client.json");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };

    let outcome = installer.install(InstallRequest {
        staging,
        manifest: release("1.0.0", b"native-manager"),
        external_database_url: None,
        house_config: None,
        operator_integration: Some(OperatorIntegration {
            omp_config_path: omp_config.clone(),
            client_config_path: client_config.clone(),
            operator_principal: "SOL\\Sol".into(),
        }),
    })?;

    assert!(outcome.omp_registered);
    let registered = String::from_utf8(fs.read(&omp_config)?)?;
    assert_eq!(registered.matches("athanor-omp-loader.ts").count(), 1);
    assert!(!registered.contains("/repo/the-athanor/adapters/omp"));
    assert!(registered.contains("C:/foreign/extension.ts"));
    assert!(!registered.contains("fixed-secret-material"));
    let client: serde_json::Value = serde_json::from_slice(&fs.read(&client_config)?)?;
    assert_eq!(client["hostToken"], "07".repeat(32));

    installer.uninstall()?;
    let unregistered = String::from_utf8(fs.read(&omp_config)?)?;
    assert!(!unregistered.contains("athanor-omp-loader.ts"));
    assert!(unregistered.contains("C:/foreign/extension.ts"));
    assert!(!fs.exists(&client_config));
    Ok(())
}

#[test]
fn first_external_install_backs_up_authority_before_migration() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let staging = PathBuf::from("C:/stage");
    stage_release(&fs, &staging, b"native-manager");
    fs.files.borrow_mut().insert(
        layout.version("1.0.0").join("orphan.partial"),
        b"partial".to_vec(),
    );
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };

    installer.install(InstallRequest {
        staging,
        manifest: release("1.0.0", b"native-manager"),
        external_database_url: Some("postgresql://external-authority".into()),
        house_config: None,
        operator_integration: None,
    })?;

    assert_eq!(
        runtime.events.borrow().as_slice(),
        ["backup", "migrate", "ready"]
    );
    assert!(!fs.exists(&layout.version("1.0.0").join("orphan.partial")));
    Ok(())
}

#[test]
fn custom_house_config_preserves_real_room_identity_and_rejects_missing_state() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let staging = PathBuf::from("C:/stage");
    stage_release(&fs, &staging, b"native-manager");
    let rooms_root = PathBuf::from("C:/Solarisael/Obsidian/obsidian");
    let state_root = PathBuf::from("C:/Solarisael/Obsidian/obsidian/house/state");
    let house = HouseInstallConfig {
        house_id: "solarisael".into(),
        rooms_root: rooms_root.clone(),
        operator_state_root: state_root.clone(),
        default_room: "kintsu".into(),
        rooms: vec![
            HostRoomConfig {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                port: 8787,
            },
            HostRoomConfig {
                room: "kodo".into(),
                spirit: "Kodo".into(),
                port: 8788,
            },
        ],
    };
    let request = || InstallRequest {
        staging: staging.clone(),
        manifest: release("1.0.0", b"native-manager"),
        external_database_url: Some("postgresql://external-authority".into()),
        house_config: Some(house.clone()),
        operator_integration: None,
    };
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };

    assert!(installer.install(request()).is_err());
    assert!(services.events.borrow().is_empty());
    for room in ["kintsu", "kodo"] {
        fs.files.borrow_mut().insert(
            rooms_root
                .join(room)
                .join(".omp/runtime/solarisael-house-state.json"),
            format!(r#"{{"version":1,"room":"{room}"}}"#).into_bytes(),
        );
    }
    installer.install(request())?;

    let config: serde_json::Value = serde_json::from_slice(&fs.read(&layout.config())?)?;
    assert_eq!(config["houseId"], "solarisael");
    assert_eq!(config["roomsRoot"], rooms_root.display().to_string());
    assert_eq!(
        config["operatorStateRoot"],
        state_root.display().to_string()
    );
    assert_eq!(config["rooms"].as_array().unwrap().len(), 2);
    Ok(())
}

#[cfg(windows)]
#[test]
fn short_operator_name_resolves_to_a_windows_security_principal() -> Result<()> {
    let root = std::env::temp_dir().join(format!(
        "athanor-acl-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(&root)?;
    let principal = std::env::var("USERNAME")?;
    NativeFileSystem.restrict_user_acl(&root, &principal)?;
    let client = root.join("client.json");
    std::fs::write(&client, b"private")?;
    NativeFileSystem.restrict_user_acl(&client, &principal)?;
    assert_eq!(std::fs::read(&client)?, b"private");
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

fn write_component_root(root: &Path, manifest: &ComponentManifest, artifact: &[u8]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    std::fs::write(root.join("index.ts"), artifact)?;
    std::fs::write(
        root.join("component-manifest.json"),
        serde_json::to_vec_pretty(manifest)?,
    )?;
    Ok(())
}

fn write_native_root(root: &Path, manifest: &ReleaseManifest, manager: &[u8]) -> Result<()> {
    let adapter = b"adapter";
    let component_manifest = serde_json::to_vec_pretty(&component(adapter))?;
    for artifact in &manifest.artifacts {
        let bytes: &[u8] = if artifact.path.ends_with("athanor-manage.exe") {
            manager
        } else if artifact.path.ends_with("component-manifest.json") {
            &component_manifest
        } else if artifact.path.ends_with("index.ts") {
            adapter
        } else {
            bail!("test fixture has no bytes for {}", artifact.path);
        };
        let path = root.join(&artifact.path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    std::fs::write(
        root.join("release-manifest.json"),
        serde_json::to_vec_pretty(manifest)?,
    )?;
    Ok(())
}

#[test]
fn component_manifest_identity_paths_and_bytes_are_strict() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("component");
    let manifest = component(b"export default 1");
    write_component_root(&root, &manifest, b"export default 1")?;
    assert_eq!(read_verified_component(&NativeFileSystem, &root)?, manifest);

    std::fs::write(root.join("index.ts"), b"export default 2")?;
    assert!(read_verified_component(&NativeFileSystem, &root).is_err());
    let size_root = temporary.path().join("wrong-size");
    let size_manifest = component(b"x");
    write_component_root(&size_root, &size_manifest, b"xx")?;
    assert!(read_verified_component(&NativeFileSystem, &size_root).is_err());

    let mut unsafe_path = component(b"x");
    unsafe_path.artifacts[0].path = "../index.ts".into();
    unsafe_path.release_id = unsafe_path.computed_release_id();
    assert!(unsafe_path.validate().is_err());

    let mut duplicate = component(b"x");
    duplicate.artifacts = vec![
        ComponentArtifact {
            path: "Index.ts".into(),
            sha256: hex::encode(Sha256::digest(b"x")),
            size: 1,
        },
        ComponentArtifact {
            path: "index.ts".into(),
            sha256: hex::encode(Sha256::digest(b"x")),
            size: 1,
        },
    ];
    duplicate.release_id = duplicate.computed_release_id();
    assert!(duplicate.validate().is_err());

    let mut uppercase_hash = component(b"x");
    uppercase_hash.artifacts[0].sha256 = uppercase_hash.artifacts[0].sha256.to_uppercase();
    uppercase_hash.release_id = uppercase_hash.computed_release_id();
    assert!(uppercase_hash.validate().is_err());

    let mut unsorted = component(b"x");
    unsorted.artifacts = vec![
        ComponentArtifact {
            path: "z.ts".into(),
            sha256: hex::encode(Sha256::digest(b"x")),
            size: 1,
        },
        ComponentArtifact {
            path: "a.ts".into(),
            sha256: hex::encode(Sha256::digest(b"x")),
            size: 1,
        },
    ];
    unsorted.release_id = unsorted.computed_release_id();
    assert!(unsorted.validate().is_err());

    let mut identity = component(b"x");
    identity.release_id = format!("0.9.3-{}", "0".repeat(64));
    assert!(identity.validate().is_err());
    let cycle = ComponentPointer {
        format: 1,
        release_id: component(b"x").release_id.clone(),
        previous_release_id: Some(component(b"x").release_id),
    };
    assert!(cycle.validate().is_err());
    Ok(())
}

#[test]
fn component_retention_waits_for_pointer_activation_and_catches_up() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let native_stage = PathBuf::from("C:/stage-native");
    stage_release(&fs, &native_stage, b"native");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: native_stage,
        manifest: release("1.0.0", b"native"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;

    let source_one = PathBuf::from("C:/adapter-one");
    let source_two = PathBuf::from("C:/adapter-two");
    let source_three = PathBuf::from("C:/adapter-three");
    let one = stage_component(&fs, &source_one, b"adapter-one");
    let two = stage_component(&fs, &source_two, b"adapter-two");
    let three = stage_component(&fs, &source_three, b"adapter-three");
    installer.install_omp_adapter(&source_one)?;
    installer.install_omp_adapter(&source_two)?;

    let historical = component(b"historical");
    let historical_root = layout.omp_adapter_version(&historical.release_id);
    stage_component(&fs, &historical_root, b"historical");
    *fs.fail_atomic_once.borrow_mut() = Some(layout.omp_adapter_current());
    assert!(installer.install_omp_adapter(&source_three).is_err());
    assert!(fs.exists(&layout.omp_adapter_version(&one.release_id)));
    assert!(fs.exists(&layout.omp_adapter_version(&two.release_id)));
    assert!(fs.exists(&layout.omp_adapter_version(&three.release_id)));
    assert!(fs.exists(&historical_root));
    let unchanged: ComponentPointer =
        serde_json::from_slice(&fs.read(&layout.omp_adapter_current())?)?;
    assert_eq!(unchanged.release_id, two.release_id);
    assert_eq!(unchanged.previous_release_id, Some(one.release_id.clone()));

    let active = installer.install_omp_adapter(&source_three)?;
    assert_eq!(active.release_id, three.release_id);
    assert_eq!(active.previous_release_id, Some(two.release_id.clone()));
    assert!(fs.exists(&layout.omp_adapter_version(&three.release_id)));
    assert!(fs.exists(&layout.omp_adapter_version(&two.release_id)));
    assert!(!fs.exists(&layout.omp_adapter_version(&one.release_id)));
    assert!(!fs.exists(&historical_root));
    Ok(())
}

#[test]
fn adapter_lifecycle_reads_only_validated_native_manifest_metadata() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let native_stage = PathBuf::from("C:/stage-native");
    let native = release("1.0.0", b"native");
    stage_release(&fs, &native_stage, b"native");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: native_stage,
        manifest: native.clone(),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;

    let native_root = layout.version("1.0.0");
    let native_manifest_path = native_root.join("release-manifest.json");
    for artifact in &native.artifacts {
        fs.files
            .borrow_mut()
            .insert(native_root.join(&artifact.path), b"tampered".to_vec());
    }
    fs.reads.borrow_mut().clear();
    let source = PathBuf::from("C:/adapter-one");
    stage_component(&fs, &source, b"adapter-one");

    fs.files
        .borrow_mut()
        .insert(native_manifest_path.clone(), b"{".to_vec());
    let malformed = installer
        .install_omp_adapter(&source)
        .expect_err("malformed native metadata must reject adapter activation");
    assert!(format!("{malformed:#}").contains("parse native release manifest"));

    let mut incompatible = native.clone();
    incompatible.compatibility.host_api = 2;
    fs.files.borrow_mut().insert(
        native_manifest_path.clone(),
        serde_json::to_vec_pretty(&incompatible)?,
    );
    let incompatible_error = installer
        .install_omp_adapter(&source)
        .expect_err("unsupported native compatibility must reject adapter activation");
    assert!(
        format!("{incompatible_error:#}").contains("release compatibility metadata is unsupported")
    );

    fs.files.borrow_mut().insert(
        native_manifest_path.clone(),
        serde_json::to_vec_pretty(&native)?,
    );
    let activated = installer.install_omp_adapter(&source)?;
    fs.files
        .borrow_mut()
        .insert(native_manifest_path.clone(), b"{".to_vec());
    let malformed_rollback = installer
        .rollback_omp_adapter(None)
        .expect_err("malformed native metadata must reject adapter rollback");
    assert!(format!("{malformed_rollback:#}").contains("parse native release manifest"));
    fs.files.borrow_mut().insert(
        native_manifest_path.clone(),
        serde_json::to_vec_pretty(&incompatible)?,
    );
    let incompatible_rollback = installer
        .rollback_omp_adapter(None)
        .expect_err("unsupported native compatibility must reject adapter rollback");
    assert!(
        format!("{incompatible_rollback:#}")
            .contains("release compatibility metadata is unsupported")
    );
    fs.files.borrow_mut().insert(
        native_manifest_path.clone(),
        serde_json::to_vec_pretty(&native)?,
    );
    let rolled_back = installer.rollback_omp_adapter(None)?;
    assert_eq!(
        rolled_back.previous_release_id.as_deref(),
        Some(activated.release_id.as_str())
    );

    let reads = fs.reads.borrow();
    let native_reads = reads
        .iter()
        .filter(|path| path.starts_with(&native_root))
        .collect::<Vec<_>>();
    assert!(native_reads.len() >= 6);
    assert!(
        native_reads
            .iter()
            .all(|path| path.as_path() == native_manifest_path)
    );
    Ok(())
}

#[test]
fn adapter_install_and_rollback_use_independent_real_releases() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let layout = InstallLayout::new(
        &temporary.path().join("Program Files"),
        &temporary.path().join("ProgramData"),
    );
    let native = release("1.0.0", b"manager");
    std::fs::create_dir_all(layout.version("1.0.0"))?;
    write_native_root(&layout.version("1.0.0"), &native, b"manager")?;
    std::fs::create_dir_all(&layout.program)?;
    std::fs::write(
        layout.current(),
        serde_json::to_vec_pretty(&CurrentRelease {
            version: "1.0.0".into(),
            previous_version: None,
            rollback_backup: None,
        })?,
    )?;

    let source_one = temporary.path().join("adapter-one");
    let one = component(b"export default 1");
    write_component_root(&source_one, &one, b"export default 1")?;
    let source_two = temporary.path().join("adapter-two");
    let two = component(b"export default 2");
    write_component_root(&source_two, &two, b"export default 2")?;

    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let installer = Installer {
        fs: &NativeFileSystem,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    assert_eq!(
        installer.install_omp_adapter(&source_one)?.release_id,
        one.release_id
    );
    let active = installer.install_omp_adapter(&source_two)?;
    assert_eq!(active.release_id, two.release_id);
    assert_eq!(active.previous_release_id, Some(one.release_id.clone()));
    assert!(layout.omp_adapter_version(&one.release_id).exists());
    assert!(layout.omp_adapter_version(&two.release_id).exists());
    let healthy = doctor(&NativeFileSystem, &services, &layout)?;
    assert!(
        healthy
            .checks
            .iter()
            .any(|check| { check.name == "omp-adapter-integrity" && check.ok })
    );
    assert!(
        healthy
            .checks
            .iter()
            .any(|check| { check.name == "omp-adapter-compatibility" && check.ok })
    );

    let rolled_back = installer.rollback_omp_adapter(None)?;
    assert_eq!(rolled_back.release_id, one.release_id);
    assert_eq!(
        rolled_back.previous_release_id,
        Some(two.release_id.clone())
    );

    let mut incompatible = component(b"incompatible");
    incompatible.compatibility.schema_version = 17;
    incompatible.release_id = incompatible.computed_release_id();
    write_component_root(
        &layout.omp_adapter_version(&incompatible.release_id),
        &incompatible,
        b"incompatible",
    )?;
    assert!(
        installer
            .rollback_omp_adapter(Some("../../escape"))
            .is_err()
    );
    assert!(
        installer
            .rollback_omp_adapter(Some(&incompatible.release_id))
            .is_err()
    );
    let missing = component(b"missing").release_id;
    assert!(installer.rollback_omp_adapter(Some(&missing)).is_err());

    let pointer: ComponentPointer =
        serde_json::from_slice(&std::fs::read(layout.omp_adapter_current())?)?;
    assert_eq!(pointer.release_id, rolled_back.release_id);
    std::fs::write(
        layout
            .omp_adapter_version(&pointer.release_id)
            .join("index.ts"),
        b"tampered",
    )?;
    let damaged = doctor(&NativeFileSystem, &services, &layout)?;
    assert!(
        damaged
            .checks
            .iter()
            .any(|check| { check.name == "omp-adapter-integrity" && !check.ok })
    );
    Ok(())
}

#[test]
fn native_activation_restores_both_pointers_after_component_write_failure() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let one = PathBuf::from("C:/stage-one");
    let two = PathBuf::from("C:/stage-two");
    stage_release(&fs, &one, b"one");
    stage_release(&fs, &two, b"two");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: one,
        manifest: release("1.0.0", b"one"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;

    let mut incompatible = component(b"old adapter");
    incompatible.compatibility.schema_version = 17;
    incompatible.release_id = incompatible.computed_release_id();
    let incompatible_root = layout.omp_adapter_version(&incompatible.release_id);
    fs.files
        .borrow_mut()
        .insert(incompatible_root.join("index.ts"), b"old adapter".to_vec());
    fs.files.borrow_mut().insert(
        incompatible_root.join("component-manifest.json"),
        serde_json::to_vec_pretty(&incompatible)?,
    );
    fs.dirs.borrow_mut().insert(incompatible_root);
    let prior_component = ComponentPointer {
        format: 1,
        release_id: incompatible.release_id,
        previous_release_id: None,
    };
    fs.write_atomic(
        &layout.omp_adapter_current(),
        &serde_json::to_vec_pretty(&prior_component)?,
    )?;
    *fs.fail_atomic_once.borrow_mut() = Some(layout.omp_adapter_current());

    assert!(
        installer
            .install(InstallRequest {
                staging: two,
                manifest: release("2.0.0", b"two"),
                external_database_url: None,
                house_config: None,
                operator_integration: None,
            })
            .is_err()
    );
    let native: CurrentRelease = serde_json::from_slice(&fs.read(&layout.current())?)?;
    let adapter: ComponentPointer =
        serde_json::from_slice(&fs.read(&layout.omp_adapter_current())?)?;
    assert_eq!(native.version, "1.0.0");
    assert_eq!(adapter, prior_component);
    Ok(())
}

#[test]
fn native_rollback_readiness_failure_restores_native_and_component_state() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let one = PathBuf::from("C:/stage-one");
    let two = PathBuf::from("C:/stage-two");
    stage_release(&fs, &one, b"one");
    stage_release(&fs, &two, b"two");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: one,
        manifest: release("1.0.0", b"one"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    installer.install(InstallRequest {
        staging: two,
        manifest: release("2.0.0", b"two"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    let component_before = fs.read(&layout.omp_adapter_current())?;
    *runtime.fail_ready_once.borrow_mut() = true;

    assert!(installer.rollback().is_err());
    let native: CurrentRelease = serde_json::from_slice(&fs.read(&layout.current())?)?;
    assert_eq!(native.version, "2.0.0");
    assert_eq!(fs.read(&layout.omp_adapter_current())?, component_before);
    Ok(())
}

#[test]
fn version_grammar_requires_an_ascii_alphanumeric_first_byte() {
    for accepted in ["1", "1.2.3", "v1.2.3-rc.1+build"] {
        assert!(safe_version(accepted), "{accepted:?} should be accepted");
    }
    for rejected in ["", ".1.2.3", "-1.2.3", "+1.2.3", "1..2", "1_2", "é1"] {
        assert!(!safe_version(rejected), "{rejected:?} should be rejected");
    }
    assert!(!safe_version(&"a".repeat(129)));
}

#[test]
fn manager_operation_lock_serializes_concurrent_threads() -> Result<()> {
    let first = OperationLock::acquire()?;
    let (started_tx, started_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let waiter = thread::spawn(move || -> Result<()> {
        let started = Instant::now();
        started_tx.send(())?;
        let _second = OperationLock::acquire()?;
        acquired_tx.send(started.elapsed())?;
        Ok(())
    });
    started_rx.recv()?;
    thread::sleep(Duration::from_millis(75));
    assert!(acquired_rx.try_recv().is_err());
    drop(first);
    assert!(acquired_rx.recv()?.as_millis() >= 50);
    waiter.join().expect("operation-lock waiter panicked")?;
    Ok(())
}

#[test]
fn concurrent_adapter_writers_preserve_both_serialized_releases() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let layout = InstallLayout::new(
        &temporary.path().join("Program Files"),
        &temporary.path().join("ProgramData"),
    );
    let native = release("1.0.0", b"manager");
    std::fs::create_dir_all(layout.version("1.0.0"))?;
    write_native_root(&layout.version("1.0.0"), &native, b"manager")?;
    std::fs::create_dir_all(&layout.program)?;
    std::fs::write(
        layout.current(),
        serde_json::to_vec_pretty(&CurrentRelease {
            version: "1.0.0".into(),
            previous_version: None,
            rollback_backup: None,
        })?,
    )?;
    let source_one = temporary.path().join("concurrent-one");
    let one = component(b"concurrent one");
    write_component_root(&source_one, &one, b"concurrent one")?;
    let source_two = temporary.path().join("concurrent-two");
    let two = component(b"concurrent two");
    write_component_root(&source_two, &two, b"concurrent two")?;
    let barrier = Arc::new(Barrier::new(3));

    let writers = [source_one, source_two]
        .into_iter()
        .map(|source| {
            let layout = layout.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || -> Result<ComponentPointer> {
                let services = FakeServices::default();
                let runtime = FakeRuntime::default();
                let installer = Installer {
                    fs: &NativeFileSystem,
                    services: &services,
                    runtime: &runtime,
                    secrets: &FixedSecrets,
                    layout,
                };
                barrier.wait();
                installer.install_omp_adapter(&source)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for writer in writers {
        writer.join().expect("adapter writer panicked")?;
    }

    let pointer: ComponentPointer =
        serde_json::from_slice(&std::fs::read(layout.omp_adapter_current())?)?;
    let active_and_previous = [
        pointer.release_id.clone(),
        pointer
            .previous_release_id
            .clone()
            .context("serialized second writer must retain the first release")?,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let expected = [one.release_id.clone(), two.release_id.clone()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(active_and_previous, expected);
    assert!(layout.omp_adapter_version(&one.release_id).is_dir());
    assert!(layout.omp_adapter_version(&two.release_id).is_dir());
    Ok(())
}

#[test]
fn operation_lock_child_process() {
    if std::env::var_os("ATHANOR_OPERATION_LOCK_CRASH_CHILD").is_some() {
        let _operation = OperationLock::acquire().expect("child acquires operation lock");
        std::process::exit(91);
    }
}

#[cfg(windows)]
#[test]
fn crashed_process_releases_windows_operation_mutex() -> Result<()> {
    use std::process::{Command, Stdio};

    let status = Command::new(std::env::current_exe()?)
        .args(["--exact", "operation_lock_child_process", "--nocapture"])
        .env("ATHANOR_OPERATION_LOCK_CRASH_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    assert_eq!(status.code(), Some(91));
    let _operation = OperationLock::acquire()?;
    Ok(())
}

#[test]
fn post_activation_operator_failure_restores_every_external_file() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let one = PathBuf::from("C:/stage-one");
    let two = PathBuf::from("C:/stage-two");
    stage_release(&fs, &one, b"manager-one");
    stage_release(&fs, &two, b"manager-two");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: one,
        manifest: release("1.0.0", b"manager-one"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;

    fs.files
        .borrow_mut()
        .insert(layout.omp_loader(), b"stable-loader-one".to_vec());
    let omp_config = PathBuf::from("C:/Users/Sol/.omp/agent/config.yml");
    let client_config = PathBuf::from("C:/Users/Sol/.omp/agent/athanor/client.json");
    fs.files
        .borrow_mut()
        .insert(omp_config.clone(), b"theme: old\n".to_vec());
    fs.files
        .borrow_mut()
        .insert(client_config.clone(), b"{\"prior\":true}".to_vec());
    let protected = [
        layout.current(),
        layout.omp_adapter_current(),
        layout.config(),
        layout.secrets(),
        layout.manager(),
        layout.omp_loader(),
        omp_config.clone(),
        client_config.clone(),
    ]
    .into_iter()
    .map(|path| Ok((path.clone(), fs.read(&path)?)))
    .collect::<Result<Vec<_>>>()?;

    *fs.fail_atomic_once.borrow_mut() = Some(omp_config.clone());
    let failure = installer
        .install(InstallRequest {
            staging: two,
            manifest: release("2.0.0", b"manager-two"),
            external_database_url: Some("postgresql://new-authority".into()),
            house_config: None,
            operator_integration: Some(OperatorIntegration {
                omp_config_path: omp_config,
                client_config_path: client_config,
                operator_principal: "SOL\\Sol".into(),
            }),
        })
        .expect_err("operator integration failure must fail the install");
    assert!(
        failure
            .to_string()
            .contains("prior installation was restored")
    );
    for (path, bytes) in protected {
        assert_eq!(
            fs.read(&path)?,
            bytes,
            "{} was not restored",
            path.display()
        );
    }
    assert!(
        runtime
            .events
            .borrow()
            .iter()
            .any(|event| event == "restore")
    );
    assert_eq!(
        runtime.events.borrow().last().map(String::as_str),
        Some("ready")
    );
    assert!(!fs.exists(&layout.version("2.0.0")));
    Ok(())
}

#[test]
fn restoration_continues_after_database_restore_failure() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let one = PathBuf::from("C:/stage-one");
    let two = PathBuf::from("C:/stage-two");
    stage_release(&fs, &one, b"manager-one");
    stage_release(&fs, &two, b"manager-two");
    let installer = Installer {
        fs: &fs,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    installer.install(InstallRequest {
        staging: one,
        manifest: release("1.0.0", b"manager-one"),
        external_database_url: None,
        house_config: None,
        operator_integration: None,
    })?;
    let native_before = fs.read(&layout.current())?;
    let component_before = fs.read(&layout.omp_adapter_current())?;
    let manager_before = fs.read(&layout.manager())?;
    *runtime.fail_ready_once.borrow_mut() = true;
    *runtime.fail_restore_once.borrow_mut() = true;

    let failure = installer
        .install(InstallRequest {
            staging: two,
            manifest: release("2.0.0", b"manager-two"),
            external_database_url: None,
            house_config: None,
            operator_integration: None,
        })
        .expect_err("readiness failure must fail the install");
    assert!(failure.to_string().contains("database restoration failure"));
    assert_eq!(fs.read(&layout.current())?, native_before);
    assert_eq!(fs.read(&layout.omp_adapter_current())?, component_before);
    assert_eq!(fs.read(&layout.manager())?, manager_before);
    assert!(!fs.exists(&layout.version("2.0.0")));
    assert_eq!(
        runtime.events.borrow().last().map(String::as_str),
        Some("ready")
    );
    assert_eq!(
        services.events.borrow().last().unwrap(),
        &format!("start:{SERVICE_NAME}")
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn native_filesystem_rejects_reparse_artifacts_and_owned_ancestors() -> Result<()> {
    use std::io::ErrorKind;
    use std::os::windows::fs::{symlink_dir, symlink_file};

    let temporary = tempfile::tempdir()?;
    let outside = temporary.path().join("outside");
    std::fs::create_dir_all(&outside)?;
    std::fs::write(outside.join("index.ts"), b"foreign")?;

    let component_root = temporary.path().join("component");
    std::fs::create_dir_all(&component_root)?;
    let linked = component(b"foreign");
    std::fs::write(
        component_root.join("component-manifest.json"),
        serde_json::to_vec_pretty(&linked)?,
    )?;
    if let Err(error) = symlink_file(outside.join("index.ts"), component_root.join("index.ts")) {
        if error.kind() == ErrorKind::PermissionDenied || error.raw_os_error() == Some(1314) {
            return Ok(());
        }
        return Err(error.into());
    }
    assert!(read_verified_component(&NativeFileSystem, &component_root).is_err());

    let nested_root = temporary.path().join("nested-component");
    std::fs::create_dir_all(&nested_root)?;
    let mut nested = component(b"foreign");
    nested.artifacts[0].path = "payload/index.ts".into();
    nested.release_id = nested.computed_release_id();
    std::fs::write(
        nested_root.join("component-manifest.json"),
        serde_json::to_vec_pretty(&nested)?,
    )?;
    if let Err(error) = symlink_dir(&outside, nested_root.join("payload")) {
        if error.kind() == ErrorKind::PermissionDenied || error.raw_os_error() == Some(1314) {
            return Ok(());
        }
        return Err(error.into());
    }
    assert!(read_verified_component(&NativeFileSystem, &nested_root).is_err());

    let layout = InstallLayout::new(
        &temporary.path().join("Program Files"),
        &temporary.path().join("ProgramData"),
    );
    let native = release("1.0.0", b"manager");
    std::fs::create_dir_all(layout.version("1.0.0"))?;
    write_native_root(&layout.version("1.0.0"), &native, b"manager")?;
    std::fs::create_dir_all(&layout.program)?;
    std::fs::write(
        layout.current(),
        serde_json::to_vec_pretty(&CurrentRelease {
            version: "1.0.0".into(),
            previous_version: None,
            rollback_backup: None,
        })?,
    )?;
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let installer = Installer {
        fs: &NativeFileSystem,
        services: &services,
        runtime: &runtime,
        secrets: &FixedSecrets,
        layout: layout.clone(),
    };
    assert!(installer.install_omp_adapter(&component_root).is_err());
    assert!(!layout.omp_adapter_version(&linked.release_id).exists());

    let staging = temporary.path().join("native-stage");
    std::fs::create_dir_all(staging.join("bin"))?;
    let native_manager = temporary.path().join("foreign-manager.exe");
    std::fs::write(&native_manager, b"manager-two")?;
    symlink_file(&native_manager, staging.join("bin/athanor-manage.exe"))?;
    let native_two = release("2.0.0", b"manager-two");
    let adapter = b"adapter";
    let adapter_manifest = serde_json::to_vec_pretty(&component(adapter))?;
    std::fs::create_dir_all(staging.join("components/omp-adapter"))?;
    std::fs::write(
        staging.join("components/omp-adapter/component-manifest.json"),
        adapter_manifest,
    )?;
    std::fs::write(staging.join("components/omp-adapter/index.ts"), adapter)?;
    assert!(
        installer
            .install(InstallRequest {
                staging,
                manifest: native_two,
                external_database_url: None,
                house_config: None,
                operator_integration: None,
            })
            .is_err()
    );
    assert!(!layout.version("2.0.0").exists());
    Ok(())
}
