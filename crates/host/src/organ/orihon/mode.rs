use super::{database_error, invalid};
use crate::HostConfig;
use hearth::orihon::{FolderBackend, FolderSettings, Mode, RecordingDefaults, RecordingState};
use protocol::ProtocolErrorBody;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};

pub(super) fn session_key(spirit: &str, session: &str) -> String {
    settings_key("orihon_session", spirit, session)
}

fn default_key(spirit: &str) -> String {
    settings_key("orihon_default", spirit, "")
}

fn settings_key(prefix: &str, spirit: &str, session: &str) -> String {
    let mut hash = Sha256::new();
    hash.update((spirit.len() as u64).to_be_bytes());
    hash.update(spirit.as_bytes());
    hash.update(session.as_bytes());
    format!("{prefix}_{:x}", hash.finalize())
}

fn initial_state(defaults: &RecordingDefaults) -> RecordingState {
    RecordingState {
        mode: defaults.mode,
        generation: uuid::Uuid::new_v4().to_string(),
        folder: defaults.folder.clone(),
    }
}

pub(super) fn defaults(state: &RecordingState) -> RecordingDefaults {
    RecordingDefaults {
        mode: state.mode,
        folder: state.folder.clone(),
    }
}

pub(super) fn validate(state: RecordingState) -> Result<RecordingState, ProtocolErrorBody> {
    if uuid::Uuid::parse_str(&state.generation).is_err() {
        return Err(invalid("invalid persisted Orihon generation"));
    }
    validate_policy(state.mode, state.folder.as_ref())?;
    Ok(state)
}

fn validate_policy(mode: Mode, folder: Option<&FolderSettings>) -> Result<(), ProtocolErrorBody> {
    if mode == Mode::Fold && folder.is_none() {
        return Err(invalid("fold mode requires a folder configuration"));
    }
    if let Some(folder) = folder {
        match folder.backend {
            FolderBackend::Local => {
                if folder.model.trim().is_empty()
                    || folder.model.len() > 256
                    || !folder.model.contains(':')
                    || folder.model.contains('\0')
                    || folder.runtime.is_some()
                {
                    return Err(invalid(
                        "Orihon local folding requires MODEL:TAG without a hosted runtime",
                    ));
                }
            }
            FolderBackend::Hosted => {
                akasha::orihon_folder::hosted_route(&folder.model)
                    .map_err(|_| invalid("Orihon hosted folding requires an exact anthropic/model or openai-codex/model selector"))?;
                let runtime = folder.runtime.as_ref().ok_or_else(|| {
                    invalid("Orihon hosted folding requires an explicit operator OMP binding")
                })?;
                akasha::orihon_folder::validate_runtime(runtime)
                    .map_err(|_| invalid("Orihon hosted OMP runtime binding is invalid"))?;
            }
        }
    }
    Ok(())
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
    spirit: &str,
    session: &str,
) -> Result<(RecordingDefaults, RecordingState), ProtocolErrorBody> {
    let key = default_key(spirit);
    let saved: Option<Json<RecordingDefaults>> =
        sqlx::query_scalar("SELECT value FROM room_settings WHERE room_key=$1 AND key=$2")
            .bind(room)
            .bind(&key)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(database_error)?;
    let defaults = match saved {
        Some(Json(defaults)) => defaults,
        None => {
            let legacy: Option<Json<RecordingState>> = sqlx::query_scalar(
                "SELECT value FROM room_settings WHERE room_key=$1 AND key='orihon'",
            )
            .bind(room)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(database_error)?;
            let defaults = match legacy {
                Some(Json(state)) => defaults(&validate(state)?),
                None => RecordingDefaults::default(),
            };
            save_value_pg(transaction, room, &key, json!(defaults)).await?;
            sqlx::query("DELETE FROM room_settings WHERE room_key=$1 AND key='orihon'")
                .bind(room)
                .execute(&mut **transaction)
                .await
                .map_err(database_error)?;
            defaults
        }
    };
    validate_policy(defaults.mode, defaults.folder.as_ref())?;
    let key = session_key(spirit, session);
    let saved: Option<Json<RecordingState>> =
        sqlx::query_scalar("SELECT value FROM room_settings WHERE room_key=$1 AND key=$2")
            .bind(room)
            .bind(&key)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(database_error)?;
    let state = match saved {
        Some(Json(state)) => validate(state)?,
        None => {
            let state = initial_state(&defaults);
            save_value_pg(transaction, room, &key, json!(state)).await?;
            state
        }
    };
    Ok((defaults, state))
}

pub(super) async fn save_pg(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    spirit: &str,
    session: &str,
    state: &RecordingState,
) -> Result<(), ProtocolErrorBody> {
    save_value_pg(
        transaction,
        room,
        &default_key(spirit),
        json!(defaults(state)),
    )
    .await?;
    save_value_pg(
        transaction,
        room,
        &session_key(spirit, session),
        json!(state),
    )
    .await
}

async fn save_value_pg(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    key: &str,
    value: Value,
) -> Result<(), ProtocolErrorBody> {
    sqlx::query(
        "INSERT INTO room_settings (room_key,key,value) VALUES ($1,$2,$3) \
         ON CONFLICT (room_key,key) DO UPDATE SET value=EXCLUDED.value",
    )
    .bind(room)
    .bind(key)
    .bind(Json(value))
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

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileSettings {
    spirits: BTreeMap<String, SpiritSettings>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SpiritSettings {
    defaults: RecordingDefaults,
    sessions: BTreeMap<String, RecordingState>,
}

fn settings_path(host: &HostConfig) -> std::path::PathBuf {
    host.room_dir
        .join(hearth::orihon::DIRECTORY)
        .join("settings.json")
}

fn read_settings(host: &HostConfig) -> Result<Option<FileSettings>, ProtocolErrorBody> {
    match fs::read(settings_path(host)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            invalid("Orihon refuses malformed settings.json; original bytes are unchanged")
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(file_error(error)),
    }
}

pub(super) fn load_or_initialize_file(
    host: &HostConfig,
    spirit: &str,
    session: &str,
) -> Result<(FileSettings, RecordingDefaults, RecordingState), ProtocolErrorBody> {
    let path = host.room_dir.join(".athanor-room.json");
    let mut root: Value = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            invalid("Orihon refuses malformed .athanor-room.json; original bytes are unchanged")
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(file_error(error)),
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| invalid("room configuration must be an object"))?;
    let saved = read_settings(host)?;
    let mut changed = saved.is_none();
    let mut settings = match saved {
        Some(settings) => settings,
        None => {
            let defaults = match object.get("orihon") {
                Some(value) => {
                    let state = serde_json::from_value(value.clone())
                        .map_err(|_| invalid("invalid Orihon room configuration"))?;
                    defaults(&validate(state)?)
                }
                None => RecordingDefaults::default(),
            };
            let mut settings = FileSettings::default();
            settings.spirits.insert(
                spirit.to_owned(),
                SpiritSettings {
                    defaults,
                    sessions: BTreeMap::new(),
                },
            );
            settings
        }
    };
    let entry = settings
        .spirits
        .entry(spirit.to_owned())
        .or_insert_with(|| {
            changed = true;
            SpiritSettings {
                defaults: RecordingDefaults::default(),
                sessions: BTreeMap::new(),
            }
        });
    validate_policy(entry.defaults.mode, entry.defaults.folder.as_ref())?;
    let defaults = entry.defaults.clone();
    let state = entry
        .sessions
        .entry(session_key(spirit, session))
        .or_insert_with(|| {
            changed = true;
            initial_state(&defaults)
        })
        .clone();
    let state = validate(state)?;

    if changed {
        save_file(host, &settings)?;
    }

    // The dedicated file is the migration fence, even if legacy cleanup fails.
    if object.remove("orihon").is_some() {
        write_json(&path, &root)?;
    }
    Ok((settings, defaults, state))
}

pub(super) fn update_file(
    settings: &mut FileSettings,
    spirit: &str,
    session: &str,
    state: &RecordingState,
) -> Result<(), ProtocolErrorBody> {
    let entry = settings
        .spirits
        .get_mut(spirit)
        .ok_or_else(|| invalid("Orihon spirit settings are not initialized"))?;
    entry.defaults = defaults(state);
    entry
        .sessions
        .insert(session_key(spirit, session), state.clone());
    Ok(())
}

pub(super) fn save_file(
    host: &HostConfig,
    settings: &FileSettings,
) -> Result<(), ProtocolErrorBody> {
    // enough: rewrite all room settings; upgrade when settings IO exceeds the 3s command budget.
    let value = serde_json::to_value(settings)
        .map_err(|_| invalid("Orihon settings serialization failed"))?;
    write_json(&settings_path(host), &value)
}

fn write_json(path: &std::path::Path, value: &Value) -> Result<(), ProtocolErrorBody> {
    crate::store::atomic_json_write(path, value).map_err(|message| {
        ProtocolErrorBody::application("orihon_io", message)
            .retryable(true)
            .build()
    })
}

/// One session's persisted recording state; `None` before the session first attached.
pub(super) async fn session_state(
    host: &HostConfig,
    pool: Option<&PgPool>,
    spirit: &str,
    session: &str,
) -> Result<Option<RecordingState>, ProtocolErrorBody> {
    let state = if host.akasha_enabled() {
        let pool = pool.ok_or_else(|| invalid("Orihon PostgreSQL storage is unavailable"))?;
        let value: Option<Json<RecordingState>> =
            sqlx::query_scalar("SELECT value FROM room_settings WHERE room_key=$1 AND key=$2")
                .bind(&host.room)
                .bind(session_key(spirit, session))
                .fetch_optional(pool)
                .await
                .map_err(database_error)?;
        value.map(|Json(state)| state)
    } else {
        let host = host.clone();
        let spirit = spirit.to_owned();
        let key = session_key(&spirit, session);
        tokio::task::spawn_blocking(move || {
            Ok::<_, ProtocolErrorBody>(read_settings(&host)?.and_then(|mut settings| {
                settings
                    .spirits
                    .remove(&spirit)
                    .and_then(|mut entry| entry.sessions.remove(&key))
            }))
        })
        .await
        .map_err(|_| invalid("Orihon session lookup did not complete"))??
    };
    state.map(validate).transpose()
}

fn file_error(error: std::io::Error) -> ProtocolErrorBody {
    ProtocolErrorBody::application(
        "orihon_io",
        format!("Orihon room configuration failed: {error}"),
    )
    .retryable(true)
    .build()
}
