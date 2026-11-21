mod change;
pub(crate) mod live;
mod mode;
mod navigator;
pub(crate) mod transcripts;
mod tree;
mod zoom;

use super::worker_lifecycle::{self, BoundOwner};
use crate::HostConfig;
use change::{ModeWrite, change_mode};
use hearth::orihon::live::{LIVE_INSTRUCTIONS, LIVE_VERSION};
use hearth::orihon::{
    CaptureBatch, FolderSettings, Kind, Mode, ReadRequest, Record, RecordingDefaults,
    RecordingState,
};
use origami::orihon::live::store::Prefix;
use protocol::organ::{OrganOperation, OrganRequest, OrganTargetScope};
use protocol::{CommandMeta, ProtocolErrorBody, ResponsePayload};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashSet;
use std::io::Write;
use tokio::time::{Duration, Instant};

/// How long a live turn waits for its view unless it names its own wait.
const TURN_WAIT_MS: u64 = 120_000;
const MAX_TURN_WAIT_MS: u64 = 600_000;
/// The serialized stable prefix a room stores and its compactor resends.
const MAX_PREFIX_BYTES: usize = 4 * 1024 * 1024;
/// Worker bindings a task child's view follows up to the session it works for.
const LINEAGE_HOPS: usize = 16;

pub(crate) async fn initialize_session(
    host: &HostConfig,
    pool: Option<&PgPool>,
    spirit: &str,
    session: &str,
) -> Result<(), ProtocolErrorBody> {
    if host.akasha_enabled() {
        let pool = pool.ok_or_else(|| invalid("Orihon PostgreSQL storage is unavailable"))?;
        let mut transaction = mode::lock_pg(pool, &host.room).await?;
        mode::load_or_initialize_pg(&mut transaction, &host.room, spirit, session).await?;
        transaction.commit().await.map_err(database_error)?;

        return Ok(());
    }

    let host = host.clone();
    let spirit = spirit.to_owned();
    let session = session.to_owned();

    tokio::task::spawn_blocking(move || {
        let _lock = mode::lock_file(&host)?;
        mode::load_or_initialize_file(&host, &spirit, &session)?;
        Ok(())
    })
    .await
    .map_err(|_| invalid("Orihon session initialization did not complete"))?
}

/// `orihon_turn`. A live turn names its generation, frontier, stable prefix and model; a task
/// child names nothing, because its worker lineage is the authority.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TurnParams {
    generation: Option<String>,
    purpose: Purpose,
    through_source_id: Option<String>,
    prefix: Option<Prefix>,
    wait_ms: Option<u64>,
    model: Option<live::TurnModel>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Purpose {
    Turn,
    Subagent,
}

/// What a durable mode change or capture asks of the room's live owner.
enum Effect {
    None,
    /// A capture appended records: a message boundary. File rooms pass the records on.
    Boundary(Vec<Record>),
    /// An explicit change into fold mode: the session takes part.
    Join {
        folder: FolderSettings,
        explicit: bool,
    },
    /// An explicit change into log or off: the session stops taking part.
    Leave,
}

impl Effect {
    async fn apply(self, live: &live::Room, session: &str) {
        match self {
            Self::None => {}
            Self::Boundary(records) => live.boundary(records),
            Self::Join { folder, explicit } => {
                live.join(session.to_owned(), folder, explicit).await;
            }
            Self::Leave => live.leave(session.to_owned()).await,
        }
    }
}

pub(super) async fn call(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    request: OrganRequest,
    live: &live::Room,
) -> ResponsePayload<Value> {
    match execute(host, pool, meta, request, live).await {
        Ok(result) => ResponsePayload::Result { result },
        Err(error) => ResponsePayload::Error { error },
    }
}

async fn execute(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    request: OrganRequest,
    live: &live::Room,
) -> Result<Value, ProtocolErrorBody> {
    if request.target_scope != OrganTargetScope::Room {
        return Err(invalid("Orihon is bound to one room"));
    }
    match request.operation {
        OrganOperation::Unfold => {
            let unfold = serde_json::from_value(Value::Object(request.params))
                .map_err(|_| invalid("invalid Orihon unfold request"))?;
            return tree::unfold(host, pool, unfold).await;
        }
        OrganOperation::OrihonZoom => {
            let zoom = serde_json::from_value(Value::Object(request.params))
                .map_err(|_| invalid("invalid Orihon zoom request"))?;
            return zoom::zoom(host, pool, zoom).await;
        }
        OrganOperation::OrihonDate => {
            let date = serde_json::from_value(Value::Object(request.params))
                .map_err(|_| invalid("invalid Orihon date request"))?;
            return zoom::date(host, pool, date).await;
        }
        OrganOperation::OrihonTurn => {
            let params = serde_json::from_value(Value::Object(request.params))
                .map_err(|_| invalid("invalid Orihon turn request"))?;
            return turn(host, pool, meta, params, live).await;
        }
        OrganOperation::Orihon if request.params.contains_key("query") => {
            let navigation = serde_json::from_value(Value::Object(request.params))
                .map_err(|_| invalid("invalid Orihon navigation request"))?;
            return navigator::navigate(host, pool, navigation).await;
        }
        _ => {}
    }
    if !host.akasha_enabled() {
        let host = host.clone();
        let spirit = meta.sender_spirit.clone();
        let writer = meta.sender_session.clone();
        let owner = live.clone();
        let (result, effect) = tokio::task::spawn_blocking(move || {
            file_call(&host, &spirit, &writer, request, &owner)
        })
        .await
        .map_err(|_| invalid("Orihon file operation did not complete"))??;
        effect.apply(live, &meta.sender_session).await;
        return Ok(result);
    }
    let pool = pool.ok_or_else(|| invalid("Orihon PostgreSQL storage is unavailable"))?;

    pg_call(host, pool, meta, request, live).await
}

async fn pg_call(
    host: &HostConfig,
    pool: &PgPool,
    meta: &CommandMeta,
    request: OrganRequest,
    live: &live::Room,
) -> Result<Value, ProtocolErrorBody> {
    if request.operation == OrganOperation::Orihon {
        let read = decode_read(request)?;
        let receipt = origami::orihon::read_pg(pool, &host.room, read)
            .await
            .map_err(storage_error)?;
        return read_result(receipt);
    }

    // Mode changes cannot overtake an accepted capture in another Host process.
    let mut transaction = mode::lock_pg(pool, &host.room).await?;
    let (mut defaults, mut state) = mode::load_or_initialize_pg(
        &mut transaction,
        &host.room,
        &meta.sender_spirit,
        &meta.sender_session,
    )
    .await?;
    match request.operation {
        OrganOperation::OrihonMode => {
            let (write, effect) = change_mode(&mut state, request)?;

            if write == ModeWrite::NewGeneration {
                state.generation_start_seq =
                    mode::generation_start_pg(&mut transaction, &host.room).await?;
            }
            if write != ModeWrite::Status {
                mode::save_pg(
                    &mut transaction,
                    &host.room,
                    &meta.sender_spirit,
                    &meta.sender_session,
                    &state,
                )
                .await?;
                defaults = mode::defaults(&state);
            }
            transaction.commit().await.map_err(database_error)?;
            effect.apply(live, &meta.sender_session).await;
            Ok(mode_result(
                &state,
                &defaults,
                &meta.sender_spirit,
                &meta.sender_session,
                live,
            ))
        }
        OrganOperation::OrihonCapture => {
            let batch = capture_batch(host, request, &state)?;
            if state.mode == Mode::Off {
                let result = off_result(&state);
                transaction.commit().await.map_err(database_error)?;
                return Ok(result);
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
            // The owner reads accepted records from PostgreSQL; the file export only mirrors.
            if receipt.appended > 0 {
                live.boundary(Vec::new());
            }
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
            Ok(result)
        }
        _ => Err(invalid("unsupported Orihon operation")),
    }
}

fn file_call(
    host: &HostConfig,
    spirit: &str,
    writer: &str,
    request: OrganRequest,
    live: &live::Room,
) -> Result<(Value, Effect), ProtocolErrorBody> {
    let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
    if request.operation == OrganOperation::Orihon {
        let receipt = origami::orihon::read_file(&directory, &host.room, decode_read(request)?)
            .map_err(storage_error)?;
        return Ok((read_result(receipt)?, Effect::None));
    }
    let _lock = mode::lock_file(host)?;
    let (mut settings, mut defaults, mut state) =
        mode::load_or_initialize_file(host, spirit, writer)?;
    match request.operation {
        OrganOperation::OrihonMode => {
            let (write, effect) = change_mode(&mut state, request)?;

            if write == ModeWrite::NewGeneration {
                state.generation_start_seq = mode::generation_start_file(host)?;
            }
            if write != ModeWrite::Status {
                mode::update_file(&mut settings, spirit, writer, &state)?;
                mode::save_file(host, &settings)?;
                defaults = mode::defaults(&state);
            }
            Ok((mode_result(&state, &defaults, spirit, writer, live), effect))
        }
        OrganOperation::OrihonCapture => {
            let batch = capture_batch(host, request, &state)?;
            if state.mode == Mode::Off {
                return Ok((off_result(&state), Effect::None));
            }
            let receipt = origami::orihon::append_file(&directory, &host.room, writer, batch)
                .map_err(storage_error)?;
            let result = capture_result(&state, &receipt);
            let effect = if receipt.appended > 0 {
                Effect::Boundary(receipt.accepted_records)
            } else {
                Effect::None
            };
            Ok((result, effect))
        }
        _ => Err(invalid("unsupported Orihon operation")),
    }
}

/// `orihon_turn`. A live turn waits for the persisted chat view through its frontier; a task
/// child gets the view as built, cut at its first unbuilt line.
async fn turn(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    params: TurnParams,
    live: &live::Room,
) -> Result<Value, ProtocolErrorBody> {
    let started = Instant::now();
    let TurnParams {
        generation,
        purpose,
        through_source_id,
        prefix,
        wait_ms,
        model,
    } = params;
    if let Purpose::Subagent = purpose {
        if generation.is_some()
            || through_source_id.is_some()
            || prefix.is_some()
            || wait_ms.is_some()
            || model.is_some()
        {
            return Err(invalid(
                "a task child's view takes no generation, throughSourceId, prefix, waitMs or model",
            ));
        }
        let (spirit, session) = lineage_root(host, pool, &meta.sender_session).await?;
        let root = mode::session_state(host, pool, &spirit, &session).await?;
        if !matches!(root, Some(state) if state.mode == Mode::Fold) {
            return Err(live::not_live(
                "the session this task child works for is not in fold mode",
            ));
        }
        return live.subagent().await;
    }
    let (Some(generation), Some(prefix), Some(model)) = (generation, prefix, model) else {
        return Err(invalid("a live turn requires generation, prefix and model"));
    };
    let state = mode::session_state(host, pool, &meta.sender_spirit, &meta.sender_session)
        .await?
        .ok_or_else(|| invalid("Orihon session settings are not initialized"))?;
    if state.mode != Mode::Fold {
        return Err(live::not_live("this session is not in fold mode"));
    }
    if state.generation != generation {
        return Err(ProtocolErrorBody::application(
            "orihon_generation_changed",
            "Orihon generation changed; establish a new session baseline before this turn",
        )
        .build());
    }
    let wait = wait_ms.unwrap_or(TURN_WAIT_MS);
    if !(1..=MAX_TURN_WAIT_MS).contains(&wait) {
        return Err(invalid("waitMs must be 1 through 600000"));
    }
    validate_prefix(&prefix)?;
    let folder = state
        .folder
        .ok_or_else(|| invalid("fold mode requires a folder configuration"))?;
    live.turn(live::TurnRequest {
        session: meta.sender_session.clone(),
        generation_start_seq: state.generation_start_seq,
        through_source_id,
        prefix,
        model,
        folder,
        started,
        deadline: started + Duration::from_millis(wait),
    })
    .await
}

/// The stable prompt the compactor resends: the Orihon instructions as one exact system part,
/// tools with unique names and object schemas, and at most 4 MiB serialized.
fn validate_prefix(prefix: &Prefix) -> Result<(), ProtocolErrorBody> {
    let refuse =
        |message: &str| ProtocolErrorBody::application("orihon_prefix_invalid", message).build();
    if !prefix.system.iter().any(|part| part == LIVE_INSTRUCTIONS) {
        return Err(refuse(
            "one system part must be the Orihon instructions exactly",
        ));
    }
    let mut names = HashSet::with_capacity(prefix.tools.len());
    for tool in &prefix.tools {
        if tool.name.trim().is_empty() || !names.insert(tool.name.as_str()) {
            return Err(refuse("tool names must be unique and not blank"));
        }
        if !tool.input_schema.is_object() {
            return Err(refuse("every tool inputSchema must be a JSON object"));
        }
    }
    let mut size = ByteCount(0);
    if serde_json::to_writer(&mut size, prefix).is_err() || size.0 > MAX_PREFIX_BYTES {
        return Err(refuse("the prefix must serialize to at most 4 MiB"));
    }
    Ok(())
}

/// Counts serialized bytes without keeping them.
struct ByteCount(usize);

impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The session a task child works for: from each bound session to its swarm's owner, until a
/// session no lifecycle instance is bound to. Every link must still be live work.
async fn lineage_root(
    host: &HostConfig,
    pool: Option<&PgPool>,
    child: &str,
) -> Result<(String, String), ProtocolErrorBody> {
    let mut link = bound_owner(host, pool, child).await?.ok_or_else(|| {
        ProtocolErrorBody::application(
            "orihon_lineage_pending",
            "this session is not bound to a task yet",
        )
        .retryable(true)
        .build()
    })?;
    for _ in 0..LINEAGE_HOPS {
        if !link.live {
            return Err(live::not_live("a task this child works for has ended"));
        }
        match bound_owner(host, pool, &link.session).await? {
            Some(parent) => link = parent,
            None => return Ok((link.spirit, link.session)),
        }
    }
    Err(invalid(&format!(
        "this task child's worker lineage is deeper than {LINEAGE_HOPS} bindings"
    )))
}

/// The owner of the swarm holding the newest lifecycle instance bound to `session` in this
/// Host's house and room, read from the store the Host's worker lifecycle writes.
async fn bound_owner(
    host: &HostConfig,
    pool: Option<&PgPool>,
    session: &str,
) -> Result<Option<BoundOwner>, ProtocolErrorBody> {
    if !host.akasha_enabled() {
        let host = host.clone();
        let session = session.to_owned();
        return tokio::task::spawn_blocking(move || worker_lifecycle::bound_owner(&host, &session))
            .await
            .map_err(|_| invalid("Orihon worker lineage lookup did not complete"))?;
    }
    let pool = pool.ok_or_else(|| invalid("Orihon PostgreSQL storage is unavailable"))?;
    let owner: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT s.owner_spirit, s.owner_session,
                i.state <> 'retired' AND i.settled_outcome IS NULL AND s.disposed_at IS NULL
         FROM worker_lifecycle.instances i
         JOIN worker_lifecycle.books b ON b.book_id = i.book_id
         JOIN worker_lifecycle.swarms s ON s.swarm_id = b.swarm_id
         WHERE i.bound_session = $1 AND s.house_id = $2 AND s.room = $3
         ORDER BY i.updated_at DESC, i.instance_id DESC LIMIT 1",
    )
    .bind(session)
    .bind(&host.house_id)
    .bind(&host.room)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?;
    Ok(owner.map(|(spirit, session, live)| BoundOwner {
        spirit,
        session,
        live,
    }))
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
    let operator = crate::store::RoomStateStore::new(host.room_state_path(), host.room.clone())
        .identity(&host.spirit)
        .map_err(|_| invalid("Orihon requires the bound operator display name"))?
        .operator;
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
        if let Some(fold_text) = record.fold_text.as_mut()
            && !host.bearer_token.is_empty()
            && fold_text.contains(&host.bearer_token)
        {
            *fold_text = fold_text.replace(&host.bearer_token, "[REDACTED]");
            record.redacted = true;
        }
    }
    Ok(batch)
}

fn mode_result(
    state: &RecordingState,
    defaults: &RecordingDefaults,
    spirit: &str,
    session: &str,
    live: &live::Room,
) -> Value {
    json!({"ok":true,"mode":state.mode,"generation":state.generation,"generationStartSeq":state.generation_start_seq,"folder":state.folder,
        "defaults": defaults, "spirit": spirit, "sourceSession": session,
        "supportedModes":["off","log","fold"],
        "instructions":LIVE_INSTRUCTIONS,"instructionsVersion":LIVE_VERSION,
        "liveStatus":live.status()})
}

fn off_result(state: &RecordingState) -> Value {
    json!({"ok":true,"mode":"off","generation":state.generation,"acceptedSourceIds":[],"appended":0,"replayed":0})
}

fn capture_result(state: &RecordingState, receipt: &hearth::orihon::CaptureReceipt) -> Value {
    json!({"ok":true,"mode":state.mode,"generation":state.generation,"acceptedSourceIds":receipt.accepted_source_ids,"appended":receipt.appended,"replayed":receipt.replayed})
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_mode_without_a_model_does_not_set_a_default() {
        let mut state = RecordingState {
            mode: Mode::Log,
            generation: uuid::Uuid::new_v4().to_string(),
            generation_start_seq: 0,
            folder: None,
        };
        let request = serde_json::from_value(json!({
            "operation": "orihon_mode",
            "params": {"mode": "fold", "backend": "hosted"}
        }))
        .unwrap();

        assert!(change_mode(&mut state, request).is_err());
        assert_eq!(state.mode, Mode::Log);
        assert_eq!(state.folder, None);
    }
}
