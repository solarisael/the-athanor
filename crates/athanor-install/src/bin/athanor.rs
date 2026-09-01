use anyhow::{Result, bail};
use athanor_install::app;
use std::env;

fn main() -> Result<()> {
    if env::args_os().nth(1).is_some() {
        bail!("usage: athanor");
    }
    app::run()
}
