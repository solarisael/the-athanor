use anyhow::Result;
use athanor_install::supervisor::{
    HostRoomConfig, ProcessSpec, Processes, Readiness, STOP_TIMEOUT, Supervisor, SupervisorConfig,
    runtime_plan,
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
fn delivery_readiness_uses_a_fresh_process_owned_marker() -> Result<()> {
    let config = SupervisorConfig {
        database_mode: "managed".into(),
        database_host: "127.0.0.1".into(),
        database_port: 5432,
        nats_host: "127.0.0.1".into(),
        nats_port: 4222,
        rooms_root: PathBuf::from("rooms")
            .canonicalize()
            .unwrap_or_else(|_| std::env::current_dir().unwrap().join("rooms")),
        house_id: "solarisael".into(),
        rooms: vec![HostRoomConfig {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            port: 8787,
        }],
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
    let ready_path = PathBuf::from("data").join("state/delivery.ready");
    let ready_display = ready_path.display().to_string();
    assert_eq!(delivery.readiness, Readiness::File(ready_path.clone()));
    assert_eq!(
        delivery
            .environment
            .get("ATHANOR_DELIVERY_READY_FILE")
            .map(String::as_str),
        Some(ready_display.as_str())
    );
    assert_eq!(
        delivery.environment.get("DATABASE_URL").map(String::as_str),
        Some("postgresql://test")
    );
    assert_eq!(
        delivery
            .environment
            .get("SOLARISAEL_NATS_URL")
            .map(String::as_str),
        Some("nats://127.0.0.1:4222")
    );
    Ok(())
}

#[test]
fn external_plan_emits_distinct_room_hosts_without_postgresql() -> Result<()> {
    let rooms_root = std::env::current_dir()?.join("rooms");
    let config = SupervisorConfig {
        database_mode: "external".into(),
        database_host: "127.0.0.1".into(),
        database_port: 5432,
        nats_host: "127.0.0.1".into(),
        nats_port: 4222,
        rooms_root: rooms_root.clone(),
        house_id: "solarisael".into(),
        rooms: vec![
            HostRoomConfig {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                port: 8787,
            },
            HostRoomConfig {
                room: "kodo".into(),
                spirit: "Kodo".into(),
                port: 8788,
            },
        ],
    };
    let plan = runtime_plan(
        &PathBuf::from("release"),
        &PathBuf::from("data"),
        &config,
        "postgresql://external",
        "token",
    )?;

    assert_eq!(
        plan.iter()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>(),
        ["nats", "delivery", "host:kintsu", "host:kodo"]
    );
    for (room, spirit, port) in [("kintsu", "Kintsu", 8787), ("kodo", "Kodo", 8788)] {
        let host = plan
            .iter()
            .find(|spec| spec.name == format!("host:{room}"))
            .unwrap();
        assert_eq!(
            host.environment
                .get("ATHANOR_HOST_ROOM")
                .map(String::as_str),
            Some(room)
        );
        assert_eq!(
            host.environment
                .get("ATHANOR_HOST_SPIRIT")
                .map(String::as_str),
            Some(spirit)
        );
        assert_eq!(
            host.environment
                .get("ATHANOR_HOST_BIND")
                .map(String::as_str),
            Some(format!("127.0.0.1:{port}").as_str())
        );
        assert_eq!(
            host.environment.get("ATHANOR_HOST_ROOM_DIR"),
            Some(&rooms_root.join(room).display().to_string())
        );
        assert_eq!(
            host.environment.get("ATHANOR_HOST_STATE_DIR"),
            Some(
                &PathBuf::from("data")
                    .join("state/host")
                    .join(room)
                    .display()
                    .to_string()
            )
        );
        assert_eq!(
            host.readiness,
            Readiness::Tcp(format!("127.0.0.1:{port}").parse().unwrap())
        );
    }
    Ok(())
}
