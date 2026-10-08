mod giga;
pub(crate) use giga::{GigaRoomWorker, serve as serve_giga};

use crate::HostConfig;
use akasha::insula_writer::{end_span, start_span};
use akasha::native::{self, ProtocolRequest};
use akasha::{AppError, Config, TrustedBinding};
use protocol::organ::{OrganOperation, OrganRequest};
use protocol::{
    CommandMeta, ProtocolError, ProtocolErrorBody, RequestEnvelope, ResponseEnvelope,
    ResponsePayload,
};
use serde_json::{Map, Value};
use sqlx::PgPool;
use tokio::sync::Mutex;

pub(crate) fn substrate_config(host: &HostConfig) -> Result<Config, AppError> {
    let database_url = host
        .database_url
        .as_deref()
        .ok_or_else(|| AppError::Config("this Host room does not enable AKASHA".into()))?;
    let mut config = Config::for_database(database_url)?;
    config.nats_url = host.nats_url.clone();
    config.giga_source_room = Some(host.room.clone());
    config.giga_source_ledger_dir = Some(hearth::conversation::source_ledger_directory_path(
        &host.room_dir,
    ));
    Ok(config)
}

pub(crate) async fn call(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    mut request: OrganRequest,
    giga_worker: &Mutex<GigaRoomWorker>,
) -> ResponsePayload<Value> {
    let method = request.operation.method();
    let bound = match bound_host_config(host, meta) {
        Ok(bound) => bound,
        Err(error) => {
            return ResponsePayload::Error {
                error: error.protocol_error_body(&method),
            };
        }
    };
    let host = &bound;
    if request.operation == OrganOperation::GigaSetEnablement {
        if request.target_scope != protocol::organ::OrganTargetScope::Room {
            return ResponsePayload::Error {
                error: ProtocolError::InvalidParams("GIGA enablement is bound to one room".into())
                    .into(),
            };
        }
        let enablement =
            match serde_json::from_value::<akasha::GigaEnablement>(Value::Object(request.params)) {
                Ok(enablement) => enablement,
                Err(error) => {
                    return ResponsePayload::Error {
                        error: ProtocolError::InvalidParams(error.to_string()).into(),
                    };
                }
            };
        return match giga_worker
            .lock()
            .await
            .set_enablement(host, pool, enablement)
            .await
        {
            Ok(result) => ResponsePayload::Result { result },
            Err(error) => ResponsePayload::Error {
                error: error.protocol_error_body(&method),
            },
        };
    }
    let enablement = if request.operation == OrganOperation::GigaConversationIngest {
        match request
            .params
            .remove("enablement")
            .map(serde_json::from_value::<akasha::GigaEnablement>)
            .transpose()
        {
            Ok(enablement) => enablement,
            Err(error) => {
                return ResponsePayload::Error {
                    error: ProtocolError::InvalidParams(error.to_string()).into(),
                };
            }
        }
    } else {
        None
    };
    let request = match bound_request(host, meta, request) {
        Ok(request) => request,
        Err(error) => {
            return ResponsePayload::Error {
                error: error.into(),
            };
        }
    };
    execute_native(host, pool, meta, request, enablement, giga_worker).await
}

async fn execute_native(
    host: &HostConfig,
    pool: Option<&PgPool>,
    meta: &CommandMeta,
    request: ProtocolRequest,
    enablement: Option<akasha::GigaEnablement>,
    giga_worker: &Mutex<GigaRoomWorker>,
) -> ResponsePayload<Value> {
    let method = native::operation_name(&request);
    let health = matches!(&request, ProtocolRequest::SubstrateHealth(_));
    let binding = TrustedBinding {
        house_id: host.house_id.clone(),
        room: host.room.clone(),
        spirit: host.spirit.clone(),
        session_id: meta.sender_session.clone(),
    };
    let config = native::needs_runtime(&request).then(|| substrate_config(host));
    let (runtime, mut failure) = match (config.as_ref(), pool) {
        (Some(Ok(config)), Some(pool)) => (Some((config, pool)), None),
        (None, _) => (None, None),
        _ => (
            None,
            Some(AppError::Config(
                "the Host native runtime is unavailable".into(),
            )),
        ),
    };
    if let Some(Err(error)) = config.as_ref() {
        return ResponsePayload::Error {
            error: error.protocol_error_body(&method),
        };
    }
    if let Some((_, pool)) = runtime {
        failure = Config::validate_pool(pool).await.err();
    }
    if failure.is_none() && matches!(&request, ProtocolRequest::GigaConversationIngest(_)) {
        failure = giga::admit_ingest(host, pool, giga_worker, enablement)
            .await
            .err();
    }
    let span = start_span(
        &binding,
        native::INSULA_COMPONENT,
        native::INSULA_LAYER,
        native::operation_name(&request),
    );
    let services = native::NativeServices {
        health_config: health.then(|| substrate_config(host)),
        nats_auth: Some(host.nats_auth.as_ref()),
        giga_enablement: if matches!(&request, ProtocolRequest::GigaHealth(_)) {
            Some(giga_worker.lock().await.enablement())
        } else {
            None
        },
    };
    match native::execute_service(
        meta.message_id.clone(),
        request,
        runtime,
        failure,
        span.as_ref(),
        services,
    )
    .await
    {
        Ok(dispatched) => {
            end_span(span, dispatched.outcome, dispatched.error_class);
            match serde_json::from_str::<ResponseEnvelope<Value>>(&dispatched.json) {
                Ok(mut response) => {
                    if health {
                        if let ResponsePayload::Result { result } = &mut response.payload {
                            result
                                .as_object_mut()
                                .expect("native health is an object")
                                .insert("configured".into(), Value::Bool(host.akasha_enabled()));
                        }
                    }
                    response.payload
                }
                Err(_) => ResponsePayload::Error {
                    error: unknown_outcome(&method),
                },
            }
        }
        Err(_) => {
            end_span(
                span,
                akasha::OutcomeClass::Error,
                Some("response_serialization"),
            );
            ResponsePayload::Error {
                error: unknown_outcome(&method),
            }
        }
    }
}

fn bound_host_config(host: &HostConfig, meta: &CommandMeta) -> Result<HostConfig, AppError> {
    let store = crate::store::RoomStateStore::new(host.room_state_path(), host.room.clone());
    let spirit = store
        .embodied_spirit(&host.spirit)
        .map_err(AppError::Config)?;
    if meta.house_id != host.house_id
        || meta.sender_room != host.room
        || meta.sender_spirit != spirit
        || meta.sender_session.trim().is_empty()
    {
        return Err(AppError::Refusal {
            code: "foreign_organ_binding",
            message: "organ calls require this Host room's current identity",
        });
    }
    let mut bound = host.clone();
    bound.spirit = spirit;
    Ok(bound)
}

fn unknown_outcome(method: &str) -> ProtocolErrorBody {
    ProtocolErrorBody {
        code: "outcome_unknown".into(),
        message: format!(
            "native {method} completed without a usable response; reconcile before retry"
        ),
        retryable: false,
        details: Some(serde_json::json!({
            "execution": {"request_dispatched": true, "write_outcome": "unknown", "retry": "reconcile_first"}
        })),
    }
}

fn bound_request(
    host: &HostConfig,
    meta: &CommandMeta,
    request: OrganRequest,
) -> Result<ProtocolRequest, ProtocolError> {
    let mut params = request.params;
    if request.target_scope == protocol::organ::OrganTargetScope::House
        && !matches!(
            request.operation,
            OrganOperation::CanonRead | OrganOperation::CanonWrite | OrganOperation::Remember
        )
    {
        return Err(ProtocolError::InvalidParams(
            "House target scope is not allowed for this operation".into(),
        ));
    }
    for key in [
        "room",
        "room_dir",
        "roomDir",
        "spirit",
        "session",
        "houseId",
        "house_id",
        "requesterRoom",
        "requesterSpirit",
        "requesterSession",
        "successorSession",
    ] {
        if params.contains_key(key) {
            return Err(ProtocolError::InvalidParams(format!(
                "{key} belongs to the Host binding, not organ params"
            )));
        }
    }
    bind_identity(host, meta, request.operation, &mut params)?;
    if request.target_scope == protocol::organ::OrganTargetScope::House {
        put(&mut params, "room", "house");
    }
    let request = native::decode_envelope(RequestEnvelope {
        protocol: protocol::PROTOCOL_VERSION,
        id: meta.message_id.clone(),
        method: request.operation.method(),
        params: Value::Object(params),
    })
    .1?;
    match request {
        ProtocolRequest::Recall(recall) if !host.akasha_enabled() => {
            Ok(ProtocolRequest::VaultRecall(protocol::VaultRecallParams {
                room: host.room.clone(),
                room_dir: host.room_dir.to_string_lossy().into_owned(),
                query: recall.query().to_owned(),
            }))
        }
        request => Ok(request),
    }
}

fn put(params: &mut Map<String, Value>, key: &str, value: &str) {
    params.insert(key.into(), Value::String(value.into()));
}

fn bind_identity(
    host: &HostConfig,
    meta: &CommandMeta,
    operation: OrganOperation,
    params: &mut Map<String, Value>,
) -> Result<(), ProtocolError> {
    use OrganOperation::*;
    match operation {
        LessonUpdate | LessonDelete | DesignDocumentQuery | DesignDocumentWrite
        | SubstrateHealth => {}
        RestartRequest | RestartStatus | RestartTransition | RestartVerify => {
            return bind_restart(host, meta, operation, params);
        }
        _ => put(params, "room", &host.room),
    }
    match operation {
        HallwayCreate | HallwayJoin | HallwayPost | HallwayRead | HallwayInbox
        | HallwayKnockPolicy | HallwayKnock | QuestPost | QuestBoard | QuestClaim | QuestReport
        | QuestEvidence => {
            put(params, "spirit", &host.spirit);
            put(params, "session", &meta.sender_session);
        }
        LessonTriggerMatch => put(params, "session", &meta.sender_session),
        VaultRecall => put(params, "room_dir", &host.room_dir.to_string_lossy()),
        _ => {}
    }
    let draft = operation == QuestPost
        && matches!(
            params.get("action").and_then(Value::as_str),
            Some("goalDraft" | "draft")
        );
    if operation == QuestBoard || draft {
        put(params, "houseId", &host.house_id);
    }
    Ok(())
}

fn bind_restart(
    host: &HostConfig,
    meta: &CommandMeta,
    operation: OrganOperation,
    params: &mut Map<String, Value>,
) -> Result<(), ProtocolError> {
    use OrganOperation::*;
    match operation {
        RestartRequest => {
            put(params, "requesterRoom", &host.room);
            put(params, "requesterSpirit", &host.spirit);
            put(params, "requesterSession", &meta.sender_session);
        }
        RestartStatus => {}
        RestartVerify => {
            put(params, "room", &host.room);
            put(params, "spirit", &host.spirit);
            put(params, "successorSession", &meta.sender_session);
        }
        RestartTransition => {
            if params.get("to").and_then(Value::as_str) != Some("exiting") {
                return Err(ProtocolError::InvalidParams(
                    "the adapter may request only the exiting restart transition".into(),
                ));
            }
            put(params, "requesterSession", &meta.sender_session);
        }
        _ => unreachable!(),
    }
    Ok(())
}
