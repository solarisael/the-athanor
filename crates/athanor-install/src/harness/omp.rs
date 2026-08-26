use super::config::{ConsoleMode as HarnessConsoleMode, HarnessLaunch};
use anyhow::{Context, Result, bail};
use omp_keeper::{
    config::{ConsoleMode, DEFAULT_CLAIMANT, DEFAULT_WATCH_INTERVAL_SECS, KeeperConfig},
    control::{KeeperObserver, StopControl},
    keeper::{ControlledOutcome, Outcome, run_controlled},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) enum OmpExit {
    Stopped,
    Failed(String),
}

pub(super) struct OwnedOmp {
    control: StopControl,
    pid: Arc<AtomicU32>,
    thread: Option<JoinHandle<Result<ControlledOutcome>>>,
}

struct PidObserver {
    pid: Arc<AtomicU32>,
}

impl KeeperObserver for PidObserver {
    fn child_started(&self, pid: u32) {
        self.pid.store(pid, Ordering::Release);
    }

    fn child_stopped(&self, pid: u32) {
        let _ = self
            .pid
            .compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

pub(super) fn start(
    launch: &HarnessLaunch,
    program_root: &PathBuf,
    state_root: &PathBuf,
    timeout: Duration,
) -> Result<OwnedOmp> {
    let program = launch
        .program
        .to_str()
        .context("the OMP program path is not Unicode")?
        .to_owned();
    let workspace = launch
        .workspace
        .to_str()
        .context("the OMP workspace path is not Unicode")?
        .to_owned();
    let mut omp_launch = Vec::with_capacity(launch.arguments.len() + 1);
    omp_launch.push(program);
    omp_launch.extend(launch.arguments.iter().cloned());
    let config = KeeperConfig {
        omp_launch,
        workspace,
        program_root: program_root.clone(),
        state_root: state_root.clone(),
        claimant: DEFAULT_CLAIMANT.to_owned(),
        watch_interval_secs: DEFAULT_WATCH_INTERVAL_SECS,
        capability: None,
        capability_path: Some(launch.workspace.join(".omp/runtime/restart-capability")),
    };
    config.validate()?;

    let console = match launch.console {
        HarnessConsoleMode::NewWindow => ConsoleMode::NewWindow,
    };
    let control = StopControl::new();
    let thread_control = control.clone();
    let pid = Arc::new(AtomicU32::new(0));
    let observer = PidObserver {
        pid: Arc::clone(&pid),
    };
    let thread =
        thread::spawn(move || run_controlled(&config, console, &thread_control, &observer));
    let owned = OwnedOmp {
        control,
        pid,
        thread: Some(thread),
    };
    let deadline = Instant::now() + timeout;
    while owned.pid().is_none() {
        if owned.is_finished() {
            let exit = owned.into_exit();
            match exit {
                OmpExit::Stopped => bail!("OMP stopped before it reported a child pid"),
                OmpExit::Failed(message) => bail!("{message}"),
            }
        }
        if Instant::now() >= deadline {
            owned.control.request_stop();
            bail!("OMP did not report a child pid before the start timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(owned)
}

impl OwnedOmp {
    pub(super) fn pid(&self) -> Option<u32> {
        match self.pid.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }

    pub(super) fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub(super) fn observe(&mut self) -> Option<OmpExit> {
        self.is_finished().then(|| self.take_exit())
    }

    pub(super) fn stop(&mut self, timeout: Duration) -> Result<()> {
        self.control.request_stop();
        let deadline = Instant::now() + timeout;
        while !self.is_finished() {
            if Instant::now() >= deadline {
                bail!("OMP keeper thread is still alive after the stop request");
            }
            thread::sleep(Duration::from_millis(25));
        }
        match self.take_exit() {
            OmpExit::Stopped => Ok(()),
            OmpExit::Failed(message) => bail!("{message}"),
        }
    }

    fn into_exit(mut self) -> OmpExit {
        self.take_exit()
    }

    fn take_exit(&mut self) -> OmpExit {
        let Some(thread) = self.thread.take() else {
            return OmpExit::Stopped;
        };
        match thread.join() {
            Ok(Ok(ControlledOutcome::Stopped)) => OmpExit::Stopped,
            Ok(Ok(ControlledOutcome::Completed(Outcome::Stopped { .. }))) => OmpExit::Stopped,
            Ok(Ok(ControlledOutcome::Completed(Outcome::Refused { message })))
            | Ok(Ok(ControlledOutcome::Completed(Outcome::Failed { message }))) => {
                OmpExit::Failed(message)
            }
            Ok(Err(error)) => OmpExit::Failed(format!("OMP keeper failed: {error:#}")),
            Err(_) => OmpExit::Failed("OMP keeper thread panicked".to_owned()),
        }
    }
}
