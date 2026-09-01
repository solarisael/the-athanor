use anyhow::{Context, Result, bail};
use protocol::{DEFAULT_HOST_WS_PATH, HOST_ROOM_PATH_PREFIX, is_safe_room_key};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, env, fs, path::Path, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientRoom {
    pub spirit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProjection {
    pub format: u32,
    pub house_id: String,
    pub host_token: String,
    pub state_root: String,
    pub host_url: String,
    pub default_room: String,
    pub rooms: BTreeMap<String, ClientRoom>,
}

impl ClientProjection {
    pub fn validate(&self) -> Result<()> {
        if self.format != 2 {
            bail!("installed OMP client projection format must be 2");
        }
        if [
            &self.house_id,
            &self.host_token,
            &self.host_url,
            &self.default_room,
        ]
        .into_iter()
        .any(|value| value.trim().is_empty())
        {
            bail!("installed OMP client projection identity is incomplete");
        }
        if !Path::new(&self.state_root).is_absolute() {
            bail!("installed OMP client stateRoot must be absolute");
        }
        if !self.rooms.contains_key(&self.default_room) {
            bail!("installed OMP client defaultRoom has no room identity");
        }
        for (room, identity) in &self.rooms {
            if [!is_safe_room_key(room), identity.spirit.trim().is_empty()]
                .into_iter()
                .any(|invalid| invalid)
            {
                bail!("installed OMP client room identity is incomplete");
            }
        }
        Ok(())
    }

    /// Reads and validates the invoking user's restricted projection.
    pub fn installed() -> Result<Self> {
        let user_profile =
            PathBuf::from(env::var_os("USERPROFILE").context("USERPROFILE is unavailable")?);
        let path = user_profile.join(".omp/agent/athanor/client.json");
        let client: Self = serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )?;
        client.validate()?;
        Ok(client)
    }

    // [protocol/host/path] [protocol/room/key]
    /// The room-scoped WebSocket URL, or a refusal when the room is unknown.
    pub fn room_ws_url(&self, room: &str) -> Result<(String, &ClientRoom)> {
        let identity = self
            .rooms
            .get(room)
            .with_context(|| format!("installed Athanor has no room identity for {room:?}"))?;
        let base = self.host_url.trim_end_matches('/');
        Ok((
            format!("{base}{HOST_ROOM_PATH_PREFIX}{room}{DEFAULT_HOST_WS_PATH}"),
            identity,
        ))
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
    let adapter_entry = value.ends_with("/index.ts") || value.ends_with("/hygiene.ts");
    let owned_adapter = value.contains("/the-athanor/adapters/omp/")
        || value.contains("/athanor-omp/")
        || value.contains("/solarisael/athanor/components/omp-adapter/versions/")
        || (value.contains("/solarisael/athanor/versions/") && value.contains("/adapters/omp/"));
    (owned_adapter && adapter_entry)
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
