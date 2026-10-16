use super::{database_error, invalid};
use crate::HostConfig;
use hearth::orihon::{Mode, RecordingState};
use protocol::ProtocolErrorBody;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use std::fs::{self, File, OpenOptions};

fn initial_state() -> RecordingState {
    RecordingState {
        mode: Mode::Log,
        generation: uuid::Uuid::new_v4().to_string(),
    }
}

fn validate(state: RecordingState) -> Result<RecordingState, ProtocolErrorBody> {
    if state.mode == Mode::Fold || uuid::Uuid::parse_str(&state.generation).is_err() {
        return Err(invalid("invalid persisted Orihon mode or generation"));
    }
    Ok(state)
}

pub(super) async fn lock_pg<'a>(
    pool: &'a PgPool,
    room: &str,
) -> Result<Transaction<'a, Postgres>, ProtocolErrorBody> {
    let mut transaction = pool.begin().await.map_err(database_error)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('orihon-mode:' || $1, 0))")
        .bind(room)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    Ok(transaction)
}

pub(super) async fn load_or_initialize_pg(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
) -> Result<RecordingState, ProtocolErrorBody> {
    let value: Option<Json<RecordingState>> =
        sqlx::query_scalar("SELECT value FROM room_settings WHERE room_key=$1 AND key='orihon'")
            .bind(room)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(database_error)?;
    match value {
        Some(Json(state)) => validate(state),
        None => {
            let state = initial_state();
            save_pg(transaction, room, &state).await?;
            Ok(state)
        }
    }
}

pub(super) async fn save_pg(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    state: &RecordingState,
) -> Result<(), ProtocolErrorBody> {
    sqlx::query(
        "INSERT INTO room_settings (room_key,key,value) VALUES ($1,'orihon',$2) \
         ON CONFLICT (room_key,key) DO UPDATE SET value=EXCLUDED.value",
    )
    .bind(room)
    .bind(Json(state))
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

pub(super) fn lock_file(host: &HostConfig) -> Result<File, ProtocolErrorBody> {
    let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
    fs::create_dir_all(&directory).map_err(file_error)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("mode.lock"))
        .map_err(file_error)?;
    file.lock().map_err(file_error)?;
    Ok(file)
}

pub(super) fn load_or_initialize_file(
    host: &HostConfig,
) -> Result<(Value, RecordingState), ProtocolErrorBody> {
    let path = host.room_dir.join(".athanor-room.json");
    let mut root: Value = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            invalid("Orihon refuses malformed .athanor-room.json; original bytes are unchanged")
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(file_error(error)),
    };
    let object = root
        .as_object()
        .ok_or_else(|| invalid("room configuration must be an object"))?;
    if let Some(value) = object.get("orihon") {
        let state = serde_json::from_value(value.clone())
            .map_err(|_| invalid("invalid Orihon room configuration"))?;
        return Ok((root, validate(state)?));
    }
    let state = initial_state();
    save_file(host, &mut root, &state)?;
    Ok((root, state))
}

pub(super) fn save_file(
    host: &HostConfig,
    root: &mut Value,
    state: &RecordingState,
) -> Result<(), ProtocolErrorBody> {
    root["orihon"] =
        serde_json::to_value(state).map_err(|_| invalid("Orihon mode serialization failed"))?;
    crate::store::atomic_json_write(&host.room_dir.join(".athanor-room.json"), root).map_err(
        |message| {
            ProtocolErrorBody::application("orihon_io", message)
                .retryable(true)
                .build()
        },
    )
}

fn file_error(error: std::io::Error) -> ProtocolErrorBody {
    ProtocolErrorBody::application(
        "orihon_io",
        format!("Orihon room configuration failed: {error}"),
    )
    .retryable(true)
    .build()
}
