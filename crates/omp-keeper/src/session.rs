//! The keeper's line to the House: newline-delimited JSON to a substrate child.
//!
//! The concern: telling a House that answered apart from a House that was not
//! there. An answer is a result or a refusal, and both are decisions. Silence
//! is neither: the substrate child died mid-ask, or its Postgres could not be
//! reached (`database`), so no decision was made. Silence is its own answer,
//! `Answer::Unreachable`, and never an error, so no caller can mistake a blink
//! of the House for the end of the restart.

use crate::protocol::{PROTOCOL_VERSION, ProtocolErrorBody, RequestEnvelope, ResponseEnvelope};
use crate::resolve::resolve_substrate_exe;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub enum Answer<T> {
    Ok(T),
    Refused(ProtocolErrorBody),
    /// The House made no decision, and why, in one line.
    Unreachable(String),
}

impl<T> Answer<T> {
    pub fn map<U>(self, into: impl FnOnce(T) -> U) -> Answer<U> {
        match self {
            Self::Ok(value) => Answer::Ok(into(value)),
            Self::Refused(refusal) => Answer::Refused(refusal),
            Self::Unreachable(reason) => Answer::Unreachable(reason),
        }
    }
}

/// One substrate child for as long as its line holds. A child that answers
/// `database` is kept: it is alive and reaches for Postgres again on the next
/// ask. A child whose line breaks is put down, and the next ask starts a fresh
/// one from the release pointer as it stands then.
pub struct HouseLine<'a> {
    program_root: &'a Path,
    state_root: &'a Path,
    session: Option<SubstrateSession>,
}

impl<'a> HouseLine<'a> {
    /// No child starts until the first ask.
    pub fn new(program_root: &'a Path, state_root: &'a Path) -> Self {
        Self {
            program_root,
            state_root,
            session: None,
        }
    }

    /// A substrate that cannot be resolved or spawned is an install fault, not
    /// a House that blinked, so it stays an error.
    pub fn call<P, T>(&mut self, method: &str, params: &P) -> Result<Answer<T>>
    where
        P: Serialize,
        T: DeserializeOwned,
    {
        let session = match self.session.as_mut() {
            Some(session) => session,
            None => {
                let executable = resolve_substrate_exe(self.program_root)
                    .context("the current substrate could not be resolved")?;
                self.session
                    .insert(SubstrateSession::start(&executable, self.state_root)?)
            }
        };
        let answer = session.call(method, params)?;
        if session.line_broken
            && let Some(broken) = self.session.take()
        {
            broken.put_down();
        }
        Ok(answer)
    }

    pub fn close(mut self) -> Result<()> {
        match self.session.take() {
            Some(session) => session.close(),
            None => Ok(()),
        }
    }
}

struct SubstrateSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    /// The child stopped reading or writing mid-ask; nothing more will cross.
    line_broken: bool,
}

impl SubstrateSession {
    fn start(executable: &Path, state_root: &Path) -> Result<Self> {
        let mut child = Command::new(executable)
            .env("ATHANOR_STATE_DIR", state_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("substrate could not start: {}", executable.display()))?;
        let stdin = child
            .stdin
            .take()
            .context("substrate stdin was not piped")?;
        let stdout = child
            .stdout
            .take()
            .context("substrate stdout was not piped")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            line_broken: false,
        })
    }

    // # enough: one request in flight, matched by id; the keeper never pipelines
    fn call<P, T>(&mut self, method: &str, params: &P) -> Result<Answer<T>>
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
        if let Err(error) = writeln!(self.stdin, "{line}").and_then(|()| self.stdin.flush()) {
            return Ok(self.line_broke(format!(
                "the substrate stopped listening before {method}: {error}"
            )));
        }

        let mut response = String::new();
        loop {
            response.clear();
            match self.stdout.read_line(&mut response) {
                Ok(0) => {
                    return Ok(self.line_broke(format!(
                        "the substrate closed the connection before answering {method}"
                    )));
                }
                Ok(_) if response.trim().is_empty() => {}
                Ok(_) => break,
                Err(error) => {
                    return Ok(self.line_broke(format!(
                        "the {method} answer could not be read: {error}"
                    )));
                }
            }
        }
        let envelope: ResponseEnvelope = serde_json::from_str(response.trim())
            .with_context(|| format!("{method} response was not a protocol envelope"))?;
        if envelope.id != id {
            bail!(
                "{method} response carried id {} instead of {id}",
                envelope.id
            );
        }
        match (envelope.result, envelope.error) {
            (Some(result), None) => Ok(Answer::Ok(
                serde_json::from_value(result)
                    .with_context(|| format!("{method} result did not match the restart door"))?,
            )),
            (None, Some(error)) if error.is_house_unreachable() => Ok(Answer::Unreachable(
                format!("{method} answered {}: {}", error.code, error.message),
            )),
            (None, Some(error)) => Ok(Answer::Refused(error)),
            _ => bail!("{method} response must carry exactly one of result or error"),
        }
    }

    fn line_broke<T>(&mut self, reason: String) -> Answer<T> {
        self.line_broken = true;
        Answer::Unreachable(reason)
    }

    fn close(mut self) -> Result<()> {
        drop(self.stdin);
        let status = self.child.wait().context("substrate did not exit")?;
        if !status.success() {
            bail!("substrate exited unsuccessfully: {status}");
        }
        Ok(())
    }

    /// A child whose line broke is killed and reaped, never waited on: it may
    /// be stuck on the very pipe that broke. One that already exited is what
    /// we wanted.
    fn put_down(mut self) {
        let _ = self.child.kill();
        if let Err(error) = self.child.wait() {
            eprintln!("omp-keeper: a silent substrate child could not be reaped: {error}");
        }
    }
}
