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
    installer::{CurrentRelease, InstallRequest, Installer},
    layout::InstallLayout,
    manifest::{Artifact, Compatibility, REQUIRED_SCHEMA, ReleaseManifest, RollbackContract},
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};

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
            schema_version: REQUIRED_SCHEMA,
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
        schema_version: REQUIRED_SCHEMA,
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
                component: "app".into(),
                path: "bin/athanor.exe".into(),
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
        } else if artifact.path.ends_with("athanor.exe") {
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
    incompatible.compatibility.schema_version = REQUIRED_SCHEMA - 1;
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
