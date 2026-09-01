use crate::layout::safe_version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};
use thiserror::Error;

pub const MANIFEST_FORMAT: u32 = 1;
/// Release manifests and `origami` share the protocol-owned migration head.
pub const REQUIRED_SCHEMA: u32 = protocol::SUBSTRATE_SCHEMA_VERSION;
pub const SUPPORTED_PLATFORM: &str = "windows-x64";
pub const POSTGRESQL_VERSION: &str = "18.4-2";
pub const PGVECTOR_VERSION: &str = "0.8.6";
pub const NATS_VERSION: &str = "2.14.4";
pub const GODOT_VERSION: &str = "4.7.1-stable";

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
    pub format: u32,
    pub product: String,
    pub version: String,
    pub platform: String,
    pub schema_version: u32,
    pub compatibility: Compatibility,
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub rollback: RollbackContract,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    pub host_api: u32,
    pub substrate_api: u32,
    pub delivery_api: u32,
    pub godot_api: String,
    pub godot: String,
    pub postgresql: String,
    pub pgvector: String,
    pub nats_server: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub component: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub executable: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RollbackContract {
    pub database_restore_required: bool,
    pub minimum_retained_versions: u8,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ManifestError {
    #[error("unsupported manifest format {0}")]
    Format(u32),
    #[error("unsupported product {0:?}")]
    Product(String),
    #[error("invalid release version {0:?}")]
    Version(String),
    #[error("unsupported platform {0:?}")]
    Platform(String),
    #[error("schema {0} is incompatible; this installer requires {REQUIRED_SCHEMA}")]
    Schema(u32),
    #[error("release compatibility metadata is unsupported: {0}")]
    Compatibility(String),
    #[error(
        "release rollback contract must require database restore and retain at least two versions"
    )]
    RollbackContract,
    #[error("manifest contains no artifacts")]
    Empty,
    #[error("unsafe artifact path {0:?}")]
    UnsafePath(String),
    #[error("invalid SHA-256 for {0:?}")]
    Digest(String),
    #[error("duplicate artifact path {0:?}")]
    Duplicate(String),
    #[error("artifact size mismatch for {path:?}: expected {expected}, got {actual}")]
    Size {
        path: String,
        expected: u64,
        actual: u64,
    },
    #[error("artifact checksum mismatch for {0:?}")]
    Checksum(String),
}

impl ReleaseManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.format != MANIFEST_FORMAT {
            return Err(ManifestError::Format(self.format));
        }
        if self.product != "the-athanor" {
            return Err(ManifestError::Product(self.product.clone()));
        }
        if !safe_version(&self.version) {
            return Err(ManifestError::Version(self.version.clone()));
        }
        if self.platform != SUPPORTED_PLATFORM {
            return Err(ManifestError::Platform(self.platform.clone()));
        }
        if self.schema_version != REQUIRED_SCHEMA {
            return Err(ManifestError::Schema(self.schema_version));
        }
        let compatibility_ok = self.compatibility.host_api == 1
            && self.compatibility.substrate_api == 1
            && self.compatibility.delivery_api == 1
            && self.compatibility.godot_api == "4.7"
            && self.compatibility.godot == GODOT_VERSION
            && self.compatibility.postgresql == POSTGRESQL_VERSION
            && self.compatibility.pgvector == PGVECTOR_VERSION
            && self.compatibility.nats_server == NATS_VERSION;
        if !compatibility_ok {
            return Err(ManifestError::Compatibility(format!(
                "hostApi={}, substrateApi={}, deliveryApi={}, godotApi={}, Godot={}, PostgreSQL={}, pgvector={}, NATS={}",
                self.compatibility.host_api,
                self.compatibility.substrate_api,
                self.compatibility.delivery_api,
                self.compatibility.godot_api,
                self.compatibility.godot,
                self.compatibility.postgresql,
                self.compatibility.pgvector,
                self.compatibility.nats_server,
            )));
        }
        if !self.rollback.database_restore_required || self.rollback.minimum_retained_versions < 2 {
            return Err(ManifestError::RollbackContract);
        }
        if self.artifacts.is_empty() {
            return Err(ManifestError::Empty);
        }
        let mut paths = std::collections::BTreeSet::new();
        for artifact in &self.artifacts {
            let path = Path::new(&artifact.path);
            let safe = !path.is_absolute()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                && !artifact.path.contains('\\');
            if !safe {
                return Err(ManifestError::UnsafePath(artifact.path.clone()));
            }
            if artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ManifestError::Digest(artifact.path.clone()));
            }
            if !paths.insert(artifact.path.to_ascii_lowercase()) {
                return Err(ManifestError::Duplicate(artifact.path.clone()));
            }
        }
        Ok(())
    }

    pub fn verify_bytes(&self, artifact: &Artifact, bytes: &[u8]) -> Result<(), ManifestError> {
        let actual = bytes.len() as u64;
        if actual != artifact.size {
            return Err(ManifestError::Size {
                path: artifact.path.clone(),
                expected: artifact.size,
                actual,
            });
        }
        let digest = hex::encode(Sha256::digest(bytes));
        if !digest.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(ManifestError::Checksum(artifact.path.clone()));
        }
        Ok(())
    }
}
