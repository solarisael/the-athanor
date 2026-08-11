use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub trait FileSystem {
    fn exists(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> Result<()>;
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn remove_file(&self, path: &Path) -> Result<()>;
    fn remove_tree(&self, path: &Path) -> Result<()>;
    fn restrict_acl(&self, path: &Path) -> Result<()>;
}

pub trait ServiceManager {
    fn install_or_update(&self, name: &str, display_name: &str, executable: &Path) -> Result<()>;
    fn start(&self, name: &str) -> Result<()>;
    fn stop(&self, name: &str) -> Result<()>;
    fn remove(&self, name: &str) -> Result<()>;
    fn is_installed(&self, name: &str) -> Result<bool>;
}

pub trait RuntimeControl {
    fn backup_database(&self, backup_dir: &Path) -> Result<Option<PathBuf>>;
    fn import_legacy(&self, source: &Path, backup_dir: &Path) -> Result<()>;
    fn migrate_database(&self) -> Result<()>;
    fn restore_database(&self, backup: &Path) -> Result<()>;
    fn wait_ready(&self) -> Result<()>;
}

pub trait SecretSource {
    fn fill(&self, destination: &mut [u8]) -> Result<()>;
}

#[derive(Default)]
pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        fs::read(path).with_context(|| format!("read {}", path.display()))
    }
    fn create_dir_all(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))
    }
    fn copy(&self, from: &Path, to: &Path) -> Result<()> {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)
            .with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
        Ok(())
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let parent = path.parent().context("atomic write target has no parent")?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("new");
        fs::write(&temporary, bytes).with_context(|| format!("write {}", temporary.display()))?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        fs::rename(&temporary, path).with_context(|| format!("commit {}", path.display()))
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        fs::rename(from, to)
            .with_context(|| format!("rename {} to {}", from.display(), to.display()))
    }
    fn remove_file(&self, path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
        }
        Ok(())
    }
    fn remove_tree(&self, path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
        }
        Ok(())
    }
    fn restrict_acl(&self, path: &Path) -> Result<()> {
        #[cfg(windows)]
        {
            let status = Command::new("icacls.exe")
                .arg(path)
                .args([
                    "/inheritance:r",
                    "/grant:r",
                    "SYSTEM:(OI)(CI)F",
                    "*S-1-5-32-544:(OI)(CI)F",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                bail!("failed to restrict ACL on {}", path.display());
            }
        }
        #[cfg(not(windows))]
        {
            let _ = path;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct OsSecrets;
impl SecretSource for OsSecrets {
    fn fill(&self, destination: &mut [u8]) -> Result<()> {
        getrandom::fill(destination)
            .map_err(|error| anyhow::anyhow!("operating-system random source failed: {error}"))
    }
}

#[derive(Default)]
pub struct ScServiceManager;
impl ScServiceManager {
    fn run(&self, args: &[&str], allow_missing: bool) -> Result<()> {
        #[cfg(windows)]
        {
            let output = Command::new("sc.exe")
                .args(args)
                .output()
                .context("launch Service Control Manager client")?;
            if output.status.success()
                || (allow_missing
                    && [1060, 1062].iter().any(|code| {
                        String::from_utf8_lossy(&output.stdout).contains(&code.to_string())
                    }))
            {
                return Ok(());
            }
            bail!(
                "Service Control Manager rejected operation: {}",
                String::from_utf8_lossy(&output.stdout).trim()
            );
        }
        #[cfg(not(windows))]
        {
            let _ = (args, allow_missing);
            bail!("Windows services are only available on Windows")
        }
    }
}
impl ServiceManager for ScServiceManager {
    fn install_or_update(&self, name: &str, display_name: &str, executable: &Path) -> Result<()> {
        let binary = format!("\"{}\" service", executable.display());
        if self.is_installed(name)? {
            self.run(
                &[
                    "config",
                    name,
                    "binPath=",
                    &binary,
                    "start=",
                    "auto",
                    "DisplayName=",
                    display_name,
                ],
                false,
            )
        } else {
            self.run(
                &[
                    "create",
                    name,
                    "binPath=",
                    &binary,
                    "start=",
                    "auto",
                    "DisplayName=",
                    display_name,
                ],
                false,
            )
        }
    }
    fn start(&self, name: &str) -> Result<()> {
        self.run(&["start", name], false)
    }
    fn stop(&self, name: &str) -> Result<()> {
        self.run(&["stop", name], true)
    }
    fn remove(&self, name: &str) -> Result<()> {
        self.run(&["delete", name], true)
    }
    fn is_installed(&self, name: &str) -> Result<bool> {
        #[cfg(windows)]
        {
            Ok(Command::new("sc.exe")
                .args(["query", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?
                .success())
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Ok(false)
        }
    }
}
