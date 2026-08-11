//! Database-free, file-authoritative Vault retrieval.
//!
//! Every index is derived in memory for one request. Configured files remain the
//! authority; this crate neither opens a database nor writes into a Vault.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

const ROOM_MARKER: &str = ".solarisael-room.json";
const DEFAULT_MAX_FILE_BYTES: u64 = 512 * 1024;
const DEFAULT_MAX_FILES: usize = 5_000;
const MAX_RESULTS: usize = 8;
const CHUNK_CHARS: usize = 6_000;
const CHUNK_OVERLAP: usize = 400;
const EXCERPT_CHARS: usize = 900;

const ELIGIBLE_EXTENSIONS: &[&str] = &["md", "markdown", "json", "jsonl", "txt"];
const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".idea",
    ".next",
    ".nuxt",
    ".turbo",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "coverage",
    "target",
    "out",
];
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "do", "for", "from", "how", "i", "in",
    "is", "it", "of", "on", "or", "that", "the", "this", "to", "was", "we", "what", "when",
    "where", "which", "with", "you", "ao", "aos", "com", "como", "da", "das", "de", "do", "dos",
    "e", "em", "eu", "na", "nas", "no", "nos", "o", "os", "ou", "para", "por", "que", "se", "um",
    "uma",
];

#[derive(Clone, Copy)]
enum Field {
    Path,
    Title,
    Heading,
    Keys,
    Tags,
    Body,
    Metadata,
}
const FIELDS: [Field; 7] = [
    Field::Path,
    Field::Title,
    Field::Heading,
    Field::Keys,
    Field::Tags,
    Field::Body,
    Field::Metadata,
];
impl Field {
    fn index(self) -> usize {
        self as usize
    }
    fn name(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Keys => "keys",
            Self::Tags => "tags",
            Self::Body => "body",
            Self::Metadata => "metadata",
        }
    }
    fn weight(self) -> f64 {
        match self {
            Self::Path => 4.2,
            Self::Title => 3.8,
            Self::Heading => 3.4,
            Self::Keys => 2.6,
            Self::Tags => 2.8,
            Self::Body => 1.0,
            Self::Metadata => 1.4,
        }
    }
    fn length_normalization(self) -> f64 {
        match self {
            Self::Path => 0.2,
            Self::Title => 0.25,
            Self::Heading => 0.3,
            Self::Keys => 0.45,
            Self::Tags => 0.3,
            Self::Body => 0.75,
            Self::Metadata => 0.5,
        }
    }
}

#[derive(Debug)]
pub enum VaultError {
    EmptyQuery,
    InvalidRoomDirectory(String),
    RoomMismatch { requested: String, actual: String },
}
impl VaultError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyQuery => "empty_query",
            Self::InvalidRoomDirectory(_) => "invalid_room_directory",
            Self::RoomMismatch { .. } => "room_mismatch",
        }
    }
}
impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyQuery => f.write_str("empty query"),
            Self::InvalidRoomDirectory(message) => f.write_str(message),
            Self::RoomMismatch { requested, actual } => {
                write!(f, "room name/path mismatch: {requested} != {actual}")
            }
        }
    }
}
impl std::error::Error for VaultError {}

#[derive(Clone, Debug)]
pub struct VaultRecallRequest {
    pub room_dir: PathBuf,
    pub room: String,
    pub query: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct VaultCandidate {
    pub source_path: String,
    pub title: String,
    pub heading_path: String,
    pub sources: Vec<String>,
    pub score: f64,
    pub term_coverage: f64,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub reasons: Vec<String>,
    pub excerpt: String,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultTaxonomy {
    pub memory_types: Vec<String>,
    pub thread_keys: Vec<String>,
    pub named_entities: Vec<String>,
    pub file_types: Vec<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultRecallResult {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: String,
    pub authority: String,
    pub roots: Vec<String>,
    pub scanned_files: usize,
    pub indexed_documents: usize,
    pub retrieval_candidates: Vec<VaultCandidate>,
    pub canon_matches: Vec<Value>,
    pub semantic_chunks: Vec<Value>,
    pub content_chunks: Vec<Value>,
    pub date_matches: Vec<Value>,
    pub query_dates: Vec<String>,
    pub taxonomy: VaultTaxonomy,
    pub warnings: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomMarker {
    #[serde(default)]
    vault_roots: Vec<String>,
    #[serde(default)]
    vault_ignore: Vec<String>,
    vault_max_file_bytes: Option<u64>,
    vault_max_files: Option<usize>,
}
struct VaultConfig {
    roots: Vec<PathBuf>,
    ignore: Vec<String>,
    max_file_bytes: u64,
    max_files: usize,
}
struct VaultDocument {
    source_path: String,
    title: String,
    heading_path: String,
    body: String,
    fields: [String; 7],
    terms: [HashMap<String, usize>; 7],
    lengths: [usize; 7],
}
struct VaultIndex {
    roots: Vec<String>,
    documents: Vec<VaultDocument>,
    scanned_files: usize,
    warnings: Vec<String>,
}
struct IgnoreRule {
    negated: bool,
    directory_only: bool,
    regex: Regex,
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
fn normalized_text(value: &str) -> String {
    value
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect()
}
fn token_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-')
}
fn trim_token(value: &str) -> &str {
    value.trim_matches(|character| matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-'))
}
fn tokens(value: &str) -> Vec<String> {
    let normalized = normalized_text(value);
    let mut result = Vec::new();
    for raw in normalized.split(|character| !token_character(character)) {
        let token = trim_token(raw);
        if token.is_empty() {
            continue;
        }
        result.push(token.to_owned());
        for part in token.split(['_', ':', '+', '#', '.', '/', '-']) {
            if !part.is_empty() && part != token {
                result.push(part.to_owned());
            }
        }
    }
    result
}
fn term_frequency(value: &str) -> HashMap<String, usize> {
    let mut frequencies = HashMap::new();
    for term in tokens(value) {
        *frequencies.entry(term).or_insert(0) += 1;
    }
    frequencies
}
fn bounded_u64(value: Option<u64>, fallback: u64, minimum: u64, maximum: u64) -> u64 {
    value
        .filter(|value| (*value >= minimum) && (*value <= maximum))
        .unwrap_or(fallback)
}
fn bounded_usize(value: Option<usize>, fallback: usize, minimum: usize, maximum: usize) -> usize {
    value
        .filter(|value| (*value >= minimum) && (*value <= maximum))
        .unwrap_or(fallback)
}

fn normalize_lexical(path: &Path) -> PathBuf {
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
fn load_config(room_dir: &Path) -> Result<VaultConfig, VaultError> {
    let metadata = fs::symlink_metadata(room_dir).map_err(|error| {
        VaultError::InvalidRoomDirectory(format!(
            "Vault room unavailable: {} ({error})",
            normalized_path(room_dir)
        ))
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(VaultError::InvalidRoomDirectory(format!(
            "Vault room is not a direct readable directory: {}",
            normalized_path(room_dir)
        )));
    }
    let marker = fs::read_to_string(room_dir.join(ROOM_MARKER))
        .ok()
        .and_then(|body| serde_json::from_str::<RoomMarker>(&body).ok())
        .unwrap_or_default();
    let configured = if marker
        .vault_roots
        .iter()
        .any(|root| !root.trim().is_empty())
    {
        marker.vault_roots
    } else {
        vec![".".into()]
    };
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for root in configured {
        let trimmed = root.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
        } else {
            room_dir.join(trimmed)
        };
        let candidate = normalize_lexical(&candidate);
        if seen.insert(normalized_path(&candidate)) {
            roots.push(candidate);
        }
    }
    Ok(VaultConfig {
        roots,
        ignore: marker
            .vault_ignore
            .into_iter()
            .filter(|rule| !rule.trim().is_empty())
            .collect(),
        max_file_bytes: bounded_u64(
            marker.vault_max_file_bytes,
            DEFAULT_MAX_FILE_BYTES,
            16 * 1024,
            4 * 1024 * 1024,
        ),
        max_files: bounded_usize(marker.vault_max_files, DEFAULT_MAX_FILES, 1, 50_000),
    })
}

fn glob_regex(pattern: &str) -> Option<Regex> {
    let normalized = pattern.trim().replace('\\', "/");
    let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
    let anchored = normalized.starts_with('/');
    let body = normalized.strip_prefix('/').unwrap_or(normalized);
    let mut source = String::new();
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '*' if characters.peek() == Some(&'*') => {
                characters.next();
                source.push_str(".*");
            }
            '*' => source.push_str("[^/]*"),
            '?' => source.push_str("[^/]"),
            other => source.push_str(&regex::escape(&other.to_string())),
        }
    }
    Regex::new(&if anchored {
        format!(r"(?i)^{source}(?:/|$)")
    } else {
        format!(r"(?i)(?:^|/){source}(?:/|$)")
    })
    .ok()
}
fn parse_ignore_rules(lines: impl IntoIterator<Item = String>) -> Vec<IgnoreRule> {
    let mut rules = Vec::new();
    for raw in lines {
        let mut value = raw.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        let negated = value.starts_with('!');
        if negated {
            value = &value[1..];
        }
        let directory_only = value.ends_with('/');
        if directory_only {
            value = &value[..value.len() - 1];
        }
        if value.is_empty() {
            continue;
        }
        if let Some(regex) = glob_regex(value) {
            rules.push(IgnoreRule {
                negated,
                directory_only,
                regex,
            });
        }
    }
    rules
}
fn root_ignore_rules(root: &Path, configured: &[String]) -> Vec<IgnoreRule> {
    let lines = fs::read_to_string(root.join(".gitignore"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .chain(configured.iter().cloned())
        .collect::<Vec<_>>();
    parse_ignore_rules(lines)
}
fn ignored_by_rules(relative_path: &str, is_directory: bool, rules: &[IgnoreRule]) -> bool {
    let mut ignored = false;
    for rule in rules {
        if rule.directory_only && !is_directory {
            continue;
        }
        if rule.regex.is_match(relative_path) {
            ignored = !rule.negated;
        }
    }
    ignored
}
fn ignored_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower == ".env" || lower.starts_with(".env.") {
        return true;
    }
    if [".pem", ".key", ".p12", ".pfx", ".keystore"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        return true;
    }
    if [
        "package-lock.json",
        "bun.lock",
        "bun.lockb",
        "pnpm-lock.yaml",
        "yarn.lock",
    ]
    .contains(&lower.as_str())
    {
        return true;
    }
    lower
        .split(['-', '_', '.'])
        .any(|part| matches!(part, "secret" | "secrets" | "credential" | "credentials"))
}
fn eligible_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| ELIGIBLE_EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}

fn collect_files(config: &VaultConfig) -> (Vec<(PathBuf, PathBuf)>, Vec<String>) {
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

fn make_document(
    source_path: &str,
    path_text: &str,
    title: &str,
    heading_path: &str,
    body: String,
    keys: &str,
    tags: &str,
    metadata: &str,
) -> VaultDocument {
    let fields = [
        path_text.to_owned(),
        title.to_owned(),
        heading_path.to_owned(),
        keys.to_owned(),
        tags.to_owned(),
        body.clone(),
        metadata.to_owned(),
    ];
    let terms = std::array::from_fn(|index| term_frequency(&fields[index]));
    let lengths = std::array::from_fn(|index| terms[index].values().sum());
    VaultDocument {
        source_path: source_path.to_owned(),
        title: title.to_owned(),
        heading_path: heading_path.to_owned(),
        body,
        fields,
        terms,
        lengths,
    }
}
fn split_body(body: &str) -> Vec<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.chars().count() <= CHUNK_CHARS {
        return vec![trimmed.to_owned()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    loop {
        let end = trimmed[start..]
            .char_indices()
            .nth(CHUNK_CHARS)
            .map_or(trimmed.len(), |(offset, _)| start + offset);
        let chunk = trimmed[start..end].trim();
        if !chunk.is_empty() {
            chunks.push(chunk.to_owned());
        }
        if end == trimmed.len() {
            break;
        }
        start = trimmed[..end]
            .char_indices()
            .rev()
            .nth(CHUNK_OVERLAP.saturating_sub(1))
            .map_or(end, |(offset, _)| offset);
    }
    chunks
}
fn markdown_documents(source_path: &str, path_text: &str, content: &str) -> Vec<VaultDocument> {
    let title = Path::new(source_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let normalized = content.replace("\r\n", "\n");
    let mut documents = Vec::new();
    let mut body = normalized.as_str();
    if let Some(frontmatter) = body.strip_prefix("---\n") {
        if let Some(end) = frontmatter.find("\n---\n") {
            let value = &frontmatter[..end];
            let keys = value
                .lines()
                .filter_map(|line| line.split_once(':').map(|(key, _)| key))
                .collect::<Vec<_>>()
                .join(" ");
            documents.push(make_document(
                source_path,
                path_text,
                title,
                "__frontmatter__",
                value.to_owned(),
                &keys,
                "",
                "",
            ));
            body = &frontmatter[end + 5..];
        }
    }
    let mut headings: Vec<String> = Vec::new();
    let mut current_heading = "__preamble__".to_owned();
    let mut current = Vec::new();
    let flush = |documents: &mut Vec<VaultDocument>,
                 current: &mut Vec<&str>,
                 headings: &[String],
                 current_heading: &str| {
        let heading_path = if current_heading == "__preamble__" {
            current_heading.to_owned()
        } else {
            headings
                .iter()
                .filter(|value| !value.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" > ")
        };
        for (index, chunk) in split_body(&current.join("\n")).into_iter().enumerate() {
            let heading = if index == 0 {
                heading_path.clone()
            } else {
                format!("{heading_path} [{}]", index + 1)
            };
            documents.push(make_document(
                source_path,
                path_text,
                title,
                &heading,
                chunk,
                "",
                "",
                "",
            ));
        }
        current.clear();
    };
    for line in body.lines() {
        let hashes = line
            .chars()
            .take_while(|character| *character == '#')
            .count();
        let heading = if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
            Some(line[hashes + 1..].trim())
        } else {
            None
        };
        if let Some(heading) = heading {
            flush(&mut documents, &mut current, &headings, &current_heading);
            headings.resize(hashes, String::new());
            headings[hashes - 1] = heading.to_owned();
            current_heading = heading.to_owned();
        } else {
            current.push(line);
        }
    }
    flush(&mut documents, &mut current, &headings, &current_heading);
    documents
}
fn escaped_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
fn flatten_json(value: &Value, pointer: &str) -> (String, String) {
    fn visit(value: &Value, pointer: &str, keys: &mut Vec<String>, values: &mut Vec<String>) {
        match value {
            Value::Null => values.push(format!("{pointer}: null")),
            Value::Bool(value) => values.push(format!("{pointer}: {value}")),
            Value::Number(value) => values.push(format!("{pointer}: {value}")),
            Value::String(value) => values.push(format!("{pointer}: {value}")),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    visit(item, &format!("{pointer}/{index}"), keys, values);
                }
            }
            Value::Object(object) => {
                for (key, child) in object {
                    keys.push(key.clone());
                    visit(
                        child,
                        &format!("{pointer}/{}", escaped_pointer(key)),
                        keys,
                        values,
                    );
                }
            }
        }
    }
    let mut keys = Vec::new();
    let mut values = Vec::new();
    visit(value, pointer, &mut keys, &mut values);
    (keys.join(" "), values.join("\n"))
}
fn json_documents(
    source_path: &str,
    path_text: &str,
    value: &Value,
    heading_prefix: &str,
) -> Vec<VaultDocument> {
    let title = Path::new(source_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let records: Vec<(String, &Value)> = match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("{heading_prefix}/{index}"), item))
            .collect(),
        Value::Object(object) => object
            .iter()
            .map(|(key, item)| (format!("{heading_prefix}/{}", escaped_pointer(key)), item))
            .collect(),
        _ => vec![(
            if heading_prefix.is_empty() {
                "$".into()
            } else {
                heading_prefix.into()
            },
            value,
        )],
    };
    records
        .into_iter()
        .flat_map(|(heading, record)| {
            let (keys, body) =
                flatten_json(record, if heading.is_empty() { "$" } else { &heading });
            split_body(&body)
                .into_iter()
                .enumerate()
                .map(|(index, body)| {
                    let chunk_heading = if index == 0 {
                        heading.clone()
                    } else {
                        format!("{heading} [{}]", index + 1)
                    };
                    make_document(
                        source_path,
                        path_text,
                        title,
                        &chunk_heading,
                        body,
                        &keys,
                        "",
                        &heading,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
fn parse_file(
    root: &Path,
    absolute: &Path,
    max_file_bytes: u64,
    warnings: &mut Vec<String>,
) -> Vec<VaultDocument> {
    let source_path = normalized_path(absolute);
    let metadata = match fs::metadata(absolute) {
        Ok(metadata) => metadata,
        Err(_) => {
            warnings.push(format!("{source_path}: skipped unreadable text file."));
            return Vec::new();
        }
    };
    if metadata.len() > max_file_bytes {
        warnings.push(format!(
            "{source_path}: skipped file larger than {max_file_bytes} bytes."
        ));
        return Vec::new();
    }
    let content = match fs::read_to_string(absolute) {
        Ok(content) => content,
        Err(_) => {
            warnings.push(format!("{source_path}: skipped unreadable text file."));
            return Vec::new();
        }
    };
    let path_text = absolute
        .strip_prefix(root)
        .map(normalized_path)
        .unwrap_or_else(|_| source_path.clone());
    let extension = absolute
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "md" | "markdown" => markdown_documents(&source_path, &path_text, &content),
        "json" => match serde_json::from_str(&content) {
            Ok(value) => json_documents(&source_path, &path_text, &value, ""),
            Err(_) => {
                warnings.push(format!("{source_path}: skipped malformed JSON."));
                Vec::new()
            }
        },
        "jsonl" => {
            let mut documents = Vec::new();
            let mut malformed = 0;
            for (index, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str(line) {
                    Ok(value) => documents.extend(json_documents(
                        &source_path,
                        &path_text,
                        &value,
                        &format!("line:{}", index + 1),
                    )),
                    Err(_) => malformed += 1,
                }
            }
            if malformed > 0 {
                warnings.push(format!(
                    "{source_path}: skipped {malformed} malformed JSONL record{}.",
                    if malformed == 1 { "" } else { "s" }
                ));
            }
            documents
        }
        _ => {
            let title = absolute
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            split_body(&content)
                .into_iter()
                .enumerate()
                .map(|(index, body)| {
                    let heading = if index == 0 {
                        "__document__".into()
                    } else {
                        format!("__document__ [{}]", index + 1)
                    };
                    make_document(&source_path, &path_text, title, &heading, body, "", "", "")
                })
                .collect()
        }
    }
}
fn build_index(config: &VaultConfig) -> VaultIndex {
    let (files, mut warnings) = collect_files(config);
    let scanned_files = files.len();
    let mut documents = Vec::new();
    for (root, absolute) in files {
        documents.extend(parse_file(
            &root,
            &absolute,
            config.max_file_bytes,
            &mut warnings,
        ));
    }
    VaultIndex {
        roots: config
            .roots
            .iter()
            .map(|root| normalized_path(root))
            .collect(),
        documents,
        scanned_files,
        warnings,
    }
}

fn query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let all = tokens(query)
        .into_iter()
        .filter(|term| seen.insert(term.clone()))
        .collect::<Vec<_>>();
    let meaningful = all
        .iter()
        .filter(|term| term.len() > 1 && !STOPWORDS.contains(&term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if meaningful.is_empty() {
        all
    } else {
        meaningful
    }
}
fn quoted_terms(query: &str) -> Vec<String> {
    let quote = Regex::new(r#"[\"“”]([^\"“”]+)[\"“”]"#).expect("static quote regex");
    quote
        .captures_iter(query)
        .filter_map(|capture| capture.get(1).map(|value| normalized_text(value.as_str())))
        .filter(|value| !value.is_empty())
        .collect()
}
fn excerpt(document: &VaultDocument, terms: &[String]) -> String {
    let body = document
        .body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if body.chars().count() <= EXCERPT_CHARS {
        return body;
    }
    let lower = normalized_text(&body);
    let position = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .unwrap_or(0);
    let desired_start = position.saturating_sub(EXCERPT_CHARS / 3);
    let mut start = desired_start.min(body.len());
    while start > 0 && !body.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = start;
    for (offset, character) in body[start..].char_indices().take(EXCERPT_CHARS) {
        end = start + offset + character.len_utf8();
    }
    let clipped = body[start..end].trim();
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        clipped,
        if end < body.len() { "…" } else { "" }
    )
}
fn rank(index: &VaultIndex, query: &str) -> Vec<VaultCandidate> {
    let terms = query_terms(query);
    if terms.is_empty() || index.documents.is_empty() {
        return Vec::new();
    }
    let compound_terms = terms
        .iter()
        .filter(|term| {
            term.len() >= 4
                && term
                    .chars()
                    .any(|character| matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-'))
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut averages = [1.0; 7];
    for field in FIELDS {
        averages[field.index()] = (index
            .documents
            .iter()
            .map(|document| document.lengths[field.index()])
            .sum::<usize>() as f64
            / index.documents.len() as f64)
            .max(1.0);
    }
    let frequencies = terms
        .iter()
        .map(|term| {
            let count = index
                .documents
                .iter()
                .filter(|document| document.terms.iter().any(|field| field.contains_key(term)))
                .count();
            (term.clone(), count)
        })
        .collect::<HashMap<_, _>>();
    let exact_phrases = std::iter::once(normalized_text(query).trim().to_owned())
        .chain(quoted_terms(query))
        .filter(|term| term.chars().count() >= 3)
        .collect::<BTreeSet<_>>();
    let total = index.documents.len() as f64;
    let mut ranked = Vec::new();
    for document in &index.documents {
        let mut score = 0.0;
        let mut matched_terms = Vec::new();
        let mut matched_fields = BTreeSet::new();
        for term in &terms {
            let mut combined_tf = 0.0;
            for field in FIELDS {
                let tf = *document.terms[field.index()].get(term).unwrap_or(&0) as f64;
                if tf <= 0.0 {
                    continue;
                }
                matched_fields.insert(field.index());
                let b = field.length_normalization();
                combined_tf += field.weight() * tf
                    / (1.0 - b
                        + b * document.lengths[field.index()] as f64 / averages[field.index()]);
            }
            if combined_tf <= 0.0 {
                continue;
            }
            matched_terms.push(term.clone());
            let frequency = *frequencies.get(term).unwrap_or(&0) as f64;
            score += (1.0 + (total - frequency + 0.5) / (frequency + 0.5)).ln()
                * (2.2 * combined_tf)
                / (1.2 + combined_tf);
        }
        let mut exact_fields = BTreeSet::new();
        for phrase in &exact_phrases {
            for field in FIELDS {
                if normalized_text(&document.fields[field.index()]).contains(phrase) {
                    exact_fields.insert(field.index());
                    score += field.weight()
                        * if matches!(field, Field::Body) {
                            1.5
                        } else {
                            2.25
                        };
                }
            }
        }
        if score <= 0.0
            || (!compound_terms.is_empty()
                && !compound_terms
                    .iter()
                    .any(|term| matched_terms.contains(term)))
        {
            continue;
        }
        let reasons = [
            (!matched_fields.is_empty()).then(|| {
                format!(
                    "BM25F fields: {}",
                    matched_fields
                        .iter()
                        .map(|index| FIELDS[*index].name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }),
            (!exact_fields.is_empty()).then(|| {
                format!(
                    "exact content fields: {}",
                    exact_fields
                        .iter()
                        .map(|index| FIELDS[*index].name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        let missing_terms = terms
            .iter()
            .filter(|term| !matched_terms.contains(term))
            .cloned()
            .collect();
        ranked.push((score, document, matched_terms, missing_terms, reasons));
    }
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.source_path.cmp(&right.1.source_path))
            .then_with(|| left.1.heading_path.cmp(&right.1.heading_path))
    });
    ranked
        .into_iter()
        .take(MAX_RESULTS)
        .map(
            |(score, document, matched_terms, missing_terms, reasons)| VaultCandidate {
                source_path: document.source_path.clone(),
                title: document.title.clone(),
                heading_path: document.heading_path.clone(),
                sources: vec![document.source_path.clone()],
                score,
                term_coverage: matched_terms.len() as f64 / terms.len().max(1) as f64,
                excerpt: excerpt(document, &matched_terms),
                matched_terms,
                missing_terms,
                reasons,
            },
        )
        .collect()
}

pub fn recall(request: VaultRecallRequest) -> Result<VaultRecallResult, VaultError> {
    if request.query.trim().is_empty() {
        return Err(VaultError::EmptyQuery);
    }
    if !request.room_dir.is_absolute() {
        return Err(VaultError::InvalidRoomDirectory(
            "Vault room directory must be absolute".into(),
        ));
    }
    let room_dir = normalize_lexical(&request.room_dir);
    let actual_room = room_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if actual_room != request.room {
        return Err(VaultError::RoomMismatch {
            requested: request.room,
            actual: actual_room.to_owned(),
        });
    }
    let config = load_config(&room_dir)?;
    let index = build_index(&config);
    let retrieval_candidates = rank(&index, &request.query);
    Ok(VaultRecallResult {
        ok: true,
        query: request.query,
        found: !retrieval_candidates.is_empty(),
        source: "vault-files".into(),
        authority: "vault-files".into(),
        roots: index.roots,
        scanned_files: index.scanned_files,
        indexed_documents: index.documents.len(),
        retrieval_candidates,
        canon_matches: Vec::new(),
        semantic_chunks: Vec::new(),
        content_chunks: Vec::new(),
        date_matches: Vec::new(),
        query_dates: Vec::new(),
        taxonomy: VaultTaxonomy {
            memory_types: vec!["vault-file".into()],
            thread_keys: Vec::new(),
            named_entities: Vec::new(),
            file_types: vec![
                "markdown".into(),
                "json".into(),
                "jsonl".into(),
                "text".into(),
            ],
        },
        warnings: index.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        room: PathBuf,
        alpha: PathBuf,
        beta: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn fixture() -> Fixture {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "athanor-house-vault-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let room = root.join("work-room");
        let alpha = root.join("alpha-project");
        let beta = root.join("beta-project");
        fs::create_dir_all(&room).unwrap();
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&beta).unwrap();
        fs::write(room.join(ROOM_MARKER), r#"{"version":1,"room":"work-room","vaultRoots":["../alpha-project","../beta-project"],"vaultIgnore":["private/**"]}"#).unwrap();
        fs::write(alpha.join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(alpha.join("README.md"), "---\ntags: [furnace, retrieval]\n---\n# Architecture\nThe exact bridge identifier is HINGE-PROTOCOL-77.\n\n## Failure behavior\nLexical recall remains available when embeddings disappear.\n").unwrap();
        fs::write(
            alpha.join("ignored.md"),
            "HINGE-PROTOCOL-77 must never surface",
        )
        .unwrap();
        fs::write(alpha.join(".env"), "HINGE-PROTOCOL-77=secret").unwrap();
        fs::create_dir(alpha.join("node_modules")).unwrap();
        fs::write(
            alpha.join("node_modules/noise.json"),
            r#"{"hinge":"HINGE-PROTOCOL-77"}"#,
        )
        .unwrap();
        fs::create_dir(alpha.join("private")).unwrap();
        fs::write(alpha.join("private/notes.md"), "HINGE-PROTOCOL-77 hidden").unwrap();
        fs::write(beta.join("projects.json"), r#"{"atlas":{"owner":"Dino","sharedLibrary":"cross-project-capsule"},"leo":{"owner":"Leo","status":"evaluating"}}"#).unwrap();
        fs::write(beta.join("events.jsonl"), "{\"type\":\"decision\",\"project\":\"atlas\",\"value\":\"cold models need attributed evidence\"}\n{ malformed\n{\"type\":\"receipt\",\"project\":\"atlas\",\"value\":\"vault-search-live\"}").unwrap();
        Fixture {
            root,
            room,
            alpha,
            beta,
        }
    }
    fn search(fixture: &Fixture, query: &str) -> VaultRecallResult {
        recall(VaultRecallRequest {
            room_dir: fixture.room.clone(),
            room: "work-room".into(),
            query: query.into(),
        })
        .unwrap()
    }
    #[test]
    fn exact_markdown_recall_is_attributed_and_ignored_paths_stay_absent() {
        let fixture = fixture();
        let result = search(&fixture, "HINGE-PROTOCOL-77");
        assert!(result.found);
        let first = &result.retrieval_candidates[0];
        assert_eq!(
            first.source_path,
            normalized_path(&fixture.alpha.join("README.md"))
        );
        assert_eq!(first.heading_path, "Architecture");
        assert!(first.matched_terms.contains(&"hinge-protocol-77".into()));
        assert!(
            first
                .reasons
                .iter()
                .any(|reason| reason.contains("exact content fields: body"))
        );
        assert!(result.retrieval_candidates.iter().all(|candidate| {
            !candidate.source_path.contains("ignored.md")
                && !candidate.source_path.contains("node_modules")
                && !candidate.source_path.contains("private/")
        }));
    }
    #[test]
    fn structured_records_and_malformed_line_receipts_match_the_file_contract() {
        let fixture = fixture();
        let json = search(&fixture, "cross-project-capsule Dino");
        assert_eq!(
            json.retrieval_candidates[0].source_path,
            normalized_path(&fixture.beta.join("projects.json"))
        );
        assert_eq!(json.retrieval_candidates[0].heading_path, "/atlas");
        let jsonl = search(&fixture, "vault-search-live");
        assert!(
            jsonl.retrieval_candidates[0]
                .heading_path
                .contains("line:3")
        );
        assert!(
            jsonl
                .warnings
                .iter()
                .any(|warning| warning.contains("skipped 1 malformed JSONL record"))
        );
    }
    #[test]
    fn multi_term_paraphrase_and_source_ties_are_deterministic() {
        let fixture = fixture();
        let paraphrase = search(&fixture, "embeddings disappear lexical retrieval");
        assert_eq!(
            paraphrase.retrieval_candidates[0].source_path,
            normalized_path(&fixture.alpha.join("README.md"))
        );
        fs::write(fixture.beta.join("a.md"), "# Receipt\nTIE-MARKER-88").unwrap();
        fs::write(fixture.beta.join("b.md"), "# Receipt\nTIE-MARKER-88").unwrap();
        let ties = search(&fixture, "TIE-MARKER-88");
        let paths = ties
            .retrieval_candidates
            .iter()
            .map(|candidate| candidate.source_path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                normalized_path(&fixture.beta.join("a.md")),
                normalized_path(&fixture.beta.join("b.md"))
            ]
        );
    }
    #[test]
    fn rejects_room_mismatch_and_refuses_symlink_escape() {
        let fixture = fixture();
        let mismatch = recall(VaultRecallRequest {
            room_dir: fixture.room.clone(),
            room: "another-room".into(),
            query: "hinge".into(),
        })
        .unwrap_err();
        assert!(matches!(mismatch, VaultError::RoomMismatch { .. }));
        let outside = fixture.root.join("outside.md");
        fs::write(&outside, "ESCAPE-MARKER-42").unwrap();
        let link = fixture.alpha.join("escape.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside, &link).unwrap();
        let result = search(&fixture, "ESCAPE-MARKER-42");
        assert!(!result.found);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("refused symbolic link"))
        );
    }
    #[test]
    fn chunks_results_and_formats_are_bounded() {
        let fixture = fixture();
        let baseline_documents = search(&fixture, "absent-baseline").indexed_documents;
        let mut body = "padding ".repeat(2_000);
        body.push_str(" BOUNDED-MARKER-99");
        fs::write(fixture.beta.join("long.txt"), body).unwrap();
        fs::write(fixture.beta.join("not-authority.csv"), "BOUNDED-MARKER-99").unwrap();
        for index in 0..10 {
            fs::write(
                fixture.beta.join(format!("bounded-{index:02}.md")),
                format!("# Bounded\nBOUNDED-MARKER-99 receipt {index}"),
            )
            .unwrap();
        }
        let result = search(&fixture, "BOUNDED-MARKER-99");
        assert_eq!(result.indexed_documents, baseline_documents + 13);
        assert_eq!(result.retrieval_candidates.len(), MAX_RESULTS);
        assert!(
            result
                .retrieval_candidates
                .iter()
                .all(|candidate| candidate.excerpt.chars().count() <= EXCERPT_CHARS + 2)
        );
        assert!(
            result
                .retrieval_candidates
                .iter()
                .all(|candidate| !candidate.source_path.ends_with(".csv"))
        );
    }
}
