//! The one-use launch manifest of a Host-driven terminal relaunch.
//!
//! A terminal started by the Host's own CLI never hands its caller's
//! environment to the program it opens (Tern drops it), so the Host writes the
//! exact launch into a private manifest and the terminal starts
//! `athanor launch-session --manifest <path>`. Only that path crosses the
//! command line; the Host bearer travels inside the file and reaches the
//! harness environment after the file is read and removed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const LAUNCH_SESSION_MODE: &str = "launch-session";
pub const MANIFEST_FLAG: &str = "--manifest";
pub const MANIFEST_FORMAT: u32 = 1;
/// Manifests live in this directory of the Host's private state directory.
pub const MANIFEST_DIR: &str = "portable-launch";
pub const MANIFEST_MAX_BYTES: u64 = 64 * 1024;
/// Tern starts the helper within seconds; an older manifest is never honored.
pub const MANIFEST_TTL_SECS: u64 = 120;

/// Room scope the Host provisions from its own config. Every key is required.
pub const ROOM_ENV: [&str; 4] = [
    "ATHANOR_CLIENT_ROOM_DIR",
    "ATHANOR_HOST_URL",
    "ATHANOR_HOST_TOKEN",
    "ATHANOR_HOST_HOUSE_ID",
];
/// OMP profile selection captured from the live predecessor. A key it did not
/// have is removed from the successor too, so an inherited terminal value can
/// never swap the operator's harness profile.
pub const PROFILE_ENV: [&str; 4] = [
    "PI_CODING_AGENT_DIR",
    "OMP_PROFILE",
    "PI_PROFILE",
    "PI_CONFIG_DIR",
];
/// Removed from the inherited environment and never set: a relaunched harness
/// is not a keeper child.
pub const CLEARED_ENV: [&str; 1] = ["ATHANOR_KEEPER_INSTANCE_ID"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LaunchManifest {
    pub format: u32,
    /// Lowercase hyphenated UUID; the manifest file is named `<launchId>.json`.
    pub launch_id: String,
    pub expires_at_unix: u64,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub environment: BTreeMap<String, String>,
}

fn safe_value(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.contains(['\0', '\n', '\r'])
}

/// A lowercase hyphenated UUID, the only spelling a manifest name may carry.
pub fn is_launch_id(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(index, c)| match index {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        })
}

/// OMP's own profile name rule (pi-utils PROFILE_NAME_RE).
fn is_profile_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Validate one captured profile variable. Shared by the Host (at attach) and
/// the helper (at launch) so both read the same rule.
pub fn validate_profile_value(key: &str, value: &str) -> Result<(), String> {
    if !safe_value(value) {
        return Err(format!("{key} holds an unsafe value"));
    }
    match key {
        "PI_CODING_AGENT_DIR" if !Path::new(value).is_absolute() => {
            Err("PI_CODING_AGENT_DIR must be an absolute path".into())
        }
        "OMP_PROFILE" | "PI_PROFILE" if !is_profile_name(value) => {
            Err(format!("{key} is not an OMP profile name"))
        }
        "PI_CONFIG_DIR" if value.contains(['/', '\\', ':']) || value == "." || value == ".." => {
            Err("PI_CONFIG_DIR must be one directory name".into())
        }
        "PI_CODING_AGENT_DIR" | "OMP_PROFILE" | "PI_PROFILE" | "PI_CONFIG_DIR" => Ok(()),
        other => Err(format!("{other} is not a carried harness profile variable")),
    }
}

impl LaunchManifest {
    /// Structural rules only; expiry and the file binding are the reader's.
    pub fn validate(&self) -> Result<(), String> {
        if self.format != MANIFEST_FORMAT {
            return Err(format!(
                "unsupported launch manifest format {}",
                self.format
            ));
        }
        if !is_launch_id(&self.launch_id) {
            return Err("launchId is not a lowercase UUID".into());
        }
        if !safe_value(&self.program) || !Path::new(&self.program).is_absolute() {
            return Err("program must be an absolute path".into());
        }
        if !safe_value(&self.cwd) || !Path::new(&self.cwd).is_absolute() {
            return Err("cwd must be an absolute path".into());
        }
        if self.args.len() > 256
            || self
                .args
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains(['\0', '\n', '\r']))
        {
            return Err("args holds an unsafe argument".into());
        }
        for key in ROOM_ENV {
            match self.environment.get(key) {
                Some(value) if safe_value(value) => {}
                _ => return Err(format!("{key} is missing from the room scope")),
            }
        }
        for (key, value) in &self.environment {
            if ROOM_ENV.contains(&key.as_str()) {
                continue;
            }
            validate_profile_value(key, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> LaunchManifest {
        let absolute = if cfg!(windows) { "C:\\work" } else { "/work" };
        LaunchManifest {
            format: MANIFEST_FORMAT,
            launch_id: "0b0e7a52-3f4e-4b7a-9d8c-1e2f3a4b5c6d".into(),
            expires_at_unix: 1,
            program: absolute.into(),
            args: vec!["--resume".into(), absolute.into()],
            cwd: absolute.into(),
            environment: ROOM_ENV
                .iter()
                .map(|key| (key.to_string(), "value".to_string()))
                .collect(),
        }
    }

    #[test]
    fn room_scope_is_required_and_foreign_keys_refuse() {
        assert!(manifest().validate().is_ok());
        let mut missing = manifest();
        missing.environment.remove("ATHANOR_HOST_TOKEN");
        assert!(missing.validate().is_err());
        let mut foreign = manifest();
        foreign
            .environment
            .insert("OPENAI_API_KEY".into(), "secret".into());
        assert!(foreign.validate().is_err());
        let mut profile = manifest();
        profile
            .environment
            .insert("OMP_PROFILE".into(), "work".into());
        assert!(profile.validate().is_ok());
        profile
            .environment
            .insert("PI_PROFILE".into(), "../escape".into());
        assert!(profile.validate().is_err());
    }

    #[test]
    fn launch_ids_are_lowercase_hyphenated_uuids() {
        assert!(is_launch_id("0b0e7a52-3f4e-4b7a-9d8c-1e2f3a4b5c6d"));
        assert!(!is_launch_id("0B0E7A52-3F4E-4B7A-9D8C-1E2F3A4B5C6D"));
        assert!(!is_launch_id("../../0b0e7a52-3f4e-4b7a-9d8c-1e2f3a4b"));
    }
}
