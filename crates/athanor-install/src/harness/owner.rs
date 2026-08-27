use super::{
    config::{HarnessKind, HarnessRegistry, HarnessSpec, detail},
    omp::{self, OmpExit, OwnedOmp},
};
use anyhow::{Context, Result, bail};
use ::protocol::harness::{HarnessCommand, HarnessLifecycle, HarnessStatus};
use interactive_process::{InteractiveChild, InteractiveCommand};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

pub const STOP_TIMEOUT: Duration = Duration::from_secs(15);

struct OwnedChild {
    child: InteractiveChild,
    pid: u32,
}

enum OwnedHarness {
    Process(OwnedChild),
    Omp(OwnedOmp),
}

#[derive(Default)]
struct OwnedState {
    children: BTreeMap<String, OwnedHarness>,
    failures: BTreeMap<String, String>,
}

enum Observation {
    Alive,
    Absent,
    Ended(Option<String>),
}

pub struct HarnessOwner {
    registry: HarnessRegistry,
    token: String,
    program_root: PathBuf,
    state_root: PathBuf,
    state: Mutex<OwnedState>,
}

impl HarnessOwner {
    pub fn new(
        registry: HarnessRegistry,
        token: String,
        program_root: PathBuf,
        state_root: PathBuf,
    ) -> Self {
        Self {
            registry,
            token,
            program_root,
            state_root,
            state: Mutex::new(OwnedState::default()),
        }
    }

    pub fn registry(&self) -> &HarnessRegistry {
        &self.registry
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn authorized(&self, token: &str) -> bool {
        constant_time_eq(&self.token, token)
    }

    pub fn dispatch(&self, command: &HarnessCommand) -> Result<Vec<HarnessStatus>> {
        match command {
            HarnessCommand::List {} => Ok(self.list()),
            HarnessCommand::Start { harness_id } => self.start(harness_id),
            HarnessCommand::Stop { harness_id } => self.stop(harness_id),
            HarnessCommand::Restart { harness_id } => self.restart(harness_id),
        }
    }

    pub fn list(&self) -> Vec<HarnessStatus> {
        let mut state = self.state.lock().unwrap();
        self.registry
            .specs()
            .map(|spec| status(&mut state, spec))
            .collect()
    }

    pub fn start(&self, harness_id: &str) -> Result<Vec<HarnessStatus>> {
        let spec = self.spec(harness_id)?;
        let mut state = self.state.lock().unwrap();
        observe(&mut state, harness_id);
        if state.children.contains_key(harness_id) {
            bail!("harness {harness_id:?} is already running");
        }
        self.adopt(&mut state, harness_id, spec)?;
        Ok(vec![status(&mut state, spec)])
    }

    pub fn stop(&self, harness_id: &str) -> Result<Vec<HarnessStatus>> {
        let spec = self.spec(harness_id)?;
        let mut state = self.state.lock().unwrap();
        observe(&mut state, harness_id);
        stop_owned(&mut state, harness_id)?;
        Ok(vec![status(&mut state, spec)])
    }

    pub fn restart(&self, harness_id: &str) -> Result<Vec<HarnessStatus>> {
        let spec = self.spec(harness_id)?;
        let mut state = self.state.lock().unwrap();
        observe(&mut state, harness_id);
        if state.children.contains_key(harness_id) {
            stop_owned(&mut state, harness_id)?;
        }
        self.adopt(&mut state, harness_id, spec)?;
        Ok(vec![status(&mut state, spec)])
    }

    pub fn shutdown(&self) {
        let mut state = self.state.lock().unwrap();
        let owned: Vec<String> = state.children.keys().cloned().collect();
        for harness_id in owned {
            if let Err(error) = stop_owned(&mut state, &harness_id) {
                eprintln!("athanor: harness {harness_id:?} did not stop cleanly: {error:#}");
            }
        }
    }

    fn spec(&self, harness_id: &str) -> Result<&HarnessSpec> {
        self.registry
            .get(harness_id)
            .with_context(|| format!("unknown harness {harness_id:?}"))
    }

    fn adopt(&self, state: &mut OwnedState, harness_id: &str, spec: &HarnessSpec) -> Result<()> {
        let started = match &spec.kind {
            HarnessKind::Process(launch) => start_process(launch).map(|child| {
                let pid = child.id();
                OwnedHarness::Process(OwnedChild { child, pid })
            }),
            HarnessKind::Omp(launch) => {
                omp::start(launch, &self.program_root, &self.state_root, STOP_TIMEOUT)
                    .map(OwnedHarness::Omp)
            }
        };
        match started {
            Ok(owned) => {
                state.failures.remove(harness_id);
                state.children.insert(harness_id.to_owned(), owned);
                Ok(())
            }
            Err(error) => {
                state
                    .failures
                    .insert(harness_id.to_owned(), detail(format!("{error:#}")));
                Err(error)
            }
        }
    }
}

fn observe(state: &mut OwnedState, harness_id: &str) {
    let observation = match state.children.get_mut(harness_id) {
        None => Observation::Absent,
        Some(OwnedHarness::Process(owned)) => match owned.child.try_wait() {
            Ok(None) => Observation::Alive,
            Ok(Some(status)) if status.success() => Observation::Ended(None),
            Ok(Some(status)) => Observation::Ended(Some(detail(format!("exited with {status}")))),
            Err(error) => {
                Observation::Ended(Some(detail(format!("exit status unavailable: {error}"))))
            }
        },
        Some(OwnedHarness::Omp(owned)) => match owned.observe() {
            None => Observation::Alive,
            Some(OmpExit::Stopped) => Observation::Ended(None),
            Some(OmpExit::Failed(message)) => Observation::Ended(Some(detail(message))),
        },
    };
    if let Observation::Ended(failure) = observation {
        state.children.remove(harness_id);
        match failure {
            Some(text) => state.failures.insert(harness_id.to_owned(), text),
            None => state.failures.remove(harness_id),
        };
    }
}

fn status(state: &mut OwnedState, spec: &HarnessSpec) -> HarnessStatus {
    observe(state, &spec.harness_id);
    let pid = state
        .children
        .get(&spec.harness_id)
        .and_then(|owned| match owned {
            OwnedHarness::Process(owned) => Some(owned.pid),
            OwnedHarness::Omp(owned) => owned.pid(),
        });
    let failure = state.failures.get(&spec.harness_id).cloned();
    let lifecycle = match (pid.is_some(), failure.is_some()) {
        (true, _) => HarnessLifecycle::Running,
        (false, true) => HarnessLifecycle::Failed,
        (false, false) => HarnessLifecycle::Stopped,
    };
    HarnessStatus {
        harness_id: spec.harness_id.clone(),
        label: spec.label.clone(),
        lifecycle,
        pid,
        detail: failure,
    }
}

fn stop_owned(state: &mut OwnedState, harness_id: &str) -> Result<()> {
    let owned = state
        .children
        .get_mut(harness_id)
        .with_context(|| format!("this Athanor owns no running harness for {harness_id:?}"))?;
    match owned {
        OwnedHarness::Process(owned) => stop_process(owned, harness_id)?,
        OwnedHarness::Omp(owned) => owned.stop(STOP_TIMEOUT)?,
    }
    state.children.remove(harness_id);
    state.failures.remove(harness_id);
    Ok(())
}

fn start_process(launch: &super::config::HarnessLaunch) -> Result<InteractiveChild> {
    let mut command = InteractiveCommand::new(&launch.program);
    command
        .args(&launch.arguments)
        .current_dir(&launch.workspace);
    command
        .spawn()
        .with_context(|| format!("start harness {}", launch.program.display()))
}

fn stop_process(owned: &mut OwnedChild, harness_id: &str) -> Result<()> {
    if owned.child.try_wait()?.is_none() {
        if let Err(error) = owned.child.terminate() {
            if owned.child.try_wait()?.is_none() {
                return Err(error).with_context(|| format!("stop harness {harness_id:?}"));
            }
        }
    }
    let deadline = Instant::now() + STOP_TIMEOUT;
    while owned.child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            bail!("harness {harness_id:?} is still alive after the stop request");
        }
        thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}

fn constant_time_eq(expected: &str, provided: &str) -> bool {
    let (expected, provided) = (expected.as_bytes(), provided.as_bytes());
    if expected.len() != provided.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in expected.iter().zip(provided) {
        difference |= left ^ right;
    }
    difference == 0
}
