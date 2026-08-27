use regex::Regex;
use std::fs;
use std::path::Path;

const ELIGIBLE_EXTENSIONS: &[&str] = &["md", "markdown", "json", "jsonl", "txt"];
pub(crate) const IGNORED_DIRECTORIES: &[&str] = &[
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

pub(crate) struct IgnoreRule {
    negated: bool,
    directory_only: bool,
    regex: Regex,
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
pub(crate) fn root_ignore_rules(root: &Path, configured: &[String]) -> Vec<IgnoreRule> {
    let lines = fs::read_to_string(root.join(".gitignore"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .chain(configured.iter().cloned())
        .collect::<Vec<_>>();
    parse_ignore_rules(lines)
}
pub(crate) fn ignored_by_rules(
    relative_path: &str,
    is_directory: bool,
    rules: &[IgnoreRule],
) -> bool {
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
pub(crate) fn ignored_file(name: &str) -> bool {
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
pub(crate) fn eligible_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| ELIGIBLE_EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}
