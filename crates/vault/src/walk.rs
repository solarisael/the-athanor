use crate::config::VaultConfig;
use crate::ignore::{
    IGNORED_DIRECTORIES, eligible_file, ignored_by_rules, ignored_file, root_ignore_rules,
};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn normalize_lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component.as_os_str());
                }
            }
            _ => out.push(component.as_os_str()),
        }
    }
    out
}

pub(crate) fn collect_files(config: &VaultConfig) -> (Vec<(PathBuf, PathBuf)>, Vec<String>) {
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for root in &config.roots {
        let metadata = match fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(_) => {
                warnings.push(format!("Vault root unavailable: {}", normalized_path(root)));
                continue;
            }
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            warnings.push(format!(
                "Vault root is not a direct readable directory: {}",
                normalized_path(root)
            ));
            continue;
        }
        let canonical_root = match fs::canonicalize(root) {
            Ok(path) => path,
            Err(_) => {
                warnings.push(format!("Vault root unavailable: {}", normalized_path(root)));
                continue;
            }
        };
        let rules = root_ignore_rules(root, &config.ignore);
        let mut directories = vec![root.clone()];
        while let Some(directory) = directories.pop() {
            let mut entries = match fs::read_dir(&directory) {
                Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
                Err(_) => {
                    warnings.push(format!(
                        "Vault directory unreadable: {}",
                        normalized_path(&directory)
                    ));
                    continue;
                }
            };
            entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
            for entry in entries.into_iter().rev() {
                let absolute = entry.path();
                let relative = absolute
                    .strip_prefix(root)
                    .map(normalized_path)
                    .unwrap_or_default();
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => {
                        warnings.push(format!(
                            "{}: skipped unreadable entry.",
                            normalized_path(&absolute)
                        ));
                        continue;
                    }
                };
                if file_type.is_symlink() {
                    warnings.push(format!(
                        "{}: refused symbolic link.",
                        normalized_path(&absolute)
                    ));
                    continue;
                }
                if file_type.is_dir() {
                    let lower = entry.file_name().to_string_lossy().to_lowercase();
                    if IGNORED_DIRECTORIES.contains(&lower.as_str())
                        || ignored_by_rules(&relative, true, &rules)
                    {
                        continue;
                    }
                    directories.push(absolute);
                    continue;
                }
                if !file_type.is_file()
                    || ignored_file(&entry.file_name().to_string_lossy())
                    || ignored_by_rules(&relative, false, &rules)
                    || !eligible_file(&absolute)
                {
                    continue;
                }
                let canonical_file = match fs::canonicalize(&absolute) {
                    Ok(path) => path,
                    Err(_) => {
                        warnings.push(format!(
                            "{}: skipped unreadable text file.",
                            normalized_path(&absolute)
                        ));
                        continue;
                    }
                };
                if !canonical_file.starts_with(&canonical_root) {
                    warnings.push(format!(
                        "{}: refused path escaping configured Vault root.",
                        normalized_path(&absolute)
                    ));
                    continue;
                }
                files.push((root.clone(), absolute));
                if files.len() >= config.max_files {
                    warnings.push(format!(
                        "Vault file limit reached ({}); results cover only the scanned prefix.",
                        config.max_files
                    ));
                    return (files, warnings);
                }
            }
        }
    }
    files.sort_by(|left, right| normalized_path(&left.1).cmp(&normalized_path(&right.1)));
    (files, warnings)
}
