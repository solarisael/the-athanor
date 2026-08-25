use crate::protocol::{PROTOCOL_VERSION, ProtocolErrorBody, RequestEnvelope, ResponseEnvelope};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub enum Answer<T> {
    Ok(T),
    Refused(ProtocolErrorBody),
}

pub struct SubstrateSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl SubstrateSession {
    pub fn start(executable: &Path) -> Result<Self> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("substrate could not start: {}", executable.display()))?;
        let stdin = child.stdin.take().context("substrate stdin was not piped")?;
        let stdout = child.stdout.take().context("substrate stdout was not piped")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    // # enough: one request in flight, matched by id; the keeper never pipelines
    pub fn call<P, T>(&mut self, method: &str, params: &P) -> Result<Answer<T>>
    where
        P: Serialize,
        T: DeserializeOwned,
    {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let line = serde_json::to_string(&RequestEnvelope {
            protocol: PROTOCOL_VERSION,
            id: &id,
            method,
            params,
        })
        .with_context(|| format!("{method} request could not be encoded"))?;
        writeln!(self.stdin, "{line}").with_context(|| format!("{method} request could not be written"))?;
        self.stdin
            .flush()
            .with_context(|| format!("{method} request could not be flushed"))?;

        let mut response = String::new();
        loop {
            response.clear();
            let read = self
                .stdout
                .read_line(&mut response)
                .with_context(|| format!("{method} response could not be read"))?;
            if read == 0 {
                bail!("substrate closed the connection before answering {method}");
            }
            if !response.trim().is_empty() {
                break;
            }
        }
        let envelope: ResponseEnvelope = serde_json::from_str(response.trim())
            .with_context(|| format!("{method} response was not a protocol envelope"))?;
        if envelope.id != id {
            bail!("{method} response carried id {} instead of {id}", envelope.id);
        }
        match (envelope.result, envelope.error) {
            (Some(result), None) => Ok(Answer::Ok(
                serde_json::from_value(result)
                    .with_context(|| format!("{method} result did not match the restart door"))?,
            )),
            (None, Some(error)) => Ok(Answer::Refused(error)),
            _ => bail!("{method} response must carry exactly one of result or error"),
        }
    }

    pub fn close(mut self) -> Result<()> {
        drop(self.stdin);
        self.child.wait().context("substrate did not exit")?;
        Ok(())
    }
}
