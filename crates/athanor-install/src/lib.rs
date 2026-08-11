pub mod boundaries;
pub mod installer;
pub mod layout;
pub mod manifest;
pub mod native_runtime;
pub mod omp;
pub mod service;
pub mod supervisor;

use anyhow::Result;
use boundaries::{FileSystem, ServiceManager};
use installer::CurrentRelease;
use layout::{InstallLayout, SERVICE_NAME};
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
    let mut checks = Vec::new();
    let current: Option<CurrentRelease> = if fs.exists(&layout.current()) {
        match serde_json::from_slice(&fs.read(&layout.current())?) {
            Ok(value) => Some(value),
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
        match fs
            .read(&manifest_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ReleaseManifest>(&bytes).ok())
        {
            Some(manifest) => {
                let manifest_ok = manifest.validate().is_ok();
                checks.push(DoctorCheck {
                    name: "release-manifest".into(),
                    ok: manifest_ok,
                    detail: manifest_path.display().to_string(),
                });
                if manifest_ok {
                    let mut failures = Vec::new();
                    for artifact in &manifest.artifacts {
                        let verified = fs
                            .read(&version_root.join(&artifact.path))
                            .ok()
                            .is_some_and(|bytes| manifest.verify_bytes(artifact, &bytes).is_ok());
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
