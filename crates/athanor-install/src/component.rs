use crate::{boundaries::FileSystem, layout::safe_version, manifest::ReleaseManifest};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

pub const COMPONENT_MANIFEST: &str = "component-manifest.json";
pub const OMP_ADAPTER_COMPONENT: &str = "omp-adapter";
pub const COMPONENT_FORMAT: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentManifest {
    pub format: u32,
    pub component: String,
    pub version: String,
    pub release_id: String,
    pub compatibility: ComponentCompatibility,
    pub artifacts: Vec<ComponentArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentCompatibility {
    pub host_api: u32,
    pub substrate_api: u32,
    pub delivery_api: u32,
    pub schema_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentArtifact {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentPointer {
    pub format: u32,
    pub release_id: String,
    pub previous_release_id: Option<String>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ComponentManifestError {
    #[error("unsupported component manifest format {0}")]
    Format(u32),
    #[error("unsupported component {0:?}")]
    Component(String),
    #[error("invalid component version {0:?}")]
    Version(String),
    #[error("component manifest contains no artifacts")]
    Empty,
    #[error("unsafe component artifact path {0:?}")]
    UnsafePath(String),
    #[error("invalid lowercase SHA-256 for component artifact {0:?}")]
    Digest(String),
    #[error("duplicate component artifact path {0:?}")]
    Duplicate(String),
    #[error("component artifacts are not sorted by normalized ordinal path")]
    Unsorted,
    #[error("component release identity mismatch: expected {expected:?}, got {actual:?}")]
    Identity { expected: String, actual: String },
    #[error("component artifact size mismatch for {path:?}: expected {expected}, got {actual}")]
    Size {
        path: String,
        expected: u64,
        actual: u64,
    },
    #[error("component artifact checksum mismatch for {0:?}")]
    Checksum(String),
    #[error("unsupported component pointer format {0}")]
    PointerFormat(u32),
    #[error("invalid component pointer release id {0:?}")]
    PointerRelease(String),
    #[error("component pointer cannot name its active release as previous")]
    PointerCycle,
}

impl ComponentCompatibility {
    pub fn matches_native(&self, native: &ReleaseManifest) -> bool {
        self.host_api == native.compatibility.host_api
            && self.substrate_api == native.compatibility.substrate_api
            && self.delivery_api == native.compatibility.delivery_api
            && self.schema_version == native.schema_version
    }

    pub fn describe(&self) -> String {
        format!(
            "hostApi={}, substrateApi={}, deliveryApi={}, schemaVersion={}",
            self.host_api, self.substrate_api, self.delivery_api, self.schema_version
        )
    }
}

impl ComponentManifest {
    pub fn validate(&self) -> std::result::Result<(), ComponentManifestError> {
        if self.format != COMPONENT_FORMAT {
            return Err(ComponentManifestError::Format(self.format));
        }
        if self.component != OMP_ADAPTER_COMPONENT {
            return Err(ComponentManifestError::Component(self.component.clone()));
        }
        if !safe_version(&self.version) {
            return Err(ComponentManifestError::Version(self.version.clone()));
        }
        if self.artifacts.is_empty() {
            return Err(ComponentManifestError::Empty);
        }

        let mut paths = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for artifact in &self.artifacts {
            if !safe_artifact_path(&artifact.path) {
                return Err(ComponentManifestError::UnsafePath(artifact.path.clone()));
            }
            if artifact.sha256.len() != 64
                || !artifact
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ComponentManifestError::Digest(artifact.path.clone()));
            }
            if previous.is_some_and(|value| value >= artifact.path.as_str()) {
                return Err(ComponentManifestError::Unsorted);
            }
            previous = Some(&artifact.path);
            if !paths.insert(artifact.path.to_ascii_lowercase()) {
                return Err(ComponentManifestError::Duplicate(artifact.path.clone()));
            }
        }

        let expected = self.computed_release_id();
        if self.release_id != expected {
            return Err(ComponentManifestError::Identity {
                expected,
                actual: self.release_id.clone(),
            });
        }
        Ok(())
    }

    pub fn canonical_identity(&self) -> Vec<u8> {
        let mut identity = format!(
            "format={}\ncomponent={}\nversion={}\nhostApi={}\nsubstrateApi={}\ndeliveryApi={}\nschemaVersion={}\n",
            self.format,
            self.component,
            self.version,
            self.compatibility.host_api,
            self.compatibility.substrate_api,
            self.compatibility.delivery_api,
            self.compatibility.schema_version,
        );
        for artifact in &self.artifacts {
            identity.push_str("artifact=");
            identity.push_str(&artifact.path);
            identity.push('\t');
            identity.push_str(&artifact.sha256);
            identity.push('\t');
            identity.push_str(&artifact.size.to_string());
            identity.push('\n');
        }
        identity.into_bytes()
    }

    pub fn computed_release_id(&self) -> String {
        format!(
            "{}-{}",
            self.version,
            hex::encode(Sha256::digest(self.canonical_identity()))
        )
    }

    pub fn verify_bytes(
        &self,
        artifact: &ComponentArtifact,
        bytes: &[u8],
    ) -> std::result::Result<(), ComponentManifestError> {
        let actual = bytes.len() as u64;
        if actual != artifact.size {
            return Err(ComponentManifestError::Size {
                path: artifact.path.clone(),
                expected: artifact.size,
                actual,
            });
        }
        if hex::encode(Sha256::digest(bytes)) != artifact.sha256 {
            return Err(ComponentManifestError::Checksum(artifact.path.clone()));
        }
        Ok(())
    }
}

impl ComponentPointer {
    pub fn validate(&self) -> std::result::Result<(), ComponentManifestError> {
        if self.format != COMPONENT_FORMAT {
            return Err(ComponentManifestError::PointerFormat(self.format));
        }
        if !valid_release_id(&self.release_id) {
            return Err(ComponentManifestError::PointerRelease(
                self.release_id.clone(),
            ));
        }
        if let Some(previous) = &self.previous_release_id {
            if !valid_release_id(previous) {
                return Err(ComponentManifestError::PointerRelease(previous.clone()));
            }
            if previous == &self.release_id {
                return Err(ComponentManifestError::PointerCycle);
            }
        }
        Ok(())
    }
}

pub fn read_verified_component<F: FileSystem>(
    fs: &F,
    root: &std::path::Path,
) -> Result<ComponentManifest> {
    let manifest_path = root.join(COMPONENT_MANIFEST);
    fs.validate_regular_file(root, &manifest_path)?;
    let manifest: ComponentManifest = serde_json::from_slice(
        &fs.read(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse {}", manifest_path.display()))?;
    manifest.validate()?;
    for artifact in &manifest.artifacts {
        let path = root.join(&artifact.path);
        fs.validate_regular_file(root, &path)?;
        let bytes = fs
            .read(&path)
            .with_context(|| format!("read component artifact {}", path.display()))?;
        manifest.verify_bytes(artifact, &bytes)?;
    }
    Ok(manifest)
}

pub(crate) fn valid_release_id(value: &str) -> bool {
    value.rsplit_once('-').is_some_and(|(version, digest)| {
        safe_version(version)
            && digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn safe_artifact_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}
