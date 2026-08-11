use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientEndpoint {
    pub url: String,
    pub spirit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProjection {
    pub format: u32,
    pub house_id: String,
    pub host_token: String,
    pub state_root: String,
    pub default_room: String,
    pub endpoints: std::collections::BTreeMap<String, ClientEndpoint>,
}

impl ClientProjection {
    pub fn validate(&self) -> Result<()> {
        if self.format != 1 || self.house_id.trim().is_empty() || self.host_token.trim().is_empty()
        {
            bail!("installed OMP client projection identity is incomplete");
        }
        if !Path::new(&self.state_root).is_absolute() {
            bail!("installed OMP client stateRoot must be absolute");
        }
        if !self.endpoints.contains_key(&self.default_room) {
            bail!("installed OMP client defaultRoom has no endpoint");
        }
        for (room, endpoint) in &self.endpoints {
            if room.trim().is_empty() || endpoint.spirit.trim().is_empty() {
                bail!("installed OMP client endpoint identity is incomplete");
            }
            let url = url::Url::parse(&endpoint.url)?;
            let loopback = url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if url.scheme() != "ws" || !loopback {
                bail!("installed OMP client endpoint for {room:?} must be loopback WebSocket");
            }
        }
        Ok(())
    }
}

fn newline(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn item_value(line: &str) -> Option<String> {
    let value = line.trim_start().strip_prefix("- ")?.trim();
    Some(
        value
            .trim_matches('"')
            .trim_matches('\'')
            .replace("\\\"", "\""),
    )
}

fn normalized(value: &str) -> String {
    value.replace('\\', "/").to_ascii_lowercase()
}
fn is_athanor_extension(value: &str) -> bool {
    let value = normalized(value);
    ((value.contains("/the-athanor/adapters/omp/") || value.contains("/solarisael-house-omp/"))
        && (value.ends_with("/index.ts") || value.ends_with("/hygiene.ts")))
        || value.ends_with("/solarisael/athanor/bin/athanor-omp-loader.ts")
}

fn quoted_path(path: &Path) -> String {
    let value = path
        .display()
        .to_string()
        .replace('\\', "/")
        .replace('"', "\\\"");
    format!("  - \"{value}\"")
}

pub fn register_extension(text: &str, loader: &Path) -> String {
    let separator = newline(text);
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let entry = quoted_path(loader);
    let section = lines
        .iter()
        .position(|line| !line.starts_with(char::is_whitespace) && line.trim() == "extensions:");
    match section {
        Some(start) => {
            let end = (start + 1..lines.len())
                .find(|&index| {
                    let line = &lines[index];
                    !line.trim().is_empty()
                        && !line.trim_start().starts_with('#')
                        && !line.starts_with(char::is_whitespace)
                })
                .unwrap_or(lines.len());
            let mut block = lines.drain(start + 1..end).collect::<Vec<_>>();
            block.retain(|line| item_value(line).is_none_or(|value| !is_athanor_extension(&value)));
            block.push(entry);
            lines.splice(start + 1..start + 1, block);
        }
        None => {
            if lines.last().is_some_and(|line| !line.is_empty()) {
                lines.push(String::new());
            }
            lines.push("extensions:".into());
            lines.push(entry);
        }
    }
    let mut output = lines.join(separator);
    output.push_str(separator);
    output
}

pub fn unregister_extension(text: &str, loader: &Path) -> String {
    let expected = normalized(&loader.display().to_string());
    let separator = newline(text);
    let mut output = text
        .lines()
        .filter(|line| {
            item_value(line)
                .map(|value| normalized(&value) != expected)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>()
        .join(separator);
    output.push_str(separator);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_converges_source_and_version_entries_to_one_stable_loader() {
        let loader = Path::new("C:/Program Files/Solarisael/Athanor/bin/athanor-omp-loader.ts");
        let source = "theme: dark\nextensions:\n  - C:/repo/the-athanor/adapters/omp/index.ts\n  - C:/repo/the-athanor/adapters/omp/hygiene.ts\n  - C:/foreign/extension.ts\ntools:\n  quiet: false\n";
        let once = register_extension(source, loader);
        let twice = register_extension(&once, loader);
        assert_eq!(once, twice);
        assert!(once.contains("C:/foreign/extension.ts"));
        assert!(!once.contains("C:/repo/the-athanor/adapters/omp"));
        assert_eq!(once.matches("athanor-omp-loader.ts").count(), 1);
        let removed = unregister_extension(&once, loader);
        assert!(removed.contains("C:/foreign/extension.ts"));
        assert!(!removed.contains("athanor-omp-loader.ts"));
    }
}
