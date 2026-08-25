use std::io::Write;

fn main() {
    if let Ok(path) = std::env::var("FAKE_OMP_RUNS") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("fake omp run log");
        writeln!(file, "run").expect("fake omp run line");
    }
    println!("fake omp: arming an exit");
    std::process::exit(87);
}
