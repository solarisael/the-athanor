//! The loopback control wire: one bound ephemeral port, one request and one
//! response for each connection, and the token check that stands between a
//! caller and the owner's handles. Authority stays in `owner`; this file only
//! carries and refuses.

use super::{config::detail, owner::HarnessOwner};
use anyhow::{Context, Result};
use ::protocol::harness::{HarnessControlRequest, HarnessControlResponse};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

pub const CONTROL_ADDR_ENV: &str = "ATHANOR_CONTROL_ADDR";
pub const CONTROL_TOKEN_ENV: &str = "ATHANOR_CONTROL_TOKEN";

const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const UNNAMED_REQUEST: &str = "unknown";

pub struct ControlServer {
    pub address: SocketAddr,
}

impl ControlServer {
    /// Binds an ephemeral loopback port and answers newline-delimited JSON, one
    /// request and one response for each connection. The port and the token reach
    /// the GUI as ATHANOR_CONTROL_ADDR and ATHANOR_CONTROL_TOKEN.
    pub fn bind(owner: Arc<HarnessOwner>) -> Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .context("bind the harness control socket")?;
        let address = listener
            .local_addr()
            .context("read the harness control address")?;
        thread::Builder::new()
            .name("athanor-control".into())
            .spawn(move || accept(listener, owner))
            .context("start the harness control thread")?;
        Ok(Self { address })
    }
}

fn accept(listener: TcpListener, owner: Arc<HarnessOwner>) {
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let owner = Arc::clone(&owner);
                if let Err(error) = thread::Builder::new()
                    .name("athanor-control-request".into())
                    .spawn(move || {
                        if let Err(error) = answer_once(stream, owner.as_ref()) {
                            eprintln!("athanor: harness control connection failed: {error:#}");
                        }
                    })
                {
                    eprintln!("athanor: harness control worker refused: {error}");
                }
            }
            Err(error) => eprintln!("athanor: harness control accept failed: {error}"),
        }
    }
}

fn answer_once(stream: TcpStream, owner: &HarnessOwner) -> Result<()> {
    stream
        .set_read_timeout(Some(CONTROL_TIMEOUT))
        .context("set the harness control read timeout")?;
    stream
        .set_write_timeout(Some(CONTROL_TIMEOUT))
        .context("set the harness control write timeout")?;
    let mut writer = stream
        .try_clone()
        .context("clone the harness control stream")?;
    let mut line = String::new();
    BufReader::new(stream.take(MAX_REQUEST_BYTES))
        .read_line(&mut line)
        .context("read the harness control request")?;
    let response = answer(owner, &line);
    let mut payload =
        serde_json::to_vec(&response).context("encode the harness control response")?;
    payload.push(b'\n');
    writer
        .write_all(&payload)
        .context("write the harness control response")?;
    writer.flush().context("flush the harness control response")
}

fn answer(owner: &HarnessOwner, line: &str) -> HarnessControlResponse {
    let request: HarnessControlRequest = match serde_json::from_str(line.trim()) {
        Ok(request) => request,
        Err(error) => {
            return HarnessControlResponse::refusal(
                UNNAMED_REQUEST.into(),
                detail(format!("harness control request is not readable: {error}")),
            );
        }
    };
    let request_id = if request.request_id.trim().is_empty() {
        UNNAMED_REQUEST.to_owned()
    } else {
        request.request_id.clone()
    };
    if let Err(error) = request.validate() {
        return HarnessControlResponse::refusal(request_id, detail(error));
    }
    if !owner.authorized(&request.token) {
        return HarnessControlResponse::refusal(
            request_id,
            "the harness control token is not this owner's token",
        );
    }
    match owner.dispatch(&request.command) {
        Ok(harnesses) => HarnessControlResponse::success(request_id, harnesses),
        Err(error) => HarnessControlResponse::refusal(request_id, detail(format!("{error:#}"))),
    }
}
