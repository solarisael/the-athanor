use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const PRODUCT_DIR: &str = "Solarisael/Athanor";
pub const SERVICE_NAME: &str = "SolarisaelAthanor";
pub const SERVICE_DISPLAY_NAME: &str = "Solarisael Athanor";
pub const CURRENT_POINTER: &str = "current.json";
pub const RUNTIME_CONFIG: &str = "config/runtime.json";
pub const HARNESS_REGISTRY: &str = "config/harnesses.json";
pub const PROGRAM_ROOT_ENV: &str = "ATHANOR_PROGRAM_ROOT";
pub const DATA_ROOT_ENV: &str = "ATHANOR_DATA_ROOT";
pub const SECRETS_FILE: &str = "secrets/runtime-secrets.json";
pub const LEGACY_NAMES: &[&str] = &["solarisael-house", "athanor-omp", "athanor-substrate"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallLayout {
    pub program: PathBuf,
    pub data: PathBuf,
}

impl InstallLayout {
    pub fn new(program_files: &Path, program_data: &Path) -> Self {
        Self {
            program: program_files.join(PRODUCT_DIR),
            data: program_data.join(PRODUCT_DIR),
        }
    }

    pub fn from_environment() -> Result<Self> {
        match (
            std::env::var_os(PROGRAM_ROOT_ENV),
            std::env::var_os(DATA_ROOT_ENV),
        ) {
            (Some(program), Some(data)) if !program.is_empty() && !data.is_empty() => Ok(Self {
                program: PathBuf::from(program),
                data: PathBuf::from(data),
            }),
            (None, None) => {
                let program_files = PathBuf::from(
                    std::env::var_os("ProgramFiles").context("ProgramFiles is unavailable")?,
                );
                let program_data = PathBuf::from(
                    std::env::var_os("ProgramData").context("ProgramData is unavailable")?,
                );
                Ok(Self::new(&program_files, &program_data))
            }
            _ => anyhow::bail!("{PROGRAM_ROOT_ENV} and {DATA_ROOT_ENV} must be set together"),
        }
    }

    pub fn versions(&self) -> PathBuf {
        self.program.join("versions")
    }
    pub fn version(&self, version: &str) -> PathBuf {
        self.versions().join(version)
    }
    pub fn manager(&self) -> PathBuf {
        self.program.join("bin/athanor-manage.exe")
    }
    pub fn app(&self) -> PathBuf {
        self.program.join("bin/athanor.exe")
    }
    pub fn omp_loader(&self) -> PathBuf {
        self.program.join("bin/athanor-omp-loader.ts")
    }
    pub fn current(&self) -> PathBuf {
        self.program.join(CURRENT_POINTER)
    }
    pub fn omp_adapter(&self) -> PathBuf {
        self.program.join("components/omp-adapter")
    }
    pub fn omp_adapter_versions(&self) -> PathBuf {
        self.omp_adapter().join("versions")
    }
    pub fn omp_adapter_version(&self, release_id: &str) -> PathBuf {
        self.omp_adapter_versions().join(release_id)
    }
    pub fn omp_adapter_current(&self) -> PathBuf {
        self.omp_adapter().join(CURRENT_POINTER)
    }
    pub fn config(&self) -> PathBuf {
        self.data.join(RUNTIME_CONFIG)
    }
    pub fn harness_registry(&self) -> PathBuf {
        self.data.join(HARNESS_REGISTRY)
    }
    pub fn secrets(&self) -> PathBuf {
        self.data.join(SECRETS_FILE)
    }
    pub fn backups(&self) -> PathBuf {
        self.data.join("backups")
    }
    pub fn postgres_data(&self) -> PathBuf {
        self.data.join("data/postgresql")
    }
    pub fn nats_data(&self) -> PathBuf {
        self.data.join("data/nats")
    }
    pub fn logs(&self) -> PathBuf {
        self.data.join("logs")
    }
    pub fn rooms(&self) -> PathBuf {
        self.data.join("rooms")
    }
    pub fn host_state(&self) -> PathBuf {
        self.data.join("state/host")
    }
    pub fn legacy_backup(&self) -> PathBuf {
        self.backups().join("legacy-preinstall")
    }
}

pub fn safe_version(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && !value.contains("..")
}
