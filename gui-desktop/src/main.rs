mod proxy;

use std::{env, path::PathBuf};
use anyhow::{Context, bail};
use serde::Deserialize;

const HELP: &str = "Pulse — The Athanor desktop app\nUsage: pulse [--room <key>] [--port <n>] [--host-port <n>] [--dev-dir <path>] [--serve-only]\n\nDefaults: room kodo, port 4175. Environment: PULSE_ROOM, PULSE_PORT, PULSE_HOST_PORT.\n--help  Show this usage.";

struct Options {
    room: String,
    port: u16,
    host_port: Option<u16>,
    dev_dir: Option<PathBuf>,
    serve_only: bool,
}

impl Options {
    fn parse() -> anyhow::Result<Option<Self>> {
        let mut options = Self {
            room: env::var("PULSE_ROOM").unwrap_or_else(|_| "kodo".into()),
            port: env::var("PULSE_PORT").unwrap_or_else(|_| "4175".into()).parse().context("invalid PULSE_PORT")?,
            host_port: env::var("PULSE_HOST_PORT").ok().map(|value| value.parse()).transpose().context("invalid PULSE_HOST_PORT")?,
            dev_dir: None,
            serve_only: false,
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => { println!("{HELP}"); return Ok(None); }
                "--serve-only" => options.serve_only = true,
                "--room" => options.room = args.next().context("--room requires a value")?,
                "--port" => options.port = args.next().context("--port requires a value")?.parse().context("invalid --port")?,
                "--host-port" => options.host_port = Some(args.next().context("--host-port requires a value")?.parse().context("invalid --host-port")?),
                "--dev-dir" => options.dev_dir = Some(args.next().context("--dev-dir requires a value")?.into()),
                _ => bail!("unknown argument: {arg}"),
            }
        }
        if options.port == 0 || options.host_port == Some(0) { bail!("ports must be between 1 and 65535"); }
        Ok(Some(options))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Runtime { host_port: u16, rooms: Vec<Room> }
#[derive(Deserialize)]
struct Room { room: String }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Secrets { host_token: String }

fn main() -> anyhow::Result<()> {
    let Some(options) = Options::parse()? else { return Ok(()); };
    let config: Runtime = serde_json::from_slice(&std::fs::read("C:/ProgramData/Solarisael/Athanor/config/runtime.json").context("read runtime.json")?)?;
    if !config.rooms.iter().any(|room| room.room == options.room) { bail!("room {} is not in runtime.json", options.room); }
    let secrets: Secrets = serde_json::from_slice(&std::fs::read("C:/ProgramData/Solarisael/Athanor/secrets/runtime-secrets.json").context("read runtime-secrets.json")?)?;
    let host_port = options.host_port.unwrap_or(config.host_port);
    if host_port == 0 { bail!("no usable hostPort"); }
    let app_exe = env::var_os("ATHANOR_APP_EXE").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Program Files/Solarisael/Athanor/bin/athanor.exe"));
    let proxy = proxy::Proxy::new(host_port, &options.room, secrets.host_token, options.dev_dir, app_exe)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, options.port)))?;
    println!("prototype on http://127.0.0.1:{} · live House reads via /room/{} on Host :{host_port}", options.port, options.room);
    if options.serve_only {
        runtime.block_on(async { axum::serve(listener, proxy.router()).with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; }).await })?;
        return Ok(());
    }
    let server = runtime.spawn(async move { axum::serve(listener, proxy.router()).await });
    let url = format!("http://127.0.0.1:{}", options.port).parse()?;
    let result = tauri::Builder::default().setup(move |app| {
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External(url))
            .title("The Athanor — Pulse").inner_size(1400.0, 900.0).build()?;
        Ok(())
    }).run(tauri::generate_context!());
    server.abort();
    result?;
    Ok(())
}
