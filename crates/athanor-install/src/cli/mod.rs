//! The one exe. `athanor.exe` with no arguments is the Host; every other
//! mode is named by its first argument. The modes were separate binaries
//! once (athanor-manage.exe, omp-keeper.exe, athanor-chat.exe); the
//! operator ruled one exe, so they are doors of this one.

pub mod chat;
pub mod keeper;
pub mod manage;
pub mod start;
pub mod status;

use crate::{app, service};
use anyhow::{Context, Result};
use std::process::ExitCode;

const HELP: &str = "The Athanor\n\n\
Usage: athanor [MODE] [OPTIONS]\n\n\
Modes:\n  \
(none)                 Run the Host for this House.\n  \
status                 Show every component: installed, running, reachable, healthy.\n  \
start [--service]      Start what is missing. --service starts the Windows service and needs administrator rights.\n  \
keeper ROOM            Run the OMP keeper for one room, by its harness registry name.\n  \
keeper --config FILE   Run the OMP keeper for the room whose config is FILE.\n  \
chat [--room ROOM]     Talk to a room from this terminal.\n  \
doctor                 Check the installation.\n  \
install --staging DIR --manifest FILE [--external-database-file FILE] [--house-config-file FILE] [--omp-config FILE --client-config FILE --operator-principal NAME]\n  \
update                 Same options as install.\n  \
install-omp-adapter --source DIR\n  \
rollback-omp-adapter [--release-id ID]\n  \
rollback\n  \
uninstall\n  \
purge --confirm-data-loss\n  \
service                Entry point for the Windows service manager.\n  \
help";

/// Run one mode. The exit code is the mode's own; a failure that reaches
/// here becomes exit code 1 with its message on stderr.
pub fn run(arguments: Vec<String>) -> Result<ExitCode> {
    let mode = arguments.first().map(String::as_str).unwrap_or("");
    let rest: Vec<String> = arguments.iter().skip(1).cloned().collect();
    match mode {
        "" => app::run().map(|()| ExitCode::SUCCESS),
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(ExitCode::SUCCESS)
        }
        "service" => service::dispatch().map(|()| ExitCode::SUCCESS),
        "status" => status::run(),
        "start" => start::run(&rest),
        "keeper" => keeper::run(&rest),
        "chat" => chat::run(&rest).map(|()| ExitCode::SUCCESS),
        "install" | "update" | "install-omp-adapter" | "rollback-omp-adapter" | "doctor"
        | "rollback" | "uninstall" | "purge" => {
            manage::run(mode, &rest).map(|()| ExitCode::SUCCESS)
        }
        unknown => Err(anyhow::anyhow!("unknown mode {unknown:?}; run athanor help")),
    }
}

pub(crate) fn value(arguments: &[String], flag: &str) -> Result<String> {
    let index = arguments
        .iter()
        .position(|argument| argument == flag)
        .with_context(|| format!("{flag} is required"))?;
    arguments
        .get(index + 1)
        .cloned()
        .with_context(|| format!("{flag} requires a value"))
}

pub(crate) fn optional_value(arguments: &[String], flag: &str) -> Result<Option<String>> {
    arguments
        .iter()
        .position(|argument| argument == flag)
        .map(|index| {
            arguments
                .get(index + 1)
                .cloned()
                .with_context(|| format!("{flag} requires a value"))
        })
        .transpose()
}
