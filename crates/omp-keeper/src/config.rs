use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE_NAME: &str = "omp-keeper.json";
pub const DEFAULT_CLAIMANT: &str = "omp-keeper";
pub const DEFAULT_WATCH_INTERVAL_SECS: u64 = 30;
pub const MAX_WATCH_INTERVAL_SECS: u64 = 3600;
pub const USAGE: &str = "usage: omp-keeper [--config <path>]";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KeeperConfig {
    pub omp_launch: Vec<String>,
    pub workspace: String,
    pub program_root: PathBuf,
    #[serde(default = "default_claimant")]
    pub claimant: String,
    #[serde(default = "default_watch_interval_secs")]
    pub watch_interval_secs: u64,
    #[serde(default)]
    pub capability: Option<Secret>,
    #[serde(default)]
    pub capability_path: Option<PathBuf>,
}

/// The keeper's restart_claim secret: provisioned operator-side, never logged and never in Debug.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret(redacted)")
    }
}

fn default_claimant() -> String {
    DEFAULT_CLAIMANT.to_string()
}

fn default_watch_interval_secs() -> u64 {
    DEFAULT_WATCH_INTERVAL_SECS
}

impl KeeperConfig {
    pub fn validate(&self) -> Result<()> {
        let (program, _) = self
            .omp_launch
            .split_first()
            .context("ompLaunch must name the omp program and its arguments")?;
        if program.trim().is_empty() {
            bail!("ompLaunch first entry must name the omp program");
        }
        if self.workspace.trim().is_empty() {
            bail!("workspace must name the omp workspace path");
        }
        if self.program_root.as_os_str().is_empty() {
            bail!("programRoot must name the installed Athanor program root");
        }
        if !is_principal_name(&self.claimant) {
            bail!("claimant must be a lowercase slug, for example omp-keeper");
        }
        if self.watch_interval_secs > MAX_WATCH_INTERVAL_SECS {
            bail!("watchIntervalSecs must be {MAX_WATCH_INTERVAL_SECS} or less");
        }
        match (&self.capability, &self.capability_path) {
            (Some(_), Some(_)) => {
                bail!("give exactly one of capability or capabilityPath, not both")
            }
            (None, None) => bail!("give exactly one of capability or capabilityPath"),
            (Some(secret), None) if secret.expose().trim().is_empty() => {
                bail!("capability must not be blank")
            }
            (None, Some(path)) if path.as_os_str().is_empty() => {
                bail!("capabilityPath must name the keeper capability file")
            }
            _ => {}
        }
        Ok(())
    }

    pub fn read_capability(&self) -> Result<Secret> {
        if let Some(secret) = &self.capability {
            return Ok(secret.clone());
        }
        let path = self
            .capability_path
            .as_ref()
            .context("give exactly one of capability or capabilityPath")?;
        let text = std::fs::read_to_string(path).with_context(|| {
            format!(
                "keeper capability file could not be read: {}",
                path.display()
            )
        })?;
        let secret = text.trim();
        if secret.is_empty() {
            bail!("keeper capability file is empty: {}", path.display());
        }
        Ok(Secret(secret.to_string()))
    }

    pub fn program(&self) -> &str {
        &self.omp_launch[0]
    }

    pub fn program_args(&self) -> &[String] {
        &self.omp_launch[1..]
    }
}

/// Mirrors ROOM_KEY_RE in house-substrate/src/config.rs, the name shape the substrate accepts.
pub fn is_principal_name(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let mut after_dash = true;
    for character in value.chars() {
        match character {
            'a'..='z' | '0'..='9' => after_dash = false,
            '-' if !after_dash => after_dash = true,
            _ => return false,
        }
    }
    !after_dash
}

pub fn parse(text: &str) -> Result<KeeperConfig> {
    let config: KeeperConfig =
        serde_json::from_str(text).context("keeper config file is not valid keeper JSON")?;
    config.validate()?;
    Ok(config)
}

pub fn load(path: &Path) -> Result<KeeperConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("keeper config file could not be read: {}", path.display()))?;
    parse(&text).with_context(|| format!("keeper config file {}", path.display()))
}

pub fn default_config_path(executable: &Path) -> PathBuf {
    match executable.parent() {
        Some(directory) => directory.join(CONFIG_FILE_NAME),
        None => PathBuf::from(CONFIG_FILE_NAME),
    }
}

pub fn config_path_from_args<I>(arguments: I) -> Result<Option<PathBuf>>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    let mut path = None;
    while let Some(argument) = arguments.next() {
        let value = if argument == "--config" {
            arguments
                .next()
                .context("--config needs a path; {USAGE}")?
        } else if let Some(inline) = argument.strip_prefix("--config=") {
            inline.to_string()
        } else {
            bail!("unknown argument {argument}; {USAGE}");
        };
        if value.trim().is_empty() {
            bail!("--config needs a path; {USAGE}");
        }
        if path.is_some() {
            bail!("--config was given twice; {USAGE}");
        }
        path = Some(PathBuf::from(value));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITY_LINE: &str = "\"capabilityPath\": \"D:/ProgramData/keeper.capability\"";

    const MINIMAL: &str = r#"{
        "ompLaunch": ["C:/Program Files/omp/omp.exe", "--resume"],
        "workspace": "C:/Solarisael/Obsidian/obsidian/kodo",
        "programRoot": "C:/Program Files/The Athanor",
        "capabilityPath": "D:/ProgramData/keeper.capability"
    }"#;

    fn with_fields(extra: &str) -> String {
        MINIMAL.replace(CAPABILITY_LINE, &format!("{CAPABILITY_LINE}, {extra}"))
    }

    #[test]
    fn parses_the_minimal_config_and_applies_defaults() {
        let config = parse(MINIMAL).expect("minimal config parses");
        assert_eq!(config.program(), "C:/Program Files/omp/omp.exe");
        assert_eq!(config.program_args(), ["--resume"]);
        assert_eq!(config.workspace, "C:/Solarisael/Obsidian/obsidian/kodo");
        assert_eq!(
            config.program_root,
            PathBuf::from("C:/Program Files/The Athanor")
        );
        assert_eq!(config.claimant, DEFAULT_CLAIMANT);
        assert_eq!(config.watch_interval_secs, DEFAULT_WATCH_INTERVAL_SECS);
    }

    #[test]
    fn refuses_unknown_fields() {
        let text = MINIMAL.replace("\"workspace\"", "\"workSpace\"");
        let error = parse(&text).expect_err("unknown field refuses");
        assert!(format!("{error:#}").contains("not valid keeper JSON"));
    }

    #[test]
    fn refuses_a_config_with_no_capability_door_or_both_doors() {
        let neither = MINIMAL.replace(CAPABILITY_LINE, "\"claimant\": \"omp-keeper\"");
        let error = parse(&neither).expect_err("no capability refuses");
        assert!(format!("{error:#}").contains("exactly one of capability"));

        let both = with_fields("\"capability\": \"secret-value\"");
        let error = parse(&both).expect_err("both doors refuse");
        assert!(format!("{error:#}").contains("not both"));

        let blank = MINIMAL.replace(CAPABILITY_LINE, "\"capability\": \"  \"");
        let error = parse(&blank).expect_err("blank capability refuses");
        assert!(format!("{error:#}").contains("must not be blank"));
    }

    #[test]
    fn the_capability_never_reaches_debug_output() {
        let text = MINIMAL.replace(CAPABILITY_LINE, "\"capability\": \"topsecret-value\"");
        let config = parse(&text).expect("inline capability parses");
        let rendered = format!("{config:?}");
        assert!(
            !rendered.contains("topsecret-value"),
            "the secret must not print: {rendered}"
        );
        assert!(rendered.contains("Secret(redacted)"));
        assert_eq!(
            config.read_capability().expect("inline read").expose(),
            "topsecret-value"
        );
    }

    #[test]
    fn reads_the_capability_from_its_file_and_names_the_path_when_it_cannot() {
        let temp = tempfile::tempdir().expect("temp dir");
        let file = temp.path().join("keeper.capability");
        std::fs::write(&file, "file-secret\n").expect("capability file");
        let text = MINIMAL.replace(
            CAPABILITY_LINE,
            &format!("\"capabilityPath\": {}", serde_json::json!(file)),
        );
        let config = parse(&text).expect("capability path parses");
        assert_eq!(config.read_capability().expect("file read").expose(), "file-secret");

        std::fs::write(&file, "   \n").expect("blank capability file");
        let error = config.read_capability().expect_err("blank file refuses");
        assert!(format!("{error:#}").contains("is empty"));

        std::fs::remove_file(&file).expect("remove capability file");
        let error = config.read_capability().expect_err("missing file refuses");
        let rendered = format!("{error:#}");
        assert!(rendered.contains("could not be read"));
        assert!(!rendered.contains("file-secret"));
    }

    #[test]
    fn refuses_an_empty_launch_and_a_blank_program() {
        let empty = MINIMAL.replace(
            "[\"C:/Program Files/omp/omp.exe\", \"--resume\"]",
            "[]",
        );
        let error = parse(&empty).expect_err("empty ompLaunch refuses");
        assert!(format!("{error:#}").contains("ompLaunch must name"));

        let blank = MINIMAL.replace("C:/Program Files/omp/omp.exe", "   ");
        let error = parse(&blank).expect_err("blank program refuses");
        assert!(format!("{error:#}").contains("first entry must name"));
    }

    #[test]
    fn refuses_a_blank_workspace_and_an_out_of_range_interval() {
        let blank = MINIMAL.replace("C:/Solarisael/Obsidian/obsidian/kodo", " ");
        let error = parse(&blank).expect_err("blank workspace refuses");
        assert!(format!("{error:#}").contains("workspace must name"));

        let text = with_fields("\"watchIntervalSecs\": 3601");
        let error = parse(&text).expect_err("oversized interval refuses");
        assert!(format!("{error:#}").contains("3600 or less"));
    }

    #[test]
    fn accepts_a_disabled_watch_and_a_named_claimant() {
        let text = with_fields("\"watchIntervalSecs\": 0, \"claimant\": \"omp-keeper-kodo\"");
        let config = parse(&text).expect("explicit fields parse");
        assert_eq!(config.watch_interval_secs, 0);
        assert_eq!(config.claimant, "omp-keeper-kodo");
    }

    #[test]
    fn principal_names_follow_the_substrate_slug_shape() {
        assert!(is_principal_name("omp-keeper"));
        assert!(is_principal_name("keeper1"));
        assert!(!is_principal_name(""));
        assert!(!is_principal_name("-keeper"));
        assert!(!is_principal_name("keeper-"));
        assert!(!is_principal_name("omp--keeper"));
        assert!(!is_principal_name("OmpKeeper"));
        assert!(!is_principal_name("omp keeper"));
    }

    #[test]
    fn refuses_a_claimant_the_substrate_would_reject() {
        let text = with_fields("\"claimant\": \"Omp Keeper\"");
        let error = parse(&text).expect_err("invalid claimant refuses");
        assert!(format!("{error:#}").contains("lowercase slug"));
    }

    #[test]
    fn reads_the_config_path_from_the_arguments() {
        assert_eq!(config_path_from_args(Vec::<String>::new()).unwrap(), None);
        assert_eq!(
            config_path_from_args(vec!["--config".to_string(), "D:/k.json".to_string()]).unwrap(),
            Some(PathBuf::from("D:/k.json"))
        );
        assert_eq!(
            config_path_from_args(vec!["--config=D:/k.json".to_string()]).unwrap(),
            Some(PathBuf::from("D:/k.json"))
        );
        assert!(config_path_from_args(vec!["--config".to_string()]).is_err());
        assert!(config_path_from_args(vec!["--wat".to_string()]).is_err());
        assert!(
            config_path_from_args(vec![
                "--config=a".to_string(),
                "--config=b".to_string()
            ])
            .is_err()
        );
    }

    #[test]
    fn defaults_the_config_beside_the_executable() {
        let path = default_config_path(Path::new("D:/install/omp-keeper.exe"));
        assert_eq!(path, PathBuf::from("D:/install").join(CONFIG_FILE_NAME));
    }
}
