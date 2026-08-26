pub mod app;
pub mod boundaries;
pub mod component;
pub mod harness;
pub mod installer;
pub mod layout;
pub mod manifest;
pub mod native_runtime;
pub mod omp;
pub mod service;
pub mod supervisor;

use anyhow::Result;
use boundaries::{FileSystem, OperationLock, ServiceManager};
use component::{ComponentPointer, read_verified_component};
use installer::CurrentRelease;
use layout::{InstallLayout, SERVICE_NAME, safe_version};
use manifest::ReleaseManifest;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub ok: bool,
    pub installed_version: Option<String>,
    pub service_installed: bool,
    pub data_present: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

pub fn doctor<F: FileSystem, S: ServiceManager>(
    fs: &F,
    services: &S,
    layout: &InstallLayout,
) -> Result<DoctorReport> {
    let _operation = OperationLock::acquire()?;
    let mut checks = Vec::new();
    let mut native_manifest = None;
    let current: Option<CurrentRelease> = if fs.exists(&layout.current()) {
        fs.validate_regular_file(&layout.program, &layout.current())?;
        match serde_json::from_slice::<CurrentRelease>(&fs.read(&layout.current())?) {
            Ok(value)
                if safe_version(&value.version)
                    && value.previous_version.as_ref().is_none_or(|previous| {
                        safe_version(previous) && previous != &value.version
                    }) =>
            {
                checks.push(DoctorCheck {
                    name: "current-pointer".into(),
                    ok: true,
                    detail: value.version.clone(),
                });
                Some(value)
            }
            Ok(_) => {
                checks.push(DoctorCheck {
                    name: "current-pointer".into(),
                    ok: false,
                    detail: "invalid native release version lineage".into(),
                });
                None
            }
            Err(error) => {
                checks.push(DoctorCheck {
                    name: "current-pointer".into(),
                    ok: false,
                    detail: error.to_string(),
                });
                None
            }
        }
    } else {
        checks.push(DoctorCheck {
            name: "current-pointer".into(),
            ok: false,
            detail: "not installed".into(),
        });
        None
    };
    if let Some(current) = &current {
        let version_root = layout.version(&current.version);
        let manifest_path = version_root.join("release-manifest.json");
        fs.validate_regular_file(&version_root, &manifest_path)?;
        match fs
            .read(&manifest_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ReleaseManifest>(&bytes).ok())
        {
            Some(manifest) => {
                let manifest_ok =
                    manifest.validate().is_ok() && manifest.version == current.version;
                checks.push(DoctorCheck {
                    name: "release-manifest".into(),
                    ok: manifest_ok,
                    detail: manifest_path.display().to_string(),
                });
                if manifest_ok {
                    native_manifest = Some(manifest.clone());
                    let mut failures = Vec::new();
                    for artifact in &manifest.artifacts {
                        let path = version_root.join(&artifact.path);
                        let physical = fs.validate_regular_file(&version_root, &path).is_ok();
                        let verified = physical
                            && fs.read(&path).ok().is_some_and(|bytes| {
                                manifest.verify_bytes(artifact, &bytes).is_ok()
                            });
                        if !verified {
                            failures.push(artifact.path.clone());
                        }
                    }
                    checks.push(DoctorCheck {
                        name: "artifact-checksums".into(),
                        ok: failures.is_empty(),
                        detail: if failures.is_empty() {
                            format!("{} artifacts verified", manifest.artifacts.len())
                        } else {
                            failures.join(", ")
                        },
                    });
                }
            }
            None => checks.push(DoctorCheck {
                name: "release-manifest".into(),
                ok: false,
                detail: manifest_path.display().to_string(),
            }),
        }
    }
    let component_pointer_path = layout.omp_adapter_current();
    let component_pointer = if fs.exists(&component_pointer_path) {
        fs.validate_regular_file(&layout.omp_adapter(), &component_pointer_path)?;
        match fs
            .read(&component_pointer_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ComponentPointer>(&bytes).ok())
        {
            Some(pointer) => match pointer.validate() {
                Ok(()) => {
                    checks.push(DoctorCheck {
                        name: "omp-adapter-pointer".into(),
                        ok: true,
                        detail: pointer.release_id.clone(),
                    });
                    Some(pointer)
                }
                Err(error) => {
                    checks.push(DoctorCheck {
                        name: "omp-adapter-pointer".into(),
                        ok: false,
                        detail: error.to_string(),
                    });
                    None
                }
            },
            None => {
                checks.push(DoctorCheck {
                    name: "omp-adapter-pointer".into(),
                    ok: false,
                    detail: format!("cannot parse {}", component_pointer_path.display()),
                });
                None
            }
        }
    } else {
        checks.push(DoctorCheck {
            name: "omp-adapter-pointer".into(),
            ok: false,
            detail: "not installed".into(),
        });
        None
    };
    if let Some(pointer) = component_pointer {
        let root = layout.omp_adapter_version(&pointer.release_id);
        match read_verified_component(fs, &root) {
            Ok(component) if component.release_id == pointer.release_id => {
                checks.push(DoctorCheck {
                    name: "omp-adapter-integrity".into(),
                    ok: true,
                    detail: format!(
                        "{} artifacts verified for {}",
                        component.artifacts.len(),
                        component.release_id
                    ),
                });
                let compatible = native_manifest
                    .as_ref()
                    .is_some_and(|native| component.compatibility.matches_native(native));
                checks.push(DoctorCheck {
                    name: "omp-adapter-compatibility".into(),
                    ok: compatible,
                    detail: component.compatibility.describe(),
                });
            }
            Ok(component) => checks.push(DoctorCheck {
                name: "omp-adapter-integrity".into(),
                ok: false,
                detail: format!(
                    "pointer names {}, manifest names {}",
                    pointer.release_id, component.release_id
                ),
            }),
            Err(error) => checks.push(DoctorCheck {
                name: "omp-adapter-integrity".into(),
                ok: false,
                detail: error.to_string(),
            }),
        }
        if let Some(previous) = &pointer.previous_release_id {
            let previous_root = layout.omp_adapter_version(previous);
            let previous_valid = read_verified_component(fs, &previous_root)
                .is_ok_and(|component| component.release_id.as_str() == previous.as_str());
            checks.push(DoctorCheck {
                name: "omp-adapter-previous-release".into(),
                ok: previous_valid,
                detail: previous.clone(),
            });
        }
    }
    let service_installed = services.is_installed(SERVICE_NAME)?;
    checks.push(DoctorCheck {
        name: "windows-service".into(),
        ok: service_installed,
        detail: SERVICE_NAME.into(),
    });
    let data_present = fs.exists(&layout.data);
    checks.push(DoctorCheck {
        name: "persistent-data".into(),
        ok: data_present,
        detail: layout.data.display().to_string(),
    });
    Ok(DoctorReport {
        ok: checks.iter().all(|check| check.ok),
        installed_version: current.map(|value| value.version),
        service_installed,
        data_present,
        checks,
    })
}
