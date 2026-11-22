use super::*;
use crate::cli::status::health_answer;
use crate::installer::RuntimeSecrets;
use crate::supervisor::RuntimeConfig;
use akasha::spirits::{SpiritRecord, begin_summon};
use std::time::{Duration, Instant};

const TOKEN: &str = "room-move-test";

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn vault_room(vault: &Path, room: &str) {
    let folder = vault.join(room);
    write(&folder.join(MARKER), &format!("{{\"room\":\"{room}\",\"localOnly\":true}}\n"));
    write(
        &folder.join(".omp/runtime/athanor-house-state.json"),
        &host::fresh_room_state(room, "Kintsu", "sol").to_string(),
    );
    write(&folder.join(".omp/runtime/restart-capability"), "secret\n");
    write(
        &folder.join("AGENTS.md"),
        "# Room\n- @active_spirit.md — who\n- @../house/ORGANS.md — organs\n- @notes.md — library\n",
    );
    write(&folder.join("active_spirit.md"), "# Active Spirit: Kintsu\n");
    write(&folder.join("presence-pulse.md"), "pulse\n");
    write(&folder.join("notes.md"), "LIBRARY\n");
    write(&folder.join("orihon/2026-11-22.md"), "transcript\n");
    write(&folder.join("memory/a.md"), "memory\n");
}

fn config(vault: &Path, operator: &Path) -> RuntimeConfig {
    RuntimeConfig {
        database_mode: "external".into(),
        database_host: "127.0.0.1".into(),
        database_port: 5432,
        nats_host: "127.0.0.1".into(),
        // Nothing listens here: the Host serves rooms without a broker.
        nats_port: 9,
        host_port: 0,
        schema_version: 2,
        house_id: "solarisael".into(),
        rooms_root: vault.to_owned(),
        operator_state_root: operator.to_owned(),
        default_room: "kintsu".into(),
        rooms: Vec::new(),
        omp_config_path: None,
        client_config_path: None,
    }
}

async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check() {
        assert!(Instant::now() < deadline, "{what} never happened");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn files(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_owned()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push((path.strip_prefix(folder).unwrap().to_owned(), fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

// Kills: a summoned room that looks for the House page beside its own folder
// under <dataRoot>/rooms and so loses every House grant.
#[test]
fn a_summoned_room_reads_the_house_page_of_the_rooms_root() {
    let root = tempfile::tempdir().unwrap();
    let (vault, layout) = (
        root.path().join("vault"),
        InstallLayout { program: root.path().join("program"), data: root.path().join("data") },
    );
    write(&vault.join("house/.athanor-house.json"), r#"{"jevGiga":{"mode":"active"}}"#);
    write(&layout.rooms().join("hati").join(MARKER), r#"{"room":"hati"}"#);
    let secrets = RuntimeSecrets {
        host_token: TOKEN.into(),
        postgres_password: String::new(),
        external_database_url: Some("postgres://unused@127.0.0.1:9/none".into()),
        host_nats_auth: None,
        akasha_nats_auth: None,
    };
    let (_, source) =
        crate::app::host_rooms(&layout, &config(&vault, root.path()), &secrets).unwrap();
    let host::RoomSource::Spirits { factory, .. } = source else {
        panic!("the install reads its rooms from the spirit record");
    };

    let hati = factory(&SpiritRecord { room: "hati".into(), spirit: "Hati".into(), operator: "sol".into() });
    assert_eq!(hati.room_dir, layout.rooms().join("hati"));
    let marker = akasha::room_marker::read_room_marker(&hati.room_dir, hati.house_dir.as_deref());
    assert_eq!(marker["jevGiga"]["mode"], "active");
    assert_eq!(marker["room"], "hati");
}

// Kills: a move that the Host serves only after a restart, a move that leaves
// the runtime or the Host state behind or touches the library, a second run
// that moves again, and a failed move that leaves any file out of place.
// ATHANOR_MOVE_TEST_VAULT_PARENT on another filesystem (for example /dev/shm)
// proves the copy path of a move across filesystems.
// The forced failure locks a folder with Unix permission bits.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn a_served_vault_room_moves_into_the_installation_folder() {
    let database = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("the move proof requires a dedicated PostgreSQL URL");
    assert!(!database.contains("solarisael_memory") && !database.contains("solarisael-house"));
    let schema = format!("athanor_move_{}", uuid::Uuid::new_v4().simple());
    let admin = PgPoolOptions::new().max_connections(1).connect(&database).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
    let separator = if database.contains('?') { '&' } else { '?' };
    let scoped = format!("{database}{separator}options[search_path]={schema},public");
    let pool = PgPoolOptions::new().max_connections(2).connect(&scoped).await.unwrap();
    akasha::migrations::run_migrations(&pool).await.unwrap();

    let directory = tempfile::tempdir().unwrap();
    let vault_parent = std::env::var_os("ATHANOR_MOVE_TEST_VAULT_PARENT")
        .map_or_else(|| directory.path().to_owned(), PathBuf::from);
    let vault_root = tempfile::tempdir_in(&vault_parent).unwrap();
    let vault = vault_root.path().join("vault");
    let layout = InstallLayout { program: directory.path().join("program"), data: directory.path().join("data") };
    write(&vault.join("house/ORGANS.md"), "ORGANS\n");
    for room in ["kintsu", "hati"] {
        vault_room(&vault, room);
        let record = SpiritRecord { room: room.into(), spirit: "Kintsu".into(), operator: "sol".into() };
        begin_summon(&pool, &record, "test").await.unwrap().commit().await.unwrap();
    }
    let secrets = RuntimeSecrets {
        host_token: TOKEN.into(),
        postgres_password: String::new(),
        external_database_url: Some(scoped),
        host_nats_auth: None,
        akasha_nats_auth: None,
    };
    let (house, source) =
        crate::app::host_rooms(&layout, &config(&vault, directory.path()), &secrets).unwrap();
    let runtime = host::start(house, source).unwrap();
    let port = runtime.address().port();
    let places = Places {
        rooms: layout.rooms(),
        vault: vault.clone(),
        host_state: layout.host_state(),
        host_port: port,
        host_token: TOKEN.into(),
    };
    for room in ["kintsu", "hati"] {
        eventually("the vault room opens", || {
            health_answer(port, room, TOKEN).is_ok_and(|(status, _)| status == 200)
                && layout.host_state().join(room).is_dir()
        })
        .await;
    }
    let library_before: Vec<_> = files(&vault.join("kintsu"))
        .into_iter()
        .filter(|(path, _)| ["notes.md", "orihon", "memory"].iter().any(|kept| path.starts_with(kept)))
        .collect();
    let request = |room: &str| Request {
        room: room.into(),
        files: ["AGENTS.md", "active_spirit.md", "presence-pulse.md"].map(PathBuf::from).to_vec(),
        offline: false,
    };

    let answer = move_room(&pool, &places, &request("kintsu")).await.unwrap();

    assert_eq!(answer["change"], "moved", "{answer}");
    let folder = layout.rooms().join("kintsu");
    let marker: Value = serde_json::from_slice(&fs::read(folder.join(MARKER)).unwrap()).unwrap();
    assert_eq!(marker["library"], json!(vault.join("kintsu")));
    assert!(folder.join(".omp/runtime/restart-capability").is_file());
    assert!(folder.join(HOST_STATE).is_dir());
    assert!(!layout.host_state().join("kintsu").exists());
    let agents = fs::read_to_string(folder.join(INSTRUCTIONS)).unwrap();
    assert!(agents.contains("- @active_spirit.md — who"), "{agents}");
    assert!(agents.contains(&format!("- @{} — organs", vault.join("house/ORGANS.md").display())), "{agents}");
    assert!(agents.contains(&format!("- @{} — library", vault.join("kintsu/notes.md").display())), "{agents}");
    assert!(agents.ends_with(&format!("The library of this room stays in {}/.\n", vault.join("kintsu").display())));
    assert_eq!(answer["rewrites"].as_array().unwrap().len(), 2, "{answer}");
    for gone in [MARKER, RUNTIME, "AGENTS.md", "active_spirit.md", "presence-pulse.md"] {
        assert!(!vault.join("kintsu").join(gone).exists(), "{gone} stayed in the vault");
    }
    let library_after: Vec<_> = files(&vault.join("kintsu"));
    assert_eq!(library_after, library_before, "the library changed");
    // The rebuilt room writes its session ledger into the new state folder.
    let poll = format!(r#"{{"action":"poll","instanceId":"{}","conversationId":"x"}}"#, uuid::Uuid::new_v4());
    eventually("the Host serves the moved room from its new folders", || {
        let _ = host_answer(port, "POST", "/room/kintsu/session", TOKEN, &poll);
        folder.join(HOST_STATE).join("portable-sessions.json").is_file()
    })
    .await;
    assert_eq!(health_answer(port, "kintsu", TOKEN).unwrap().0, 200);
    assert!(!layout.host_state().join("kintsu").exists(), "the old state folder came back");
    let again = move_room(&pool, &places, &request("kintsu")).await.unwrap();
    assert_eq!(again["change"], "unchanged");

    // A Host state folder that cannot move fails the move midway.
    let before = files(&vault.join("hati"));
    let state_before = files(&layout.host_state().join("hati"));
    let mut locked = fs::metadata(layout.host_state()).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut locked, 0o555);
    fs::set_permissions(layout.host_state(), locked).unwrap();
    let failed = move_room(&pool, &places, &request("hati")).await;
    let mut open = fs::metadata(layout.host_state()).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut open, 0o755);
    fs::set_permissions(layout.host_state(), open).unwrap();

    let error = format!("{:#}", failed.unwrap_err());
    assert!(error.contains("were undone"), "{error}");
    assert_eq!(files(&vault.join("hati")), before);
    assert_eq!(files(&layout.host_state().join("hati")), state_before);
    assert!(!layout.rooms().join("hati").exists());
    assert!(!layout.rooms().join(".hati.moving").exists());
    assert_eq!(health_answer(port, "hati", TOKEN).unwrap().0, 200);

    tokio::task::spawn_blocking(move || runtime.shutdown()).await.unwrap().unwrap();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();
}
