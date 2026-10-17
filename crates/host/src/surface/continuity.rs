use super::{AppState, EmptyRequest, error, payload};
use akasha::{AppError, Config};
use axum::Json;
use axum::extract::{State, rejection::JsonRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hearth::{OperatorKey, RememberMemoryDetails, RememberRequest, RoomKey};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WriteKind {
    Memory,
    PaperBoat,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteRequest {
    kind: WriteKind,
    title: Option<String>,
    body: String,
    #[serde(rename = "expectedOperator")]
    expected_operator: String,
}

pub(super) async fn status(
    State(state): State<AppState>,
    body: Result<Json<EmptyRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = payload(body) {
        return response;
    }
    let identity = state.room_store.identity(&state.config.spirit);
    let operator = identity
        .as_ref()
        .ok()
        .map(|identity| identity.operator.as_str());
    let unavailable = match identity.as_ref() {
        Err(reason) => Some(reason.clone()),
        Ok(_) => match state
            .config
            .operator
            .as_ref()
            .map(|key| OperatorKey::new(key.clone()))
            .transpose()
        {
            Err(reason) => Some(reason.to_string()),
            Ok(_) => runtime(&state)
                .await
                .err()
                .map(|reason| reason.protocol_error_body("continuity_status").message),
        },
    };
    Json(json!({
        "ok": true,
        "room": state.config.room,
        "operator": operator,
        "writesAvailable": unavailable.is_none(),
        "unavailableReason": unavailable,
    }))
    .into_response()
}

pub(super) async fn write(
    State(state): State<AppState>,
    body: Result<Json<WriteRequest>, JsonRejection>,
) -> Response {
    let request = match payload(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let identity = match state.room_store.identity(&state.config.spirit) {
        Ok(identity) => identity,
        Err(reason) => return error(StatusCode::SERVICE_UNAVAILABLE, reason),
    };
    if request.expected_operator != identity.operator {
        return error(
            StatusCode::CONFLICT,
            "the operator changed; refresh status and review the write again",
        );
    }
    let operator = match state
        .config
        .operator
        .as_ref()
        .map(|key| OperatorKey::new(key.clone()))
        .transpose()
    {
        Ok(operator) => operator,
        Err(reason) => return error(StatusCode::SERVICE_UNAVAILABLE, reason),
    };
    let (config, pool) = match runtime(&state).await {
        Ok(runtime) => runtime,
        Err(reason) => return write_error(reason),
    };
    let result = match request.kind {
        WriteKind::Memory => {
            let Some(title) = request.title else {
                return error(StatusCode::BAD_REQUEST, "title is required for memory");
            };
            let room = match RoomKey::for_memory_write(identity.room.clone()) {
                Ok(room) => room,
                Err(reason) => return error(StatusCode::BAD_REQUEST, reason),
            };
            let memory = RememberRequest::new_memory(
                room,
                title,
                request.body,
                RememberMemoryDetails {
                    source_path: None,
                    threads: Vec::new(),
                    continues: Vec::new(),
                    supersedes: Vec::new(),
                    backup: false,
                },
            )
            .and_then(|memory| memory.with_operator(operator, None))
            .and_then(|memory| memory.authored_by_operator(identity.operator.clone()));
            let memory = match memory {
                Ok(memory) => memory,
                Err(reason) => return error(StatusCode::BAD_REQUEST, reason),
            };
            akasha::remember(pool, &config, memory)
                .await
                .map(|receipt| ("memory", json!(receipt)))
        }
        WriteKind::PaperBoat => {
            let boat = origami::boats::paper_boat::PaperBoatSleepRequest::new(
                identity.room.clone(),
                request.body,
                false,
            )
            .and_then(|boat| boat.authored_by_operator(identity.operator.clone(), operator));
            let boat = match boat {
                Ok(boat) => boat,
                Err(reason) => return error(StatusCode::BAD_REQUEST, reason),
            };
            akasha::paper_boat_sleep(pool, &config, boat)
                .await
                .map(|receipt| {
                    (
                        "paper-boat",
                        json!(protocol::PaperBoatSleepResult::from(receipt)),
                    )
                })
        }
    };
    match result {
        Ok((kind, receipt)) => Json(json!({
            "ok": true,
            "kind": kind,
            "room": identity.room,
            "author": { "kind": "operator", "name": identity.operator },
            "receipt": receipt,
        }))
        .into_response(),
        Err(reason) => write_error(reason),
    }
}

async fn runtime(state: &AppState) -> Result<(Config, &PgPool), AppError> {
    let config = crate::organ::substrate_config(&state.config)?;
    let pool = state
        .hallway_pool
        .as_ref()
        .ok_or_else(|| AppError::Config("the Host native runtime is unavailable".into()))?;
    Config::validate_pool(pool).await?;
    Ok((config, pool))
}

fn write_error(reason: AppError) -> Response {
    let status = match &reason {
        AppError::Invalid(_) => StatusCode::BAD_REQUEST,
        AppError::Refusal { .. } => StatusCode::FORBIDDEN,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    error(
        status,
        reason.protocol_error_body("continuity_write").message,
    )
}
