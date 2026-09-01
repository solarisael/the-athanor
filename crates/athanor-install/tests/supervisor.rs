use anyhow::Result;
use athanor_install::supervisor::{ProcessSpec, Processes, Readiness, STOP_TIMEOUT, Supervisor};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Default)]
struct FakeProcesses {
    events: RefCell<Vec<String>>,
    timeout: RefCell<Option<String>>,
    ready_failure: RefCell<Option<String>>,
}
impl Processes for FakeProcesses {
    fn spawn(&self, spec: &ProcessSpec) -> Result<u32> {
        self.events
            .borrow_mut()
            .push(format!("spawn:{}", spec.name));
        Ok(1)
    }
    fn ready(&self, name: &str, _: &Readiness) -> Result<bool> {
        self.events.borrow_mut().push(format!("ready:{name}"));
        if self.ready_failure.borrow().as_deref() == Some(name) {
            anyhow::bail!("managed child {name} exited before readiness");
        }
        Ok(true)
    }
    fn request_stop(&self, name: &str) -> Result<()> {
        self.events.borrow_mut().push(format!("stop:{name}"));
        Ok(())
    }
    fn wait_exit(&self, name: &str, timeout: Duration) -> Result<bool> {
        assert_eq!(timeout, STOP_TIMEOUT);
        self.events.borrow_mut().push(format!("wait:{name}"));
        Ok(self.timeout.borrow().as_deref() != Some(name))
    }
    fn kill_verified(&self, name: &str) -> Result<()> {
        self.events.borrow_mut().push(format!("kill:{name}"));
        Ok(())
    }
}

fn spec(name: &str, port: u16) -> ProcessSpec {
    ProcessSpec {
        name: name.into(),
        executable: PathBuf::from(format!("{name}.exe")),
        arguments: Vec::new(),
        environment: BTreeMap::new(),
        readiness: Readiness::Tcp(format!("127.0.0.1:{port}").parse().unwrap()),
    }
}

#[test]
fn reports_checkpoints_only_after_each_child_is_spawned_and_ready() -> Result<()> {
    let processes = FakeProcesses::default();
    let supervisor = Supervisor { processes };
    let specs = [
        spec("postgresql", 5432),
        spec("nats", 4222),
        spec("delivery", 4223),
        spec("host", 8787),
    ];
    let mut checkpoints = Vec::new();
    supervisor.run(&specs, |number, name| {
        checkpoints.push((number, name.to_owned()));
        Ok(())
    })?;
    assert_eq!(
        checkpoints,
        [
            (1, "postgresql".into()),
            (2, "nats".into()),
            (3, "delivery".into()),
            (4, "host".into())
        ]
    );
    assert_eq!(
        supervisor.processes.events.borrow().as_slice(),
        [
            "spawn:postgresql",
            "ready:postgresql",
            "spawn:nats",
            "ready:nats",
            "spawn:delivery",
            "ready:delivery",
            "spawn:host",
            "ready:host",
        ]
    );
    Ok(())
}

#[test]
fn readiness_failure_stops_every_started_child_in_reverse_order() {
    let processes = FakeProcesses::default();
    *processes.ready_failure.borrow_mut() = Some("delivery".into());
    let supervisor = Supervisor { processes };
    let specs = [
        spec("nats", 4222),
        spec("delivery", 4223),
        spec("host", 8787),
    ];

    let error = supervisor.run(&specs, |_, _| Ok(())).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("managed child delivery exited before readiness")
    );
    assert_eq!(
        supervisor.processes.events.borrow().as_slice(),
        [
            "spawn:nats",
            "ready:nats",
            "spawn:delivery",
            "ready:delivery",
            "stop:delivery",
            "wait:delivery",
            "stop:nats",
            "wait:nats",
        ]
    );
}

#[test]
fn shutdown_is_reverse_dependency_order_and_only_verified_timeouts_are_killed() -> Result<()> {
    let processes = FakeProcesses::default();
    *processes.timeout.borrow_mut() = Some("nats".into());
    let supervisor = Supervisor { processes };
    let specs = [
        spec("postgresql", 5432),
        spec("nats", 4222),
        spec("delivery", 4223),
        spec("host", 8787),
    ];
    supervisor.stop(&specs)?;
    assert_eq!(
        supervisor.processes.events.borrow().as_slice(),
        [
            "stop:host",
            "wait:host",
            "stop:delivery",
            "wait:delivery",
            "stop:nats",
            "wait:nats",
            "kill:nats",
            "stop:postgresql",
            "wait:postgresql",
        ]
    );
    Ok(())
}

#[test]
fn graceful_requests_never_use_the_hard_kill_boundary_when_children_exit() -> Result<()> {
    let processes = FakeProcesses::default();
    let supervisor = Supervisor { processes };
    let specs = [
        spec("postgresql", 5432),
        spec("nats", 4222),
        spec("delivery", 4223),
        spec("host", 8787),
    ];
    supervisor.stop(&specs)?;
    let events = supervisor.processes.events.borrow();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("stop:"))
            .count(),
        4
    );
    assert!(!events.iter().any(|event| event.starts_with("kill:")));
    assert_eq!(events.first().map(String::as_str), Some("stop:host"));
    assert_eq!(events.last().map(String::as_str), Some("wait:postgresql"));
    Ok(())
}

#[test]
fn external_database_plan_may_omit_postgresql_but_not_other_children() -> Result<()> {
    let processes = FakeProcesses::default();
    let supervisor = Supervisor { processes };
    let specs = [
        spec("nats", 4222),
        spec("delivery", 4223),
        spec("host", 8787),
    ];
    supervisor.run(&specs, |_, _| Ok(()))?;
    assert!(
        !supervisor
            .processes
            .events
            .borrow()
            .iter()
            .any(|event| event.contains("postgresql"))
    );
    Ok(())
}
