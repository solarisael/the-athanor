//! The harness registry wire and the two values an owner needs before it can
//! run anything: where the registry lives and the control token it will answer
//! to. Everything here is declaration and refusal; nothing here starts a
//! process.

use crate::{boundaries::SecretSource, layout::InstallLayout};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

pub const HARNESS_REGISTRY_FORMAT: u32 = 1;
pub const REGISTRY_ENV: &str = "ATHANOR_HARNESS_REGISTRY";

const CONTROL_TOKEN_BYTES: usize = 32;
const MAX_IDENTIFIER: usize = 128;
const MAX_DETAIL: usize = 512;

/// An inherited or hidden console would make an operator harness look exactly
/// like a service child of `supervisor.rs`, so every other spelling is refused
/// instead of guessed.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleMode {
    NewWindow,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessEntry {
    pub harness_id: String,
    pub label: String,
    #[serde(default)]
    pub auto_start: bool,
    /// The registry once declared a supervision driver, and `omp` named an OMP
    /// keeper that ran inside the Host. There is no driver now: the keeper
    /// mode owns the console, so this owner supervises `athanor.exe keeper`
    /// as an ordinary process. A file that still declares one is refused by
    /// name, because an ignored `"driver":"omp"` reads as provisioned and
    /// supervises nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    pub program: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub workspace: PathBuf,
    pub console: ConsoleMode,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessRegistryFile {
    pub format: u32,
    #[serde(default)]
    pub harnesses: Vec<HarnessEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessLaunch {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub workspace: PathBuf,
    pub console: ConsoleMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessSpec {
    pub harness_id: String,
    pub label: String,
    pub launch: HarnessLaunch,
}

impl HarnessEntry {
    pub fn resolve(self) -> Result<HarnessSpec> {
        bounded(&self.harness_id, "harnessId")?;
        bounded(&self.label, "label")?;
        if !self
            .harness_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            bail!(
                "harness id {:?} must use ASCII letters, digits, '-', '_' or '.'",
                self.harness_id
            );
        }
        if !self.program.is_absolute() {
            bail!(
                "harness {:?} program {} must be an absolute path",
                self.harness_id,
                self.program.display()
            );
        }
        if !self.workspace.is_absolute() {
            bail!(
                "harness {:?} workspace {} must be an absolute path",
                self.harness_id,
                self.workspace.display()
            );
        }
        if let Some(driver) = self.driver.as_deref() {
            bail!(
                "harness {:?} declares a retired driver field ({driver:?}); this Athanor \
                 supervises processes and holds no harness driver. Delete the field and run \
                 the room through the keeper mode: name the installed athanor.exe as the \
                 program, with arguments [\"keeper\", \"--config\", \
                 \"<room>/.omp/runtime/omp-keeper.json\"].",
                self.harness_id
            );
        }
        Ok(HarnessSpec {
            harness_id: self.harness_id,
            label: self.label,
            launch: HarnessLaunch {
                program: self.program,
                arguments: self.arguments,
                workspace: self.workspace,
                console: self.console,
            },
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HarnessRegistry {
    entries: BTreeMap<String, HarnessSpec>,
    auto_start: Vec<String>,
}

impl HarnessRegistry {
    pub fn parse(text: &str) -> Result<Self> {
        let file: HarnessRegistryFile =
            serde_json::from_str(text).context("parse the harness registry")?;
        if file.format != HARNESS_REGISTRY_FORMAT {
            bail!("unsupported harness registry format {}", file.format);
        }
        let mut entries = BTreeMap::new();
        let mut auto_start = Vec::new();
        for entry in file.harnesses {
            if entry.auto_start {
                auto_start.push(entry.harness_id.clone());
            }
            let spec = entry.resolve()?;
            if entries.contains_key(&spec.harness_id) {
                bail!(
                    "the harness registry declares {:?} more than once",
                    spec.harness_id
                );
            }
            entries.insert(spec.harness_id.clone(), spec);
        }
        Ok(Self { entries, auto_start })
    }

    /// An absent file is an Athanor with no harnesses yet, which must still
    /// start; malformed content is a refusal.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                Self::parse(&text).with_context(|| format!("harness registry {}", path.display()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => {
                Err(error).with_context(|| format!("read harness registry {}", path.display()))
            }
        }
    }

    pub fn auto_start_ids(&self) -> &[String] {
        &self.auto_start
    }

    pub fn get(&self, harness_id: &str) -> Option<&HarnessSpec> {
        self.entries.get(harness_id)
    }

    pub fn specs(&self) -> impl Iterator<Item = &HarnessSpec> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn registry_path(layout: &InstallLayout) -> PathBuf {
    match env::var_os(REGISTRY_ENV) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => layout.harness_registry(),
    }
}

pub fn control_token(secrets: &impl SecretSource) -> Result<String> {
    let mut bytes = [0_u8; CONTROL_TOKEN_BYTES];
    secrets
        .fill(&mut bytes)
        .context("draw the harness control token")?;
    Ok(hex::encode(bytes))
}

fn bounded(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > MAX_IDENTIFIER {
        bail!("harness registry {field} must contain 1 to {MAX_IDENTIFIER} characters");
    }
    Ok(())
}

/// Failure text reaches an operator through the wire, so it is cut to a bound
/// here rather than trusted at whatever length it arrived.
pub(super) fn detail(text: impl Into<String>) -> String {
    let text = text.into();
    match text.char_indices().nth(MAX_DETAIL) {
        Some((cut, _)) => text[..cut].to_owned(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_is_opt_in_and_keeps_file_order() {
        let mut file: serde_json::Value = serde_json::from_str(KEEPER_HARNESS).unwrap();
        let entry = file["harnesses"][0].clone();
        file["harnesses"] = serde_json::json!([
            { "harnessId": "z-first", "autoStart": true },
            { "harnessId": "manual" },
            { "harnessId": "a-last", "autoStart": true },
            { "harnessId": "disabled", "autoStart": false }
        ]);
        for item in file["harnesses"].as_array_mut().unwrap() {
            let overrides = item.as_object().unwrap().clone();
            *item = entry.clone();
            item.as_object_mut().unwrap().extend(overrides);
        }
        let registry = HarnessRegistry::parse(&file.to_string()).unwrap();
        assert_eq!(registry.auto_start_ids(), &["z-first", "a-last"]);
        assert!(HarnessRegistry::parse(KEEPER_HARNESS).unwrap().auto_start_ids().is_empty());
    }

    /// A room's keeper is the one exe in keeper mode: `athanor.exe` is the
    /// program, and the mode plus its config file are the arguments.
    const KEEPER_HARNESS: &str = r#"{
        "format": 1,
        "harnesses": [
            {
                "harnessId": "kintsu-omp",
                "label": "Kintsu OMP",
                "program": "C:/Program Files/Solarisael/Athanor/bin/athanor.exe",
                "arguments": [
                    "keeper",
                    "--config",
                    "C:/Solarisael/Obsidian/obsidian/kintsu/.omp/runtime/omp-keeper.json"
                ],
                "workspace": "C:/Solarisael/Obsidian/obsidian/kintsu",
                "console": "new_window"
            }
        ]
    }"#;

    #[test]
    fn the_keeper_resolves_as_an_ordinary_process_harness() {
        let registry = HarnessRegistry::parse(KEEPER_HARNESS).expect("the keeper entry resolves");
        assert_eq!(registry.len(), 1);
        let spec = registry.get("kintsu-omp").expect("the entry is registered");
        assert_eq!(spec.label, "Kintsu OMP");
        assert!(
            spec.launch.program.ends_with("athanor.exe"),
            "the supervised program is the one exe: {}",
            spec.launch.program.display()
        );
        assert_eq!(
            spec.launch.arguments,
            [
                "keeper",
                "--config",
                "C:/Solarisael/Obsidian/obsidian/kintsu/.omp/runtime/omp-keeper.json"
            ],
            "the arguments reach the keeper mode as written"
        );
        assert_eq!(spec.launch.console, ConsoleMode::NewWindow);
    }

    #[test]
    fn a_declared_omp_driver_is_refused_and_names_the_keeper() {
        let text = KEEPER_HARNESS.replace(
            "\"label\": \"Kintsu OMP\",",
            "\"label\": \"Kintsu OMP\",\n                \"driver\": \"omp\",",
        );
        let error = format!(
            "{:#}",
            HarnessRegistry::parse(&text).expect_err("a declared driver is refused")
        );
        assert!(
            error.contains("retired driver"),
            "the refusal names the retired field: {error}"
        );
        assert!(
            error.contains("athanor.exe") && error.contains("\"keeper\""),
            "the refusal tells the operator which program and mode to run: {error}"
        );
    }
}
