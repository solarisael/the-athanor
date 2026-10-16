mod mode;
pub(crate) mod transcripts;

use crate::HostConfig;
use hearth::orihon::{CaptureBatch, Kind, Mode, ReadRequest, RecordingState};
use protocol::organ::{OrganOperation, OrganRequest, OrganTargetScope};
use protocol::{CommandMeta, ProtocolErrorBody, ResponsePayload};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeRequest {
    mode: Option<Mode>,
}

pub(super) async fn call(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    request: OrganRequest,
) -> ResponsePayload<Value> {
    let result = execute(host, pool, meta, request).await;
    match result {
        Ok(result) => ResponsePayload::Result { result },
        Err(error) => ResponsePayload::Error { error },
    }
}

async fn execute(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    request: OrganRequest,
) -> Result<Value, ProtocolErrorBody> {
    if request.target_scope != OrganTargetScope::Room {
        return Err(invalid("Orihon is bound to one room"));
    }
    if !host.akasha_enabled() {
        let host = host.clone();
        let writer = meta.sender_session.clone();
        return tokio::task::spawn_blocking(move || file_call(&host, &writer, request))
            .await
            .map_err(|_| invalid("Orihon file operation did not complete"))?;
    }
    let pool = pool.ok_or_else(|| invalid("Orihon PostgreSQL storage is unavailable"))?;
    if request.operation == OrganOperation::Orihon {
        let read = decode_read(request)?;
        let receipt = origami::orihon::read_pg(pool, &host.room, read)
            .await
            .map_err(storage_error)?;
        return read_result(receipt);
    }

    // Mode changes cannot overtake an accepted capture in another Host process.
    let mut transaction = mode::lock_pg(pool, &host.room).await?;
    let mut state = mode::load_or_initialize_pg(&mut transaction, &host.room).await?;
    let result = match request.operation {
        OrganOperation::OrihonMode => {
            if change_mode(&mut state, request)? {
                mode::save_pg(&mut transaction, &host.room, &state).await?;
            }
            mode_result(&state)
        }
        OrganOperation::OrihonCapture => {
            let batch = capture_batch(host, request, &state)?;
            if state.mode == Mode::Off {
                return Ok(off_result(&state));
            }
            let receipt = origami::orihon::append_pg_in_transaction(
                &mut transaction,
                &host.room,
                &meta.sender_session,
                batch,
            )
            .await
            .map_err(storage_error)?;
            transaction.commit().await.map_err(database_error)?;
            let result = capture_result(&state, &receipt);
            let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
            let room = host.room.clone();
            tokio::task::spawn_blocking(move || {
                origami::orihon::export_file(&directory, &room, &receipt.accepted_records)
            })
            .await
            .map_err(|_| {
                invalid(
                    "Orihon accepted records; file export did not complete. Retry the same batch.",
                )
            })?
            .map_err(storage_error)?;
            return Ok(result);
        }
        _ => return Err(invalid("unsupported Orihon operation")),
    };
    transaction.commit().await.map_err(database_error)?;
    Ok(result)
}

fn file_call(
    host: &HostConfig,
    writer: &str,
    request: OrganRequest,
) -> Result<Value, ProtocolErrorBody> {
    let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
    if request.operation == OrganOperation::Orihon {
        let receipt = origami::orihon::read_file(&directory, &host.room, decode_read(request)?)
            .map_err(storage_error)?;
        return read_result(receipt);
    }
    let _lock = mode::lock_file(host)?;
    let (mut root, mut state) = mode::load_or_initialize_file(host)?;
    match request.operation {
        OrganOperation::OrihonMode => {
            if change_mode(&mut state, request)? {
                mode::save_file(host, &mut root, &state)?;
            }
            Ok(mode_result(&state))
        }
        OrganOperation::OrihonCapture => {
            let batch = capture_batch(host, request, &state)?;
            if state.mode == Mode::Off {
                return Ok(off_result(&state));
            }
            let receipt = origami::orihon::append_file(&directory, &host.room, writer, batch)
                .map_err(storage_error)?;
            Ok(capture_result(&state, &receipt))
        }
        _ => Err(invalid("unsupported Orihon operation")),
    }
}

fn decode_read(request: OrganRequest) -> Result<ReadRequest, ProtocolErrorBody> {
    serde_json::from_value(Value::Object(request.params))
        .map_err(|_| invalid("invalid Orihon read bounds"))
}

fn read_result(receipt: hearth::orihon::ReadReceipt) -> Result<Value, ProtocolErrorBody> {
    let mut value =
        serde_json::to_value(receipt).map_err(|_| invalid("Orihon read serialization failed"))?;
    value["ok"] = json!(true);
    value["authority"] = json!("provenance");
    Ok(value)
}

fn change_mode(
    state: &mut RecordingState,
    request: OrganRequest,
) -> Result<bool, ProtocolErrorBody> {
    let request: ModeRequest = serde_json::from_value(Value::Object(request.params))
        .map_err(|_| invalid("Orihon mode must be off or log"))?;
    if let Some(mode) = request.mode {
        if mode == Mode::Fold {
            return Err(invalid("Orihon fold mode is not implemented"));
        }
        if mode == state.mode {
            return Ok(false);
        }
        if state.mode == Mode::Off && mode == Mode::Log {
            state.generation = uuid::Uuid::new_v4().to_string();
        }
        state.mode = mode;
        return Ok(true);
    }
    Ok(false)
}

fn capture_batch(
    host: &HostConfig,
    mut request: OrganRequest,
    state: &RecordingState,
) -> Result<CaptureBatch, ProtocolErrorBody> {
    let generation = request.params.remove("generation");
    if generation.as_ref().and_then(Value::as_str) != Some(state.generation.as_str()) {
        return Err(ProtocolErrorBody::application(
            "orihon_generation_changed",
            "Orihon generation changed; establish a new session baseline before capture",
        )
        .build());
    }
    let mut batch: CaptureBatch = serde_json::from_value(Value::Object(request.params))
        .map_err(|_| invalid("invalid Orihon capture batch"))?;
    let operator = match &host.operator {
        Some(operator) => operator.clone(),
        None => {
            crate::store::RoomStateStore::new(host.room_state_path(), host.room.clone())
                .identity(&host.spirit)
                .map_err(|_| invalid("Orihon requires the bound operator"))?
                .operator
        }
    };
    for record in &mut batch.records {
        match record.kind {
            Kind::User | Kind::Note => {
                record.speaker.clone_from(&operator);
            }
            Kind::Talk => record.speaker.clone_from(&host.spirit),
            _ => {}
        }
        if !host.bearer_token.is_empty() && record.text.contains(&host.bearer_token) {
            record.text = record.text.replace(&host.bearer_token, "[REDACTED]");
            record.redacted = true;
        }
    }
    Ok(batch)
}

fn mode_result(state: &RecordingState) -> Value {
    json!({"ok":true,"mode":state.mode,"generation":state.generation,"supportedModes":["off","log"]})
}

fn off_result(state: &RecordingState) -> Value {
    json!({"ok":true,"mode":"off","generation":state.generation,"acceptedSourceIds":[],"appended":0,"replayed":0})
}

fn capture_result(state: &RecordingState, receipt: &hearth::orihon::CaptureReceipt) -> Value {
    json!({"ok":true,"mode":"log","generation":state.generation,"acceptedSourceIds":receipt.accepted_source_ids,"appended":receipt.appended,"replayed":receipt.replayed})
}

fn invalid(message: &str) -> ProtocolErrorBody {
    ProtocolErrorBody::application("invalid_params", message).build()
}

fn storage_error(error: origami::orihon::Error) -> ProtocolErrorBody {
    use origami::orihon::Error;
    match error {
        Error::Invalid(message) => invalid(&message),
        Error::Refusal { code, message } => ProtocolErrorBody::application(code, message).build(),
        Error::Database(error) => database_error(error),
        error @ Error::Io { .. } => ProtocolErrorBody::application("orihon_io", error.to_string())
            .retryable(true)
            .build(),
    }
}

fn database_error(_error: sqlx::Error) -> ProtocolErrorBody {
    ProtocolErrorBody::application("database", "Orihon database operation failed")
        .retryable(true)
        .build()
}
