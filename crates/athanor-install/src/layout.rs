use std::path::{Path, PathBuf};

pub const PRODUCT_DIR: &str = "Solarisael/Athanor";
pub const SERVICE_NAME: &str = "SolarisaelAthanor";
pub const SERVICE_DISPLAY_NAME: &str = "Solarisael Athanor";
pub const CURRENT_POINTER: &str = "current.json";
pub const RUNTIME_CONFIG: &str = "config/runtime.json";
pub const SECRETS_FILE: &str = "secrets/runtime-secrets.json";
pub const LEGACY_NAMES: &[&str] = &[
    "solarisael-house",
    "solarisael-house-omp",
    "solarisael-house-substrate",
];

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

    pub fn versions(&self) -> PathBuf {
        self.program.join("versions")
    }
    pub fn version(&self, version: &str) -> PathBuf {
        self.versions().join(version)
    }
    pub fn manager(&self) -> PathBuf {
        self.program.join("bin/athanor-manage.exe")
    }
    pub fn current(&self) -> PathBuf {
        self.program.join(CURRENT_POINTER)
    }
    pub fn config(&self) -> PathBuf {
        self.data.join(RUNTIME_CONFIG)
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
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && !value.contains("..")
}
