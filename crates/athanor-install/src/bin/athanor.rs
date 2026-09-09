use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match athanor_install::cli::run(arguments) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("athanor: {error:#}");
            ExitCode::from(1)
        }
    }
}
