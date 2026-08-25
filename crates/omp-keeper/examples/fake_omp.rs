//! A stand-in for omp: it records that it ran, then either arms an exit or
//! stays alive so the keeper's watch has something to watch.
//!
//! `FAKE_OMP_SLEEP_SECS` makes a run stay alive instead of exiting, and
//! `FAKE_OMP_SLEEP_FROM_RUN` says which run (1-based) starts doing that. The
//! deadline tests need a child that overstays: one that exits at once can only
//! ever prove the exit path.

use std::io::Write;

fn main() {
    let run = record_run();
    let sleep_secs = number("FAKE_OMP_SLEEP_SECS");
    let sleep_from_run = number("FAKE_OMP_SLEEP_FROM_RUN").unwrap_or(1);
    if let Some(sleep_secs) = sleep_secs {
        if run >= sleep_from_run {
            println!("fake omp: staying alive for {sleep_secs}s (run {run})");
            let _ = std::io::stdout().flush();
            std::thread::sleep(std::time::Duration::from_secs(sleep_secs));
            // Outliving the sleep means nobody killed this child. The smoke reads
            // this file to catch an omp the keeper orphaned instead of putting down.
            if let Ok(path) = std::env::var("FAKE_OMP_SURVIVED") {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .expect("fake omp survival log");
                writeln!(file, "survived run {run}").expect("fake omp survival line");
            }
            return;
        }
    }
    println!("fake omp: arming an exit (run {run})");
    std::process::exit(87);
}

/// The run number is the keeper's own count: this fixture appends one line per
/// launch, and the smoke tests read the same file.
fn record_run() -> u64 {
    let Ok(path) = std::env::var("FAKE_OMP_RUNS") else {
        return 1;
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("fake omp run log");
    writeln!(file, "run").expect("fake omp run line");
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .count() as u64
}

fn number(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse().ok()
}
