use anyhow::{Result, bail};
use athanor_install::app;
use std::env;

fn main() -> Result<()> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let room = match arguments.as_slice() {
        [] => None,
        [flag, value] if flag == "--room" => Some(value.clone()),
        _ => bail!("usage: athanor [--room ROOM]"),
    };
    app::run(room)
}
