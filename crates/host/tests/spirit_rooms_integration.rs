//! A real Host on a loopback port opens and closes rooms from the spirit
//! record while it runs.

use akasha::spirits::{SpiritRecord, begin_summon, retire_spirit};
use futures_util::{SinkExt, StreamExt};
use host::{HostConfig, HouseConfig, KnockAutonomy, RoomSource, fresh_room_state, start};
use sqlx::postgres::PgPoolOptions;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

const TOKEN: &str = "spirit-rooms-test";

fn room_config(house: &HouseConfig, root: &Path, record: &SpiritRecord) -> HostConfig {
    HostConfig {
        bind: house.bind,
        bearer_token: house.bearer_token.clone(),
        room_dir: root.join(&record.room),
        state_dir: root.join(&record.room).join("host"),
        house_dir: None,
        house_id: house.house_id.clone(),
        room: record.room.clone(),
        spirit: record.spirit.clone(),
        session: format!("app:{}", record.room),
        database_url: house.database_url.clone(),
        nats_url: None,
        nats_auth: None,
        knock_autonomy: KnockAutonomy::Off,
        operator: Some(record.operator.clone()),
    }
}

/// The installation folder of a room, as `athanor summon` writes it.
fn room_folder(root: &Path, record: &SpiritRecord) {
    let state = root.join(&record.room).join(".omp/runtime/athanor-house-state.json");
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::write(
        state,
        fresh_room_state(&record.room, &record.spirit, &record.operator).to_string(),
    )
    .unwrap();
}

async fn eventually(client: &reqwest::Client, url: &str, status: u16) -> serde_json::Value {
    for _ in 0..100 {
        let response = client.get(url).bearer_auth(TOKEN).send().await.unwrap();
        if response.status().as_u16() == status {
            return response.json().await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{url} never answered {status}");
}

// Kills: a Host that needs a restart for a new room, a forwarder that breaks
// the WebSocket upgrade, a retired room that still answers, and a roster
// endpoint that lists a retired room.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn the_host_opens_and_closes_rooms_from_the_spirit_record() {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("the Host proof requires a dedicated PostgreSQL URL");
    assert!(!url.contains("solarisael_memory") && !url.contains("solarisael-house"));
    let schema = format!("athanor_rooms_{}", uuid::Uuid::new_v4().simple());
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let separator = if url.contains('?') { '&' } else { '?' };
    let scoped = format!("{url}{separator}options[search_path]={schema},public");
    let pool = PgPoolOptions::new().max_connections(2).connect(&scoped).await.unwrap();
    akasha::migrations::run_migrations(&pool).await.unwrap();
    let root: PathBuf = std::env::temp_dir().join(format!("athanor-rooms-{}", uuid::Uuid::new_v4()));
    let kodo = SpiritRecord { room: "kodo".into(), spirit: "Kodo".into(), operator: "sol".into() };
    let skoll = SpiritRecord { room: "skoll".into(), spirit: "Sköll".into(), operator: "sol".into() };
    room_folder(&root, &kodo);
    let house = HouseConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        bearer_token: TOKEN.into(),
        house_id: "solarisael".into(),
        database_url: Some(scoped.clone()),
        nats_url: None,
        nats_auth: None,
    };
    let seed = vec![room_config(&house, &root, &kodo)];
    let (factory_house, factory_root) = (house.clone(), root.clone());
    let factory = Arc::new(move |record: &SpiritRecord| room_config(&factory_house, &factory_root, record));
    let runtime = start(house, RoomSource::Spirits { seed, factory }).unwrap();
    let base = format!("http://{}", runtime.address());
    let client = reqwest::Client::new();

    let listed = eventually(&client, &format!("{base}/room/kodo/spirits"), 200).await;
    assert_eq!(listed["spirits"], serde_json::json!([{"room": "kodo", "spirit": "Kodo", "operator": "sol"}]));
    eventually(&client, &format!("{base}/room/skoll/health"), 404).await;

    room_folder(&root, &skoll);
    begin_summon(&pool, &skoll, "test").await.unwrap().commit().await.unwrap();
    eventually(&client, &format!("{base}/room/skoll/health"), 200).await;
    let mut request = format!("ws://{}/room/skoll/athanor/v1/ws", runtime.address())
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    let (mut socket, upgraded) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert_eq!(upgraded.status().as_u16(), 101);
    socket.send(Message::text("not json")).await.unwrap();
    let answer = socket.next().await.unwrap().unwrap().into_text().unwrap();
    assert!(answer.contains("malformed JSON command"), "{answer}");
    let listed = eventually(&client, &format!("{base}/room/skoll/spirits"), 200).await;
    assert_eq!(listed["spirits"][1]["spirit"], "Sköll");

    retire_spirit(&pool, "skoll").await.unwrap();
    let refused = eventually(&client, &format!("{base}/room/skoll/health"), 404).await;
    assert_eq!(refused["room"], "skoll");
    let listed = eventually(&client, &format!("{base}/room/kodo/spirits"), 200).await;
    assert_eq!(listed["spirits"].as_array().unwrap().len(), 1);
    let unauthorized = client.get(format!("{base}/room/kodo/spirits")).send().await.unwrap();
    assert_eq!(unauthorized.status().as_u16(), 401);

    tokio::task::spawn_blocking(move || runtime.shutdown()).await.unwrap().unwrap();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(root);
}
