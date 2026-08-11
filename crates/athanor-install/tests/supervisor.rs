use anyhow::Result;
use athanor_install::supervisor::{
    ProcessSpec, Processes, Readiness, STOP_TIMEOUT, Supervisor, SupervisorConfig, runtime_plan,
};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Default)]
struct FakeProcesses {
    events: RefCell<Vec<String>>,
    timeout: RefCell<Option<String>>,
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

fn spec(name: &'static str, port: u16) -> ProcessSpec {
    ProcessSpec {
        name,
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

#[test]
fn delivery_readiness_executes_the_real_health_contract() -> Result<()> {
    let config = SupervisorConfig {
        database_mode: "managed".into(),
        database_host: "127.0.0.1".into(),
        database_port: 5432,
        nats_host: "127.0.0.1".into(),
        nats_port: 4222,
        room: "kintsu".into(),
        house_id: "house".into(),
        spirit: "Kintsu".into(),
        session: "managed".into(),
    };
    let plan = runtime_plan(
        &PathBuf::from("release"),
        &PathBuf::from("data"),
        &config,
        "postgresql://test",
        "token",
    )?;
    let delivery = plan
        .iter()
        .find(|spec| spec.name == "delivery")
        .expect("delivery process");
    match &delivery.readiness {
        Readiness::Command {
            executable,
            arguments,
            environment,
        } => {
            assert_eq!(
                executable,
                &PathBuf::from("release/bin/athanor-house-delivery.exe")
            );
            assert_eq!(arguments, &[std::ffi::OsString::from("health")]);
            assert_eq!(
                environment.get("DATABASE_URL").map(String::as_str),
                Some("postgresql://test")
            );
            assert_eq!(
                environment.get("SOLARISAEL_NATS_URL").map(String::as_str),
                Some("nats://127.0.0.1:4222")
            );
        }
        other => panic!("delivery readiness must be its real health command, got {other:?}"),
    }
    Ok(())
}
