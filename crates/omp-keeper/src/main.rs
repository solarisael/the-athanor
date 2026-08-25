use anyhow::{Context, Result};
use omp_keeper::config;
use omp_keeper::keeper::{self, Outcome};
use std::process::ExitCode;

fn main() -> ExitCode {
    match keeper_main() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("omp-keeper stopped: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn keeper_main() -> Result<ExitCode> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let path = match config::config_path_from_args(arguments)? {
        Some(path) => path,
        None => config::default_config_path(
            &std::env::current_exe().context("the keeper executable path is unknown")?,
        ),
    };
    let config = config::load(&path)?;
    match keeper::run(&config)? {
        // an armed exit is not a failure, so the keeper reports 0 for it
        Outcome::Stopped { exit_code } => Ok(stopped_code(exit_code)),
        Outcome::Refused { message } | Outcome::Failed { message } => {
            println!("{message}");
            Ok(ExitCode::from(1))
        }
    }
}

fn stopped_code(exit_code: i32) -> ExitCode {
    if omp_keeper::decide::armed_exit_hint(Some(exit_code)) {
        return ExitCode::from(0);
    }
    match u8::try_from(exit_code) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::from(1),
    }
}
