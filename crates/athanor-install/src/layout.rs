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

    /// The roots of this install, for the Host and each CLI mode that reads it.
    /// `ATHANOR_PROGRAM_ROOT` and `ATHANOR_DATA_ROOT` win when both are set.
    /// Without them, Windows uses `ProgramFiles` and `ProgramData`. Linux uses
    /// the folder of this executable and `$XDG_STATE_HOME/athanor`, else
    /// `$HOME/.local/state/athanor`: the roots that the systemd unit of the
    /// Linux House sets.
    pub fn from_environment() -> Result<Self> {
        Self::resolve(
            |name| std::env::var_os(name),
            || std::env::current_exe().ok(),
        )
    }

    /// The decision of [`Self::from_environment`], with the environment and
    /// the executable path injected, so that a test changes no process state.
    fn resolve(
        variable: impl Fn(&str) -> Option<std::ffi::OsString>,
        executable: impl FnOnce() -> Option<PathBuf>,
    ) -> Result<Self> {
        match (variable(PROGRAM_ROOT_ENV), variable(DATA_ROOT_ENV)) {
            (Some(program), Some(data)) if !program.is_empty() && !data.is_empty() => Ok(Self {
                program: PathBuf::from(program),
                data: PathBuf::from(data),
            }),
            (None, None) if cfg!(windows) => {
                let program_files =
                    PathBuf::from(variable("ProgramFiles").context("ProgramFiles is unavailable")?);
                let program_data =
                    PathBuf::from(variable("ProgramData").context("ProgramData is unavailable")?);
                Ok(Self::new(&program_files, &program_data))
            }
            (None, None) => Self::linux_roots(variable, executable),
            _ => anyhow::bail!("{PROGRAM_ROOT_ENV} and {DATA_ROOT_ENV} must be set together"),
        }
    }

    fn linux_roots(
        variable: impl Fn(&str) -> Option<std::ffi::OsString>,
        executable: impl FnOnce() -> Option<PathBuf>,
    ) -> Result<Self> {
        let unavailable = |input: &str| {
            format!("{input} is unavailable; set {PROGRAM_ROOT_ENV} and {DATA_ROOT_ENV}")
        };
        // XDG ignores a base directory that is not absolute, and so does the data root.
        let absolute = |name: &str| {
            variable(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };

        let program = executable()
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .with_context(|| unavailable("the path of this executable"))?;
        let state_home = absolute("XDG_STATE_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".local/state")))
            .with_context(|| unavailable("HOME"))?;

        Ok(Self {
            program,
            data: state_home.join("athanor"),
        })
    }

    pub fn versions(&self) -> PathBuf {
        self.program.join("versions")
    }
    pub fn version(&self, version: &str) -> PathBuf {
        self.versions().join(version)
    }
    /// The one exe: Host, service, installer, keeper, chat, status, and
    /// start are its modes.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    /// A process environment that holds exactly these variables.
    fn environment(
        variables: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            variables
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(*value))
        }
    }

    fn executable() -> Option<PathBuf> {
        Some(PathBuf::from("/srv/athanor/target/release/athanor"))
    }

    #[test]
    fn explicit_roots_win_over_every_default() {
        let layout = InstallLayout::resolve(
            environment(&[
                (PROGRAM_ROOT_ENV, "/opt/athanor"),
                (DATA_ROOT_ENV, "/var/lib/athanor"),
                ("XDG_STATE_HOME", "/home/sol/.state"),
                ("HOME", "/home/sol"),
                ("ProgramFiles", "C:/Program Files"),
                ("ProgramData", "C:/ProgramData"),
            ]),
            executable,
        )
        .expect("both variables name the roots");

        assert_eq!(
            layout,
            InstallLayout {
                program: PathBuf::from("/opt/athanor"),
                data: PathBuf::from("/var/lib/athanor"),
            }
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_takes_the_executable_folder_and_the_state_home() {
        let state_home = InstallLayout::resolve(
            environment(&[
                ("XDG_STATE_HOME", "/home/sol/.state"),
                ("HOME", "/home/sol"),
            ]),
            executable,
        )
        .expect("XDG_STATE_HOME names the data root");
        let home = InstallLayout::resolve(environment(&[("HOME", "/home/sol")]), executable)
            .expect("HOME names the data root");
        let relative_state_home = InstallLayout::resolve(
            environment(&[("XDG_STATE_HOME", "state"), ("HOME", "/home/sol")]),
            executable,
        )
        .expect("HOME names the data root");

        assert_eq!(
            state_home,
            InstallLayout {
                program: PathBuf::from("/srv/athanor/target/release"),
                data: PathBuf::from("/home/sol/.state/athanor"),
            }
        );
        assert_eq!(home.data, PathBuf::from("/home/sol/.local/state/athanor"));
        assert_eq!(relative_state_home.data, home.data);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_root_that_cannot_be_found_names_the_variables_to_set() {
        let no_home = InstallLayout::resolve(environment(&[]), executable)
            .unwrap_err()
            .to_string();
        let no_executable = InstallLayout::resolve(environment(&[("HOME", "/home/sol")]), || None)
            .unwrap_err()
            .to_string();

        assert!(no_home.contains("HOME"), "{no_home}");
        for error in [no_home, no_executable] {
            assert!(error.contains(PROGRAM_ROOT_ENV), "{error}");
            assert!(error.contains(DATA_ROOT_ENV), "{error}");
            assert!(!error.contains("ProgramFiles"), "{error}");
        }
    }
}
