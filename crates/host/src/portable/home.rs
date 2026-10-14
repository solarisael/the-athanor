//! The configured home, read and written only inside its own roots.
//!
//! Identity context is the home's hand-owned `AGENTS.md` with its exact
//! `@reference` lines expanded once, from the home or its sibling `house`.
//! Personal documents are text files under the home; dot segments (including
//! `.omp/runtime` secrets) and reparse points are never reachable.

use super::Refusal;
use atomicwrites::{AtomicFile, OverwriteBehavior};
use axum::http::StatusCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const INSTRUCTIONS: &str = "AGENTS.md";
const HOUSE_ROOT: &str = "house";
const MAX_CONTEXT_BYTES: usize = 512 * 1024;
pub(super) const MAX_RESOURCE_BYTES: usize = 1024 * 1024;
const MAX_LIST_ENTRIES: usize = 2000;
const TEXT_EXTENSIONS: &[&str] = &["md", "txt", "json", "jsonl", "yaml", "yml", "toml"];

pub(super) struct Context {
    pub(super) revision: String,
    pub(super) text: String,
    pub(super) sources: Vec<Value>,
}

pub(super) fn revision(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn unavailable(message: String) -> Refusal {
    Refusal::new(StatusCode::SERVICE_UNAVAILABLE, "home_unavailable", message)
}

fn invalid(message: impl Into<String>) -> Refusal {
    Refusal::new(StatusCode::BAD_REQUEST, "path_invalid", message)
}

/// Compose the identity context exactly from the home's instruction file.
pub(super) fn context(home: &Path) -> Result<Context, Refusal> {
    let home_root = fs::canonicalize(home)
        .map_err(|error| unavailable(format!("home {} is unreadable: {error}", home.display())))?;
    let house_root = home
        .parent()
        .map(|parent| parent.join(HOUSE_ROOT))
        .and_then(|house| fs::canonicalize(house).ok());
    let instructions = home_root.join(INSTRUCTIONS);
    let body = fs::read_to_string(&instructions)
        .map_err(|error| unavailable(format!("{INSTRUCTIONS} is unreadable: {error}")))?;
    let mut text = String::with_capacity(body.len());
    let mut sources = vec![
        json!({"path": format!("home/{INSTRUCTIONS}"), "revision": revision(body.as_bytes())}),
    ];
    for line in body.split_inclusive('\n') {
        // The line stays verbatim (list marker and description included); the
        // referenced bytes follow it inside provenance markers.
        text.push_str(line);
        let Some(reference) = line_reference(line) else {
            continue;
        };
        let reference = reference.as_str();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        let target = fs::canonicalize(home_root.join(reference)).map_err(|error| {
            unavailable(format!(
                "required reference @{reference} is unreadable: {error}"
            ))
        })?;
        let label = if let Ok(relative) = target.strip_prefix(&home_root) {
            format!("home/{}", slash(relative))
        } else if let Some(relative) = house_root
            .as_ref()
            .and_then(|house| target.strip_prefix(house).ok())
        {
            format!("{HOUSE_ROOT}/{}", slash(relative))
        } else {
            return Err(unavailable(format!(
                "reference @{reference} leaves the home and house roots"
            )));
        };
        // Inside a root, the same resource law applies: no dot segment (so no
        // .omp/runtime secrets) and text documents only.
        let relative = label.split_once('/').map_or("", |(_, rest)| rest);
        let text_document = Path::new(relative)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                TEXT_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
            });
        if relative
            .split('/')
            .any(|segment| segment.is_empty() || segment.starts_with('.'))
            || !text_document
        {
            return Err(unavailable(format!(
                "reference @{reference} names a hidden or non-text path"
            )));
        }
        if !target.is_file() {
            return Err(unavailable(format!("reference @{reference} is not a file")));
        }
        let bytes = fs::read(&target).map_err(|error| {
            unavailable(format!(
                "required reference @{reference} is unreadable: {error}"
            ))
        })?;
        let included = String::from_utf8(bytes)
            .map_err(|_| unavailable(format!("reference @{reference} is not UTF-8")))?;
        let digest = revision(included.as_bytes());
        text.push_str(&format!("<!-- athanor:source {label} {digest} -->\n"));
        text.push_str(&included);
        if !included.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("<!-- athanor:end {label} -->\n"));
        sources.push(json!({"path": label, "revision": digest}));
        if text.len() > MAX_CONTEXT_BYTES {
            return Err(unavailable(format!(
                "identity context exceeds {MAX_CONTEXT_BYTES} bytes"
            )));
        }
    }
    Ok(Context {
        revision: revision(text.as_bytes()),
        text,
        sources,
    })
}

/// A required reference is a line that leads with an `@` mention, optionally
/// as a list item: `@a.md`, `- @a.md — why`, `1. @"b c.md" ...`. The mention
/// grammar and punctuation trimming follow OMP's own file-mention parser
/// (`utils/file-mentions.ts`): `@"…"`, `@'…'`, or `@token` up to whitespace.
/// Mid-sentence mentions stay prose.
fn line_reference(line: &str) -> Option<String> {
    let mut rest = line.trim_start();
    for marker in ["- ", "* ", "+ "] {
        if let Some(after) = rest.strip_prefix(marker) {
            rest = after.trim_start();
            break;
        }
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(after) = rest[digits..]
            .strip_prefix(". ")
            .or_else(|| rest[digits..].strip_prefix(") "))
    {
        rest = after.trim_start();
    }
    let mention = rest.strip_prefix('@')?;
    let raw = match mention.chars().next()? {
        quote @ ('"' | '\'') => {
            let inner = &mention[1..];
            &inner[..inner.find(quote)?]
        }
        _ => mention
            .split(|c: char| c.is_whitespace() || c == '@')
            .next()?,
    };
    let cleaned = raw
        .trim()
        .trim_start_matches(['`', '"', '\'', '(', '[', '{', '<'])
        .trim_end_matches([
            ')', ']', '}', '>', '.', ',', ';', ':', '!', '?', '"', '\'', '`',
        ])
        .trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

fn slash(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Lexical law first: a relative, '/'-separated path with no dot segment.
fn segments(path: &str) -> Result<Vec<&str>, Refusal> {
    if path.is_empty() || path.len() > 1024 {
        return Err(invalid("path must be 1..1024 bytes"));
    }
    if path.starts_with('/') || path.contains('\\') || path.contains(':') || path.contains('\0') {
        return Err(invalid(
            "path must be relative to the home with '/' separators",
        ));
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || part.starts_with('.'))
    {
        return Err(invalid("path segments may not be empty or start with '.'"));
    }
    Ok(parts)
}

fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

/// Walk the path one component at a time, refusing any link on the way.
/// Returns the joined path and whether its final component exists.
fn resolve(home: &Path, path: &str, file: bool) -> Result<(PathBuf, bool), Refusal> {
    let parts = segments(path)?;
    if file {
        let extension = Path::new(parts[parts.len() - 1])
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        if !extension.is_some_and(|extension| TEXT_EXTENSIONS.contains(&extension.as_str())) {
            return Err(invalid(format!(
                "only text documents are reachable ({})",
                TEXT_EXTENSIONS.join(", ")
            )));
        }
    }
    let home_metadata = fs::symlink_metadata(home)
        .map_err(|error| unavailable(format!("home is unreadable: {error}")))?;
    if reparse(&home_metadata) {
        return Err(unavailable("home itself is a link".into()));
    }
    let mut current = home.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if reparse(&metadata) => {
                return Err(invalid("links are not followed inside the home"));
            }
            Ok(metadata) => {
                let last = index + 1 == parts.len();
                if !last && !metadata.is_dir() {
                    return Err(invalid("a parent segment is not a directory"));
                }
                if last && file && !metadata.is_file() {
                    return Err(invalid("path is not a file"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok((home.join(parts.join("/")), false));
            }
            Err(error) => return Err(Refusal::internal(format!("path unreadable: {error}"))),
        }
    }
    Ok((current, true))
}

pub(super) fn read(home: &Path, path: &str) -> Result<Value, Refusal> {
    let (target, exists) = resolve(home, path, true)?;
    if !exists {
        return Err(Refusal::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{path} does not exist"),
        ));
    }
    let bytes =
        fs::read(&target).map_err(|error| Refusal::internal(format!("read failed: {error}")))?;
    if bytes.len() > MAX_RESOURCE_BYTES {
        return Err(Refusal::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "document exceeds 1 MiB",
        ));
    }
    let revision = revision(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| invalid("document is not UTF-8"))?;
    Ok(json!({"path": path, "revision": revision, "text": text}))
}

fn current_revision(target: &Path, exists: bool) -> Result<Option<String>, Refusal> {
    if !exists {
        return Ok(None);
    }
    fs::read(target)
        .map(|bytes| Some(revision(&bytes)))
        .map_err(|error| Refusal::internal(format!("read failed: {error}")))
}

/// Compare-and-write. The caller serializes writes for this home.
pub(super) fn write(
    home: &Path,
    path: &str,
    text: &str,
    expected: Option<&str>,
) -> Result<Value, Refusal> {
    if text.len() > MAX_RESOURCE_BYTES {
        return Err(Refusal::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "document exceeds 1 MiB",
        ));
    }
    let (target, exists) = resolve(home, path, true)?;
    let current = current_revision(&target, exists)?;
    if current.as_deref() != expected {
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            format!("{path} changed since the expected revision"),
        )
        .with("currentRevision", json!(current)));
    }
    if let Some(parent) = target.parent() {
        // Every existing parent was already walked link-free; only new ones are made.
        fs::create_dir_all(parent)
            .map_err(|error| Refusal::internal(format!("parent could not be made: {error}")))?;
    }
    let overwrite = if exists {
        OverwriteBehavior::AllowOverwrite
    } else {
        OverwriteBehavior::DisallowOverwrite
    };
    AtomicFile::new(&target, overwrite)
        .write(|file| file.write_all(text.as_bytes()))
        .map_err(|error| match error {
            atomicwrites::Error::Internal(error)
                if error.kind() == io::ErrorKind::AlreadyExists =>
            {
                Refusal::new(
                    StatusCode::CONFLICT,
                    "revision_conflict",
                    format!("{path} appeared concurrently"),
                )
                .with("currentRevision", Value::Null)
            }
            other => Refusal::internal(format!("write failed: {other}")),
        })?;
    Ok(json!({"path": path, "revision": revision(text.as_bytes())}))
}

pub(super) fn list(home: &Path, prefix: Option<&str>) -> Result<Value, Refusal> {
    let start = match prefix {
        Some(prefix) => {
            let (dir, exists) = resolve(home, prefix, false)?;
            if !exists || !dir.is_dir() {
                return Err(Refusal::new(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    format!("{prefix} is not a directory"),
                ));
            }
            dir
        }
        None => home.to_path_buf(),
    };
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut pending = vec![start];
    while let Some(dir) = pending.pop() {
        let mut children: Vec<_> = fs::read_dir(&dir)
            .map_err(|error| Refusal::internal(format!("list failed: {error}")))?
            .filter_map(Result::ok)
            .collect();
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let name = child.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(child.path()) else {
                continue;
            };
            if reparse(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                pending.push(child.path());
                continue;
            }
            let extension = Path::new(name)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase);
            if !metadata.is_file()
                || !extension.is_some_and(|extension| TEXT_EXTENSIONS.contains(&extension.as_str()))
            {
                continue;
            }
            if entries.len() == MAX_LIST_ENTRIES {
                truncated = true;
                break;
            }
            let Ok(relative) = child.path().strip_prefix(home).map(slash) else {
                continue;
            };
            let Ok(bytes) = fs::read(child.path()) else {
                continue;
            };
            let modified = metadata
                .modified()
                .ok()
                .map(|at| chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339());
            entries.push(json!({"path": relative, "revision": revision(&bytes), "bytes": bytes.len(), "modifiedAt": modified}));
        }
        if truncated {
            break;
        }
    }
    Ok(json!({"entries": entries, "truncated": truncated}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("athanor-home-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("kintsu")).unwrap();
        fs::create_dir_all(root.join("house")).unwrap();
        root
    }

    #[test]
    fn context_expands_home_and_house_references_with_provenance() {
        let root = fixture();
        let home = root.join("kintsu");
        fs::write(home.join("active_spirit.md"), "I am Kintsu.\n").unwrap();
        fs::write(root.join("house/ORGANS.md"), "Organs.\n").unwrap();
        fs::write(
            home.join(INSTRUCTIONS),
            "# Kintsu\n@active_spirit.md\n@../house/ORGANS.md\ntail\n",
        )
        .unwrap();
        let context = context(&home).unwrap();
        assert!(
            context
                .text
                .contains("<!-- athanor:source home/active_spirit.md sha256:")
        );
        assert!(
            context
                .text
                .contains("I am Kintsu.\n<!-- athanor:end home/active_spirit.md -->")
        );
        assert!(context.text.contains("<!-- athanor:source house/ORGANS.md"));
        assert!(context.text.ends_with("tail\n"));
        assert_eq!(context.sources.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn context_expands_list_references_with_descriptions_like_the_real_home() {
        let root = fixture();
        let home = root.join("kintsu");
        fs::write(home.join("active_spirit.md"), "HOME_MARKER\n").unwrap();
        fs::write(home.join("agreement.md"), "AGREEMENT_MARKER\n").unwrap();
        fs::write(home.join("with space.md"), "QUOTED_MARKER\n").unwrap();
        fs::write(root.join("house/ORGANS.md"), "SHARED_MARKER\n").unwrap();
        fs::write(
            home.join(INSTRUCTIONS),
            "# Kintsu\n\n- @active_spirit.md — who I am right now\n- @agreement.md — the hand agreement\n* @\"with space.md\" (quoted)\n1. @../house/ORGANS.md — the shared organs\nMention @nowhere.md mid-sentence stays prose.\n",
        )
        .unwrap();
        let context = context(&home).unwrap();
        for marker in [
            "HOME_MARKER",
            "AGREEMENT_MARKER",
            "QUOTED_MARKER",
            "SHARED_MARKER",
        ] {
            assert!(
                context.text.contains(marker),
                "{marker} missing from {}",
                context.text
            );
        }
        assert!(context.text.contains(
            "- @agreement.md — the hand agreement\n<!-- athanor:source home/agreement.md sha256:"
        ));
        assert!(context.text.contains("<!-- athanor:source house/ORGANS.md"));
        assert!(
            context
                .text
                .contains("Mention @nowhere.md mid-sentence stays prose.\n")
        );
        assert_eq!(context.sources.len(), 5);
        fs::write(home.join(INSTRUCTIONS), "- @missing.md — required\n").unwrap();
        assert_eq!(
            super::context(&home).err().unwrap().code,
            "home_unavailable"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn context_refuses_missing_and_escaping_references() {
        let root = fixture();
        let home = root.join("kintsu");
        fs::write(home.join(INSTRUCTIONS), "@missing.md\n").unwrap();
        assert_eq!(context(&home).err().unwrap().code, "home_unavailable");
        fs::write(root.join("secret.md"), "outside").unwrap();
        fs::write(home.join(INSTRUCTIONS), "@../secret.md\n").unwrap();
        assert_eq!(context(&home).err().unwrap().code, "home_unavailable");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resources_refuse_traversal_dot_segments_and_stale_revisions() {
        let root = fixture();
        let home = root.join("kintsu");
        for path in [
            "../x.md",
            "/x.md",
            "a\\b.md",
            ".omp/runtime/restart-capability.md",
            "a//b.md",
            "C:x.md",
            "x.exe",
        ] {
            assert_eq!(
                read(&home, path).err().unwrap().code,
                "path_invalid",
                "{path}"
            );
        }
        let created = write(&home, "notes/a.md", "one", None).unwrap();
        let first = created["revision"].as_str().unwrap().to_owned();
        let conflict = write(&home, "notes/a.md", "again", None).err().unwrap();
        assert_eq!(conflict.code, "revision_conflict");
        let updated = write(&home, "notes/a.md", "two", Some(&first)).unwrap();
        assert_eq!(
            write(&home, "notes/a.md", "three", Some(&first))
                .err()
                .unwrap()
                .code,
            "revision_conflict"
        );
        assert_eq!(
            read(&home, "notes/a.md").unwrap()["revision"],
            updated["revision"]
        );
        let listed = list(&home, None).unwrap();
        assert_eq!(listed["entries"][0]["path"], "notes/a.md");
        fs::remove_dir_all(root).unwrap();
    }
}
