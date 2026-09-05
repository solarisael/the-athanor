use anyhow::{Context, Result};
use omp_keeper::config;
use omp_keeper::keeper::{self, Outcome};
use std::process::ExitCode;

/// The keeper reached `Stopped` with the code of a child it did not relaunch.
/// When that code is the armed 87, the session asked the House for a restart
/// and never got one: the House had nothing claimable, or the claim was not
/// this keeper's to take. The shell must not hear success for that, and it
/// must not hear 87 either, because 87 is the child's request, not the
/// keeper's verdict. A relaunch that verified never lands here: the loop
/// carries on and reports the exit of the child that followed it, so an
/// ordinary quit after a verified restart still reports 0.
const ARMED_EXIT_UNSERVED: u8 = 88;

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
        Outcome::Stopped { exit_code } => Ok(ExitCode::from(stopped_code(exit_code))),
        Outcome::Refused { message } | Outcome::Failed { message } => {
            println!("{message}");
            Ok(ExitCode::from(1))
        }
    }
}

/// The one exit-code decision, kept as a byte so it can be read and proven
/// without a process.
fn stopped_code(exit_code: i32) -> u8 {
    if omp_keeper::decide::armed_exit_hint(Some(exit_code)) {
        return ARMED_EXIT_UNSERVED;
    }
    u8::try_from(exit_code).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{ARMED_EXIT_UNSERVED, stopped_code};

    #[test]
    fn an_unserved_armed_exit_is_not_success() {
        assert_eq!(
            stopped_code(87),
            ARMED_EXIT_UNSERVED,
            "an armed exit the keeper did not relaunch is reported, never mapped to 0"
        );
        assert_ne!(stopped_code(87), 0);
        assert_ne!(
            stopped_code(87), 87,
            "87 is the child's request; the keeper's verdict carries its own code"
        );
    }

    #[test]
    fn an_ordinary_child_exit_travels_unchanged() {
        for code in [0_i32, 1, 2, 88, 255] {
            assert_eq!(
                u32::from(stopped_code(code)),
                u32::try_from(code).expect("a byte-wide child code"),
                "the keeper reports the code of a child it did not relaunch: {code}"
            );
        }
    }

    #[test]
    fn a_child_code_outside_one_byte_reports_failure() {
        for code in [-1_i32, 256, 3_000_000] {
            assert_eq!(
                stopped_code(code),
                1,
                "a code no shell can carry reports failure: {code}"
            );
        }
    }
}
