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

/// Supervision is declared, never read out of an id or a program path. `Omp`
/// starts exactly like `Process` today; it is the door where `request_restart`
/// supervision lands without reaching plain processes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessDriver {
    #[default]
    Process,
    Omp,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessEntry {
    pub harness_id: String,
    pub label: String,
    #[serde(default)]
    pub driver: HarnessDriver,
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
pub enum HarnessKind {
    Process(HarnessLaunch),
    Omp(HarnessLaunch),
}

impl HarnessKind {
    pub fn launch(&self) -> &HarnessLaunch {
        match self {
            Self::Process(launch) | Self::Omp(launch) => launch,
        }
    }

    pub fn driver(&self) -> HarnessDriver {
        match self {
            Self::Process(_) => HarnessDriver::Process,
            Self::Omp(_) => HarnessDriver::Omp,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessSpec {
    pub harness_id: String,
    pub label: String,
    pub kind: HarnessKind,
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
        let launch = HarnessLaunch {
            program: self.program,
            arguments: self.arguments,
            workspace: self.workspace,
            console: self.console,
        };
        let kind = match self.driver {
            HarnessDriver::Process => HarnessKind::Process(launch),
            HarnessDriver::Omp => HarnessKind::Omp(launch),
        };
        Ok(HarnessSpec {
            harness_id: self.harness_id,
            label: self.label,
            kind,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HarnessRegistry {
    entries: BTreeMap<String, HarnessSpec>,
}

impl HarnessRegistry {
    pub fn parse(text: &str) -> Result<Self> {
        let file: HarnessRegistryFile =
            serde_json::from_str(text).context("parse the harness registry")?;
        if file.format != HARNESS_REGISTRY_FORMAT {
            bail!("unsupported harness registry format {}", file.format);
        }
        let mut entries = BTreeMap::new();
        for entry in file.harnesses {
            let spec = entry.resolve()?;
            if entries.contains_key(&spec.harness_id) {
                bail!(
                    "the harness registry declares {:?} more than once",
                    spec.harness_id
                );
            }
            entries.insert(spec.harness_id.clone(), spec);
        }
        Ok(Self { entries })
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
