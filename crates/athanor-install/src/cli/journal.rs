//! The undo log of the room doors. It records each file change of one run,
//! so that a failure undoes exactly what this run did and nothing that
//! existed before it.

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

pub(crate) fn pretty(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;

    bytes.push(b'\n');
    Ok(bytes)
}

#[derive(Default)]
pub(crate) struct Journal {
    steps: Vec<Step>,
}

enum Step {
    CreatedDir(PathBuf),
    CreatedFile(PathBuf),
    Rewrote { path: PathBuf, bytes: Vec<u8> },
    Moved { from: PathBuf, to: PathBuf },
}

impl Journal {
    pub(crate) fn create_dirs(&mut self, path: &Path) -> Result<()> {
        let missing: Vec<PathBuf> = path
            .ancestors()
            .take_while(|folder| !folder.exists())
            .map(Path::to_path_buf)
            .collect();

        for folder in missing.into_iter().rev() {
            fs::create_dir(&folder).with_context(|| format!("create {}", folder.display()))?;
            self.steps.push(Step::CreatedDir(folder));
        }
        Ok(())
    }

    pub(crate) fn create_file(&mut self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create {}", path.display()))?;

        self.steps.push(Step::CreatedFile(path.to_owned()));
        file.write_all(bytes)
            .with_context(|| format!("write {}", path.display()))
    }

    /// A missing file stays missing here; the caller writes a fresh one.
    pub(crate) fn rewrite_json(
        &mut self,
        path: &Path,
        change: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<()> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        let mut document: Map<String, Value> = serde_json::from_slice(&bytes)
            .with_context(|| format!("{} is not a JSON object", path.display()))?;

        change(&mut document);
        self.rewrite(path, &pretty(&document)?)
    }

    /// Writes `bytes` over the file at `path`; the undo writes the old bytes back.
    pub(crate) fn rewrite(&mut self, path: &Path, bytes: &[u8]) -> Result<()> {
        let old = fs::read(path).with_context(|| format!("read {}", path.display()))?;

        self.steps.push(Step::Rewrote {
            path: path.to_owned(),
            bytes: old,
        });
        fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
    }

    /// Moves a file or a folder to `to`, which must not exist.
    pub(crate) fn move_path(&mut self, from: &Path, to: &Path) -> Result<()> {
        relocate(from, to)?;
        self.steps.push(Step::Moved {
            from: from.to_owned(),
            to: to.to_owned(),
        });
        Ok(())
    }

    /// Undoes the steps in reverse order and names the path of each failure.
    pub(crate) fn undo(self) -> Result<(), String> {
        let failures: Vec<String> = self
            .steps
            .into_iter()
            .rev()
            .filter_map(|step| {
                let (place, result) = match &step {
                    Step::CreatedDir(path) => (path.display().to_string(), fs::remove_dir(path).map_err(Into::into)),
                    Step::CreatedFile(path) => (path.display().to_string(), fs::remove_file(path).map_err(Into::into)),
                    Step::Rewrote { path, bytes } => (path.display().to_string(), fs::write(path, bytes).map_err(Into::into)),
                    Step::Moved { from, to } => (
                        format!("{} back to {}", to.display(), from.display()),
                        relocate(to, from),
                    ),
                };
                result.err().map(|error: anyhow::Error| format!("{place}: {error:#}"))
            })
            .collect();

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// The error of a failed run after its undo, which names each path that the
/// undo could not restore.
pub(crate) fn undone(error: anyhow::Error, journal: Journal) -> anyhow::Error {
    match journal.undo() {
        Ok(()) => anyhow!("{error:#}; the file changes of this run were undone"),
        Err(failures) => anyhow!("{error:#}; the undo of the file changes also failed: {failures}"),
    }
}

/// One rename on one filesystem. Across filesystems, a copy whose every file
/// is synced and matches its source by SHA-256 before the source goes.
fn relocate(from: &Path, to: &Path) -> Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        bail!("{} already exists", to.display());
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::CrossesDevices => {
            if let Err(error) = copy_tree(from, to).and_then(|()| sync_parent(to)) {
                return Err(discard_copy(error, to));
            }
            remove_tree(from).with_context(|| {
                format!(
                    "remove {} after its verified copy to {}",
                    from.display(),
                    to.display()
                )
            })
        }
        Err(error) => {
            Err(error).with_context(|| format!("move {} to {}", from.display(), to.display()))
        }
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(from).with_context(|| format!("read {}", from.display()))?;
    if metadata.is_dir() {
        fs::create_dir(to).with_context(|| format!("create {}", to.display()))?;
        for entry in fs::read_dir(from).with_context(|| format!("read {}", from.display()))? {
            let entry = entry.with_context(|| format!("read {}", from.display()))?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        fs::set_permissions(to, metadata.permissions())
            .with_context(|| format!("set the permissions of {}", to.display()))?;
        return sync_folder(to);
    }
    if !metadata.is_file() {
        bail!(
            "{} is a link or a device; a move across filesystems copies only files and folders",
            from.display()
        );
    }
    fs::copy(from, to).with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(to)
        .and_then(|file| file.sync_all())
        .with_context(|| format!("sync {}", to.display()))?;
    if digest(from)? != digest(to)? {
        bail!("the copy {} differs from {} by SHA-256", to.display(), from.display());
    }
    Ok(())
}

fn digest(path: &Path) -> Result<Vec<u8>> {
    let mut hasher = Sha256::new();
    let mut file = File::open(path).with_context(|| format!("read {}", path.display()))?;

    io::copy(&mut file, &mut hasher).with_context(|| format!("read {}", path.display()))?;
    Ok(hasher.finalize().to_vec())
}

fn discard_copy(error: anyhow::Error, to: &Path) -> anyhow::Error {
    match remove_tree(to) {
        Ok(()) => error,
        Err(cleanup) => anyhow!(
            "{error:#}; the partial copy {} also stays: {cleanup:#}",
            to.display()
        ),
    }
}

fn remove_tree(path: &Path) -> Result<()> {
    let removed = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => Err(error),
    };
    removed.with_context(|| format!("remove {}", path.display()))
}

fn sync_parent(path: &Path) -> Result<()> {
    path.parent().map_or(Ok(()), sync_folder)
}

#[cfg(unix)]
fn sync_folder(folder: &Path) -> Result<()> {
    File::open(folder)
        .and_then(|file| file.sync_all())
        .with_context(|| format!("sync {}", folder.display()))
}

#[cfg(not(unix))]
fn sync_folder(_folder: &Path) -> Result<()> {
    // Windows flushes each copied file through File::sync_all. The standard
    // library cannot open a folder for FlushFileBuffers.
    Ok(())
}
