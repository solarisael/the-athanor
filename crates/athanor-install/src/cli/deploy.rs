use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DRIVER: &str = "substrate/deploy-local.ps1";
const HELP: &str = "Usage:
  athanor deploy prepare [--source DIR] [--build LABEL] [--skip-tests]
  athanor deploy install RECEIPT [--source DIR]

Prepare tests and builds the release while the House stays open.
Install uses the prepared bytes. It does not build again.
Close the House Host and every OMP session before installation.
Run installation from an elevated terminal. It backs up the database,
applies migrations, installs the release and adapter, and checks health.
Restart the Host and OMP after installation.

Run from The Athanor checkout, or select it with --source DIR.
Install first looks above the receipt for its checkout, then above the
current directory. An explicit --source always takes precedence.
Keep the receipt and its adjacent payload directory together.
PowerShell 7 is required. Preparation also needs the build toolchain.
--skip-tests records an unverified test gate.
";

pub(super) fn run(arguments: &[String]) -> Result<ExitCode> {
    let help = arguments.is_empty()
        || (arguments.len() == 1 && is_help(&arguments[0]))
        || (arguments.len() == 2
            && matches!(arguments[0].as_str(), "prepare" | "install")
            && is_help(&arguments[1]));

    if help {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    }

    let options = parse(arguments)?;

    if !cfg!(windows) {
        bail!("athanor deploy supports Windows only; use the platform deployment driver");
    }

    let cwd = std::env::current_dir().context("read deployment working directory")?;
    let receipt = match options.action {
        Action::Install(path) => {
            let resolved = std::path::absolute(cwd.join(path))
                .with_context(|| format!("resolve prepared receipt {path:?}"))?;

            if !resolved.is_file() {
                bail!("prepared receipt is not a file: {}", resolved.display());
            }

            Some(resolved)
        }
        Action::Prepare { .. } => None,
    };
    let source = resolve_source(options.source, receipt.as_deref(), &cwd)?;
    println!("Source checkout: {}", source.display());
    let mut command = Command::new("pwsh");
    command
        .args(["-NoProfile", "-File"])
        .arg(source.join(DRIVER))
        .current_dir(&source);

    match options.action {
        Action::Prepare { build, skip_tests } => {
            command.arg("-PrepareOnly");

            if let Some(build) = build {
                command.arg("-Build").arg(build);
            }
            if skip_tests {
                command.arg("-SkipTests");
            }
        }
        Action::Install(_) => {
            let receipt = receipt
                .as_ref()
                .context("prepared receipt was not resolved")?;
            println!("Prepared receipt: {}", receipt.display());
            command.arg("-PreparedRelease").arg(receipt);
        }
    }

    let status = command
        .status()
        .context("run the deployment driver with PowerShell 7 (pwsh)")?;

    Ok(ExitCode::from(
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
    ))
}

#[derive(Debug, PartialEq)]
enum Action<'a> {
    Prepare {
        build: Option<&'a str>,
        skip_tests: bool,
    },
    Install(&'a str),
}

#[derive(Debug, PartialEq)]
struct Options<'a> {
    action: Action<'a>,
    source: Option<&'a str>,
}

fn is_help(argument: &str) -> bool {
    matches!(argument, "help" | "--help" | "-h")
}

fn parse(arguments: &[String]) -> Result<Options<'_>> {
    let (mode, arguments) = arguments.split_first().context("run athanor deploy help")?;

    if !matches!(mode.as_str(), "prepare" | "install") {
        bail!("unknown deployment action {mode:?}; run athanor deploy help");
    }

    let mut source = None;
    let mut build = None;
    let mut skip_tests = false;
    let mut receipt = None;
    let mut arguments = arguments.iter();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--source" if source.is_none() => {
                source = Some(option_value(&mut arguments, "--source")?);
            }
            "--build" if mode == "prepare" && build.is_none() => {
                build = Some(option_value(&mut arguments, "--build")?);
            }
            "--skip-tests" if mode == "prepare" && !skip_tests => skip_tests = true,
            value
                if mode == "install"
                    && receipt.is_none()
                    && !value.starts_with('-')
                    && !value.is_empty() =>
            {
                receipt = Some(value);
            }
            _ => bail!(
                "unexpected or repeated deployment argument {argument:?}; run athanor deploy help"
            ),
        }
    }

    let action = if mode == "prepare" {
        Action::Prepare { build, skip_tests }
    } else {
        Action::Install(receipt.context("install requires a receipt; run athanor deploy help")?)
    };

    Ok(Options { action, source })
}

fn option_value<'a>(arguments: &mut std::slice::Iter<'a, String>, flag: &str) -> Result<&'a str> {
    let value = arguments
        .next()
        .with_context(|| format!("{flag} requires a value"))?;

    if value.is_empty() || value.starts_with('-') {
        bail!("{flag} requires a value before another option");
    }

    Ok(value)
}

fn resolve_source(source: Option<&str>, receipt: Option<&Path>, cwd: &Path) -> Result<PathBuf> {
    if let Some(source) = source {
        // PowerShell rejects the Windows verbatim paths produced by canonicalize.
        let root = std::path::absolute(cwd.join(source))
            .with_context(|| format!("resolve source checkout {source:?}"))?;

        if !is_checkout(&root) {
            bail!(
                "{} is not The Athanor source checkout with {DRIVER}",
                root.display()
            );
        }

        return Ok(root);
    }

    if let Some(root) = receipt.and_then(Path::parent).and_then(find_checkout) {
        return Ok(root.to_path_buf());
    }
    if let Some(root) = find_checkout(cwd) {
        return Ok(root.to_path_buf());
    }

    bail!(
        "cannot find The Athanor source checkout; run from the checkout or pass --source DIR (athanor deploy help)"
    )
}

fn find_checkout(path: &Path) -> Option<&Path> {
    path.ancestors().find(|root| is_checkout(root))
}

fn is_checkout(root: &Path) -> bool {
    if !root.join(DRIVER).is_file() || !root.join("Cargo.toml").is_file() {
        return false;
    }

    let Ok(bytes) = std::fs::read(root.join("package.json")) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };

    manifest.get("name").and_then(serde_json::Value::as_str) == Some("the-athanor")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn deployment_actions_reject_ambiguous_or_incompatible_arguments() {
        for values in [
            vec!["install"],
            vec!["prepare", "--source", "--skip-tests"],
            vec!["prepare", "--build"],
            vec!["prepare", "--source", "a", "--source", "b"],
            vec!["prepare", "--build", "a", "--build", "b"],
            vec!["prepare", "--skip-tests", "--skip-tests"],
            vec!["install", "receipt", "--skip-tests"],
            vec!["install", "receipt", "--build", "a"],
            vec!["install", "first", "second"],
            vec!["prepare", "receipt"],
            vec!["prepare", "--skpi-tests"],
            vec!["erase"],
        ] {
            assert!(
                parse(&args(&values)).is_err(),
                "accepted ambiguous arguments: {values:?}"
            );
        }
    }

    fn checkout(root: &Path) {
        std::fs::create_dir_all(root.join("substrate")).unwrap();
        std::fs::write(root.join(DRIVER), "throw 'discovery must not execute this'").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.join("package.json"), r#"{"name":"the-athanor"}"#).unwrap();
    }

    #[test]
    fn explicit_checkout_beats_receipt_and_current_directory_without_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let explicit = temp.path().join("explicit");
        let current = temp.path().join("current");
        let prepared = temp.path().join("prepared");
        for root in [&explicit, &current, &prepared] {
            checkout(root);
        }
        let receipt = prepared.join("target/releases/one/prepared-release.json");
        let explicit_name = explicit.to_str().unwrap();
        assert_eq!(
            resolve_source(Some(explicit_name), Some(&receipt), &current)
                .unwrap()
                .canonicalize()
                .unwrap(),
            explicit.canonicalize().unwrap()
        );
        assert!(resolve_source(Some("missing"), Some(&receipt), &current).is_err());
        std::fs::write(
            explicit.join("package.json"),
            r#"{"name":"another-project"}"#,
        )
        .unwrap();
        assert!(resolve_source(Some(explicit_name), Some(&receipt), &current).is_err());
    }

    #[test]
    fn receipt_checkout_precedes_current_checkout_and_unrelated_paths_refuse() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("current");
        let prepared = temp.path().join("prepared");
        checkout(&current);
        checkout(&prepared);
        let receipt = prepared.join("target/releases/one/prepared-release.json");
        assert_eq!(
            resolve_source(None, Some(&receipt), &current).unwrap(),
            prepared
        );
        assert_eq!(
            resolve_source(None, None, &current.join("substrate")).unwrap(),
            current
        );
        assert!(resolve_source(None, None, temp.path()).is_err());
    }
}
