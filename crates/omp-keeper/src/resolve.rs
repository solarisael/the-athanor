use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const POINTER_FILE_NAME: &str = "current.json";
pub const VERSIONS_DIRECTORY: &str = "versions";
pub const BIN_DIRECTORY: &str = "bin";
const MAX_VERSION_LENGTH: usize = 128;

/// Resolves the substrate the loader would load now: contract line 57-59, never a frozen path.
pub fn resolve_substrate_exe(program_root: &Path) -> Result<PathBuf> {
    let root = PhysicalDirectory::open(program_root, "program root", None)?;
    let pointer = root.regular_file(POINTER_FILE_NAME, "current release pointer")?;
    let version = pointer_version(&pointer)?;
    let versions = root.child(VERSIONS_DIRECTORY, "active native release ancestor versions")?;
    let release = versions.child(&version, "active native release")?;
    let bin = release.child(BIN_DIRECTORY, "active native release ancestor bin")?;
    bin.regular_file(substrate_exe_name(), "substrate executable")
}

/// Mirrors rustBinaryName in adapters/omp/discovery.ts.
pub fn substrate_exe_name() -> &'static str {
    if cfg!(windows) {
        "athanor-substrate.exe"
    } else {
        "athanor-substrate"
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NativePointer {
    version: String,
    #[serde(default)]
    previous_version: Option<String>,
    // the pointer also carries rollbackBackup; the loader ignores it here and so does the keeper
    #[serde(default)]
    #[allow(dead_code)]
    rollback_backup: Option<String>,
}

fn pointer_version(pointer: &Path) -> Result<String> {
    let text = std::fs::read_to_string(pointer)
        .with_context(|| format!("current release pointer could not be read: {}", pointer.display()))?;
    let parsed: NativePointer = serde_json::from_str(&text)
        .context("current release pointer is not a valid release pointer")?;
    let version = safe_version(&parsed.version, "current release version")?;
    if let Some(previous) = &parsed.previous_version {
        let previous = safe_version(previous, "current release previousVersion")?;
        if previous == version {
            bail!("current release pointer cannot name its active version as previous");
        }
    }
    Ok(version)
}

fn safe_version(value: &str, field: &str) -> Result<String> {
    let safe = !value.is_empty()
        && value.len() <= MAX_VERSION_LENGTH
        && !value.contains("..")
        && value
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '+' | '-'));
    if !safe {
        bail!("{field} is unsafe");
    }
    Ok(value.to_string())
}

struct PhysicalDirectory {
    logical: PathBuf,
    root_physical: PathBuf,
}

impl PhysicalDirectory {
    fn open(directory: &Path, label: &str, parent: Option<&PhysicalDirectory>) -> Result<Self> {
        let logical = std::path::absolute(directory)
            .with_context(|| format!("installed Athanor {label} path could not be resolved"))?;
        let status = std::fs::symlink_metadata(&logical)
            .with_context(|| format!("installed Athanor {label} could not be inspected"))?;
        if status.is_symlink() || !status.is_dir() {
            bail!(
                "installed Athanor {label} must be a physical directory; symbolic links, junctions, and reparse points are refused"
            );
        }
        let physical = std::fs::canonicalize(&logical)
            .with_context(|| format!("installed Athanor {label} physical path could not be resolved"))?;
        let root_physical = match parent {
            Some(parent) => parent.root_physical.clone(),
            None => physical.clone(),
        };
        if !physical.starts_with(&root_physical) {
            bail!("installed Athanor {label} escapes its physical root");
        }
        Ok(Self {
            logical,
            root_physical,
        })
    }

    fn child(&self, part: &str, label: &str) -> Result<Self> {
        Self::open(&self.join(part, label)?, label, Some(self))
    }

    fn regular_file(&self, part: &str, label: &str) -> Result<PathBuf> {
        let logical = self.join(part, label)?;
        let status = std::fs::symlink_metadata(&logical)
            .with_context(|| format!("installed Athanor {label} is missing or invalid"))?;
        if status.is_symlink() || !status.is_file() {
            bail!(
                "installed Athanor {label} must be a regular physical file; symbolic links, junctions, and reparse points are refused"
            );
        }
        let physical = std::fs::canonicalize(&logical)
            .with_context(|| format!("installed Athanor {label} physical path could not be resolved"))?;
        if !physical.starts_with(&self.root_physical) {
            bail!("installed Athanor {label} escapes its physical root");
        }
        Ok(logical)
    }

    fn join(&self, part: &str, label: &str) -> Result<PathBuf> {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('/')
            || part.contains('\\')
        {
            bail!("installed Athanor {label} name {part} is unsafe");
        }
        Ok(self.logical.join(part))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Tree {
        _temp: tempfile::TempDir,
        program_root: PathBuf,
    }

    fn tree(pointer: &str, version_directory: Option<&str>) -> Tree {
        let temp = tempfile::tempdir().expect("temp dir");
        let program_root = temp.path().join("program");
        fs::create_dir_all(&program_root).expect("program root");
        fs::write(program_root.join(POINTER_FILE_NAME), pointer).expect("pointer");
        if let Some(version) = version_directory {
            let bin = program_root
                .join(VERSIONS_DIRECTORY)
                .join(version)
                .join(BIN_DIRECTORY);
            fs::create_dir_all(&bin).expect("bin");
            fs::write(bin.join(substrate_exe_name()), b"substrate").expect("substrate");
        }
        Tree {
            _temp: temp,
            program_root,
        }
    }

    #[test]
    fn resolves_the_exe_the_pointer_names() {
        let tree = tree(r#"{"version":"0.11.3"}"#, Some("0.11.3"));
        let resolved = resolve_substrate_exe(&tree.program_root).expect("resolves");
        assert_eq!(
            resolved,
            std::path::absolute(
                tree.program_root
                    .join(VERSIONS_DIRECTORY)
                    .join("0.11.3")
                    .join(BIN_DIRECTORY)
                    .join(substrate_exe_name())
            )
            .expect("absolute")
        );
    }

    #[test]
    fn follows_the_pointer_when_it_moves() {
        let tree = tree(r#"{"version":"0.11.3"}"#, Some("0.11.3"));
        let bin = tree
            .program_root
            .join(VERSIONS_DIRECTORY)
            .join("0.11.4")
            .join(BIN_DIRECTORY);
        fs::create_dir_all(&bin).expect("next bin");
        fs::write(bin.join(substrate_exe_name()), b"next").expect("next substrate");
        fs::write(
            tree.program_root.join(POINTER_FILE_NAME),
            r#"{"version":"0.11.4","previousVersion":"0.11.3"}"#,
        )
        .expect("rewrite pointer");
        let resolved = resolve_substrate_exe(&tree.program_root).expect("resolves");
        assert!(resolved.ends_with(
            Path::new(VERSIONS_DIRECTORY)
                .join("0.11.4")
                .join(BIN_DIRECTORY)
                .join(substrate_exe_name())
        ));
    }

    #[test]
    fn refuses_a_pointer_whose_version_directory_is_absent() {
        let tree = tree(r#"{"version":"0.11.9"}"#, Some("0.11.3"));
        let error = resolve_substrate_exe(&tree.program_root).expect_err("missing release refuses");
        assert!(format!("{error:#}").contains("active native release"));
    }

    #[test]
    fn refuses_a_pointer_that_escapes_the_program_root() {
        let tree = tree(r#"{"version":"../outside"}"#, Some("0.11.3"));
        let error = resolve_substrate_exe(&tree.program_root).expect_err("escape refuses");
        assert!(format!("{error:#}").contains("unsafe"));
    }

    #[test]
    fn refuses_a_pointer_with_a_traversal_or_separator_version() {
        for version in ["a..b", "sub/dir", "sub\\dir", "", "-lead"] {
            let pointer = format!("{{\"version\":{}}}", serde_json::json!(version));
            let tree = tree(&pointer, Some("0.11.3"));
            let error =
                resolve_substrate_exe(&tree.program_root).expect_err("unsafe version refuses");
            assert!(
                format!("{error:#}").contains("unsafe"),
                "version {version} must refuse as unsafe"
            );
        }
    }

    #[test]
    fn refuses_a_pointer_with_unknown_or_missing_fields() {
        let missing = tree(r#"{"release":"0.11.3"}"#, Some("0.11.3"));
        let error =
            resolve_substrate_exe(&missing.program_root).expect_err("missing version refuses");
        assert!(format!("{error:#}").contains("not a valid release pointer"));

        let unknown = tree(r#"{"version":"0.11.3","surprise":true}"#, Some("0.11.3"));
        let error =
            resolve_substrate_exe(&unknown.program_root).expect_err("unknown field refuses");
        assert!(format!("{error:#}").contains("not a valid release pointer"));
    }

    #[test]
    fn refuses_a_pointer_naming_its_active_version_as_previous() {
        let tree = tree(
            r#"{"version":"0.11.3","previousVersion":"0.11.3"}"#,
            Some("0.11.3"),
        );
        let error = resolve_substrate_exe(&tree.program_root).expect_err("self previous refuses");
        assert!(format!("{error:#}").contains("cannot name its active version as previous"));
    }

    #[test]
    fn refuses_a_program_root_that_is_not_a_directory() {
        let temp = tempfile::tempdir().expect("temp dir");
        let file = temp.path().join("program");
        fs::write(&file, b"not a directory").expect("file");
        let error = resolve_substrate_exe(&file).expect_err("file root refuses");
        assert!(format!("{error:#}").contains("must be a physical directory"));
    }

    #[test]
    fn refuses_a_release_without_the_substrate_exe() {
        let tree = tree(r#"{"version":"0.11.3"}"#, None);
        fs::create_dir_all(
            tree.program_root
                .join(VERSIONS_DIRECTORY)
                .join("0.11.3")
                .join(BIN_DIRECTORY),
        )
        .expect("empty bin");
        let error = resolve_substrate_exe(&tree.program_root).expect_err("missing exe refuses");
        assert!(format!("{error:#}").contains("substrate executable"));
    }
}
