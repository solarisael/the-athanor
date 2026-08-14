use anyhow::{Result, bail};
use athanor_install::{
    boundaries::{FileSystem, NativeFileSystem, RuntimeControl, SecretSource, ServiceManager},
    installer::{HouseInstallConfig, InstallRequest, Installer, OperatorIntegration},
    layout::{InstallLayout, SERVICE_NAME},
    manifest::{Artifact, Compatibility, ReleaseManifest, RollbackContract},
    supervisor::HostRoomConfig,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Default)]
struct FakeFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    dirs: RefCell<BTreeSet<PathBuf>>,
    acls: RefCell<Vec<PathBuf>>,
}
impl FileSystem for FakeFs {
    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path) || self.dirs.borrow().contains(path)
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
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
    fail_ready: RefCell<bool>,
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
        Ok(())
    }
    fn wait_ready(&self) -> Result<()> {
        self.events.borrow_mut().push("ready".into());
        if *self.fail_ready.borrow() {
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

fn release(version: &str, bytes: &[u8]) -> ReleaseManifest {
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
        artifacts: vec![Artifact {
            component: "installer".into(),
            path: "bin/athanor-manage.exe".into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            size: bytes.len() as u64,
            executable: true,
        }],
        rollback: RollbackContract {
            database_restore_required: true,
            minimum_retained_versions: 2,
        },
    }
}

#[test]
fn install_verifies_before_mutation_and_uninstall_preserves_data() -> Result<()> {
    let fs = FakeFs::default();
    let services = FakeServices::default();
    let runtime = FakeRuntime::default();
    let layout = InstallLayout::new(Path::new("C:/Program Files"), Path::new("C:/ProgramData"));
    let staging = PathBuf::from("C:/stage");
    let bytes = b"native-manager";
    fs.files
        .borrow_mut()
        .insert(staging.join("bin/athanor-manage.exe"), bytes.to_vec());
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
    fs.files
        .borrow_mut()
        .insert(staging.join("bin/athanor-manage.exe"), b"tampered".to_vec());
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
    fs.files
        .borrow_mut()
        .insert(one.join("bin/athanor-manage.exe"), b"one".to_vec());
    fs.files
        .borrow_mut()
        .insert(two.join("bin/athanor-manage.exe"), b"two".to_vec());
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
    fs.files.borrow_mut().insert(
        staging.join("bin/athanor-manage.exe"),
        b"native-manager".to_vec(),
    );
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
    fs.files.borrow_mut().insert(
        staging.join("bin/athanor-manage.exe"),
        b"native-manager".to_vec(),
    );
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
    fs.files.borrow_mut().insert(
        staging.join("bin/athanor-manage.exe"),
        b"native-manager".to_vec(),
    );
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
