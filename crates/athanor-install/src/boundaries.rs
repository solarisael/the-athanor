use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard, TryLockError},
    thread,
    time::{Duration, Instant},
};

pub trait FileSystem {
    fn exists(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    fn list_directories(&self, path: &Path) -> Result<Vec<PathBuf>>;
    fn validate_regular_file(&self, root: &Path, path: &Path) -> Result<()> {
        if !path.starts_with(root) {
            bail!(
                "file {} escapes its declared root {}",
                path.display(),
                root.display()
            );
        }
        if !self.exists(path) {
            bail!("required regular file is missing: {}", path.display());
        }
        Ok(())
    }
    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> Result<()>;
    /// Installs `from` at `to` while `to` may be a running image. Windows
    /// refuses to overwrite a running executable and allows renaming it, so a
    /// live `to` moves aside as `<name>.retired-<n>` first. Retired siblings
    /// are deleted once nothing holds them, on this call or a later one.
    fn replace_file(&self, from: &Path, to: &Path) -> Result<()>;
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn remove_file(&self, path: &Path) -> Result<()>;
    fn remove_tree(&self, path: &Path) -> Result<()>;
    fn restrict_acl(&self, path: &Path) -> Result<()>;
    fn restrict_user_acl(&self, path: &Path, principal: &str) -> Result<()>;
}

static PROCESS_OPERATION_LOCK: Mutex<()> = Mutex::new(());
const OPERATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
/// The Win32 named mutex that makes "one Athanor operation at a time" true
/// across processes, not just across threads.
///
/// enough: the `Global\` prefix and the `.v1` suffix are the contract with
/// every other installed copy of this binary -- an older `athanor-manage.exe`
/// already running holds this exact name, so renaming it would let two
/// installers mutate one House at once. Changing it is a release-day migration.
#[cfg(windows)]
const WINDOWS_OPERATION_MUTEX_LABEL: &str = r"Global\AthanorManagerMutation.v1";

/// Widens an ASCII label into the NUL-terminated UTF-16 Win32 wants.
///
/// The ASCII assertion is what makes the widening byte-exact: below 0x80 a
/// UTF-8 byte and its UTF-16 code unit are the same number, so this cannot
/// silently mangle a name the way a real encoder could.
#[cfg(windows)]
const fn nul_terminated_utf16<const N: usize>(label: &str) -> [u16; N] {
    let bytes = label.as_bytes();
    assert!(
        bytes.len() + 1 == N,
        "the array must hold exactly the label and its NUL"
    );
    let mut wide = [0u16; N];
    let mut index = 0;
    while index < bytes.len() {
        assert!(
            bytes[index] < 0x80,
            "the mutex name must stay ASCII so widening is byte-exact"
        );
        wide[index] = bytes[index] as u16;
        index += 1;
    }
    wide
}

#[cfg(windows)]
static WINDOWS_OPERATION_MUTEX_NAME: [u16; WINDOWS_OPERATION_MUTEX_LABEL.len() + 1] =
    nul_terminated_utf16(WINDOWS_OPERATION_MUTEX_LABEL);

pub struct OperationLock {
    _process: MutexGuard<'static, ()>,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl OperationLock {
    pub fn acquire() -> Result<Self> {
        let deadline = Instant::now() + OPERATION_LOCK_TIMEOUT;
        let process = loop {
            match PROCESS_OPERATION_LOCK.try_lock() {
                Ok(guard) => break guard,
                Err(TryLockError::Poisoned(error)) => break error.into_inner(),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(TryLockError::WouldBlock) => {
                    bail!("timed out waiting for the process-wide Athanor manager operation lock")
                }
            }
        };

        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::WaitForSingleObject,
            };
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn CreateMutexW(
                    mutex_attributes: *const std::ffi::c_void,
                    initial_owner: i32,
                    name: *const u16,
                ) -> HANDLE;
            }

            // SAFETY: the static name is NUL-terminated; null selects the default
            // security descriptor, and the returned handle is checked.
            let handle =
                unsafe { CreateMutexW(std::ptr::null(), 0, WINDOWS_OPERATION_MUTEX_NAME.as_ptr()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error())
                    .context("create the cross-process Athanor manager operation mutex");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let milliseconds = remaining.as_millis().min(u32::MAX as u128) as u32;
            // SAFETY: `handle` is a live mutex handle owned by this scope.
            let status = unsafe { WaitForSingleObject(handle, milliseconds) };
            match status {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self {
                    _process: process,
                    handle,
                }),
                WAIT_TIMEOUT => {
                    // SAFETY: ownership was not acquired, so only the live handle is closed.
                    unsafe { CloseHandle(handle) };
                    bail!("timed out waiting for the cross-process Athanor manager operation mutex")
                }
                _ => {
                    let error = std::io::Error::last_os_error();
                    // SAFETY: ownership was not acquired, so only the live handle is closed.
                    unsafe { CloseHandle(handle) };
                    Err(error).context("wait for the cross-process Athanor manager operation mutex")
                }
            }
        }
        #[cfg(not(windows))]
        {
            Ok(Self { _process: process })
        }
    }
}

#[cfg(windows)]
impl Drop for OperationLock {
    fn drop(&mut self) {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::ReleaseMutex};
        // SAFETY: successful waits grant this guard mutex ownership; the guard is
        // the sole owner of the live handle and releases before closing it.
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
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

fn validate_physical_entry(path: &Path, final_entry: bool) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    let mut is_reparse = metadata.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        is_reparse |= metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    if is_reparse {
        bail!(
            "refusing reparse or symbolic-link install path {}",
            path.display()
        );
    }
    if final_entry {
        if !metadata.is_file() {
            bail!("install artifact is not a regular file: {}", path.display());
        }
    } else if !metadata.is_dir() {
        bail!(
            "install artifact ancestor is not a directory: {}",
            path.display()
        );
    }
    Ok(())
}

const RETIRED_MARK: &str = ".retired-";

fn retired_sibling(path: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    path.with_file_name(format!("{name}{RETIRED_MARK}{stamp}"))
}

// A retired image stays locked while a session still runs it; a failed
// delete here is that session, and the next replacement tries again.
fn sweep_retired(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().contains(RETIRED_MARK) {
            let _ = fs::remove_file(entry.path());
        }
    }
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
    fn list_directories(&self, path: &Path) -> Result<Vec<PathBuf>> {
        fs::read_dir(path)
            .with_context(|| format!("enumerate {}", path.display()))?
            .filter_map(|entry| match entry {
                Ok(entry) => match entry.file_type() {
                    Ok(file_type) if file_type.is_dir() => Some(Ok(entry.path())),
                    Ok(_) => None,
                    Err(error) => Some(Err(error).with_context(|| {
                        format!("inspect directory entry below {}", path.display())
                    })),
                },
                Err(error) => Some(
                    Err(error)
                        .with_context(|| format!("read directory entry below {}", path.display())),
                ),
            })
            .collect()
    }
    fn validate_regular_file(&self, root: &Path, path: &Path) -> Result<()> {
        let relative = path.strip_prefix(root).with_context(|| {
            format!(
                "file {} escapes its declared root {}",
                path.display(),
                root.display()
            )
        })?;
        let mut cursor = root.to_path_buf();
        let mut entries = relative.components().peekable();
        validate_physical_entry(&cursor, entries.peek().is_none())?;
        while let Some(component) = entries.next() {
            use std::path::Component;
            match component {
                Component::Normal(name) => cursor.push(name),
                _ => bail!(
                    "file {} has a non-physical path below {}",
                    path.display(),
                    root.display()
                ),
            }
            validate_physical_entry(&cursor, entries.peek().is_none())?;
        }
        Ok(())
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
    fn replace_file(&self, from: &Path, to: &Path) -> Result<()> {
        if to.exists() {
            let retired = retired_sibling(to);
            fs::rename(to, &retired)
                .with_context(|| format!("retire {} to {}", to.display(), retired.display()))?;
        }
        self.copy(from, to)?;
        if let Some(parent) = to.parent() {
            sweep_retired(parent);
        }
        Ok(())
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
            let (system_grant, administrators_grant) = if path.is_dir() {
                ("SYSTEM:(OI)(CI)F", "*S-1-5-32-544:(OI)(CI)F")
            } else {
                ("SYSTEM:F", "*S-1-5-32-544:F")
            };
            let status = Command::new("icacls.exe")
                .arg(path)
                .args([
                    "/inheritance:r",
                    "/grant:r",
                    system_grant,
                    administrators_grant,
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
    fn restrict_user_acl(&self, path: &Path, principal: &str) -> Result<()> {
        if principal.trim().is_empty() {
            bail!("operator principal must not be empty");
        }
        #[cfg(windows)]
        {
            let resolved_principal = if principal.contains('\\') || principal.starts_with('*') {
                principal.to_owned()
            } else {
                let output = Command::new("whoami.exe").output()?;
                if !output.status.success() {
                    bail!("failed to resolve the invoking Windows operator");
                }
                String::from_utf8(output.stdout)?.trim().to_owned()
            };
            let is_directory = path.is_dir();
            let user_grant = if is_directory {
                format!("{resolved_principal}:(OI)(CI)F")
            } else {
                format!("{resolved_principal}:F")
            };
            let system_grant = if is_directory {
                "SYSTEM:(OI)(CI)F"
            } else {
                "SYSTEM:F"
            };
            let administrators_grant = if is_directory {
                "*S-1-5-32-544:(OI)(CI)F"
            } else {
                "*S-1-5-32-544:F"
            };
            let status = Command::new("icacls.exe")
                .arg(path)
                .args([
                    "/inheritance:r",
                    "/grant:r",
                    system_grant,
                    administrators_grant,
                    &user_grant,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !status.success() {
                bail!("failed to restrict operator ACL on {}", path.display());
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (path, principal);
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
        self.run(&["start", name], false)?;
        #[cfg(windows)]
        {
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                let output = Command::new("sc.exe").args(["query", name]).output()?;
                let state = String::from_utf8_lossy(&output.stdout);
                if output.status.success() && state.contains("STATE") && state.contains("RUNNING") {
                    return Ok(());
                }
                if state.contains("STOPPED") {
                    bail!("Windows service {name} stopped before reaching readiness");
                }
                if Instant::now() >= deadline {
                    bail!("Windows service {name} did not report RUNNING within 120 seconds");
                }
                thread::sleep(Duration::from_millis(500));
            }
        }
        #[cfg(not(windows))]
        Ok(())
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    // The deploy case: a session still runs the stable image while the
    // installer lands the next one at the same path. Plain fs::copy fails
    // with a sharing violation; replace_file must not.
    #[test]
    fn replace_file_lands_over_a_running_image_and_retires_the_old_one() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let bin = temporary.path().join("bin");
        fs::create_dir_all(&bin)?;
        let system32 = PathBuf::from(std::env::var("SystemRoot")?).join("System32");
        let live = bin.join("athanor.exe");
        fs::copy(system32.join("ping.exe"), &live)?;
        let mut running = Command::new(&live)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let next = temporary.path().join("next.exe");
        fs::copy(system32.join("timeout.exe"), &next)?;

        let plain = NativeFileSystem.copy(&next, &live);
        assert!(plain.is_err(), "fs::copy over a running image must fail");
        NativeFileSystem.replace_file(&next, &live)?;

        assert_eq!(fs::read(&live)?, fs::read(&next)?);
        assert!(
            running.try_wait()?.is_none(),
            "the old session must keep running"
        );
        let retired: Vec<PathBuf> = fs::read_dir(&bin)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().contains(RETIRED_MARK))
            .collect();
        assert_eq!(retired.len(), 1, "the live image moved aside exactly once");

        running.kill()?;
        running.wait()?;
        NativeFileSystem.replace_file(&next, &live)?;
        let remaining = fs::read_dir(&bin)?
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(RETIRED_MARK))
            .count();
        assert_eq!(
            remaining, 0,
            "nothing holds the retired images, so the sweep removes them"
        );
        Ok(())
    }
}
