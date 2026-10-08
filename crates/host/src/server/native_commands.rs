use super::*;
use protocol::{JUDGMENT_RESULT, LIFECYCLE_RESULT, ORGAN_RESULT, ROOM_STATE_RESULT};

pub(super) async fn room(state: &AppState, meta: CommandMeta, request: Value) -> Responses {
    let result = {
        let _runtime = state.runtime.lock().await;
        serde_json::from_value(request)
            .map_err(|error| format!("invalid room request: {error}"))
            .and_then(|request| {
                crate::room_state::execute(
                    &state.room_store,
                    &state.config.room_dir,
                    &state.config.spirit,
                    request,
                )
            })
    };
    result_response(state, meta, ROOM_STATE_RESULT, result).await
}

pub(super) async fn judgment(state: &AppState, meta: CommandMeta, request: Value) -> Responses {
    let result = match serde_json::from_value(request) {
        Ok(request) => {
            crate::judgment::execute(&state.config.room_dir, &state.config.room, request).await
        }
        Err(_) => Err("invalid judgment request".into()),
    };
    result_response(state, meta, JUDGMENT_RESULT, result).await
}

pub(super) async fn lifecycle(state: &AppState, meta: CommandMeta, request: Value) -> Responses {
    let result = match serde_json::from_value(request) {
        Ok(request) => {
            let messages = if matches!(&request, crate::lifecycle::LifecycleRequest::ChatNext) {
                state.runtime.lock().await.chat.snapshot()
            } else {
                Vec::new()
            };
            crate::lifecycle::execute(request, &messages)
        }
        Err(error) => Err(format!("invalid lifecycle request: {error}")),
    };
    result_response(state, meta, LIFECYCLE_RESULT, result).await
}

pub(super) async fn organ(
    state: &AppState,
    meta: CommandMeta,
    request: protocol::organ::OrganRequest,
) -> Responses {
    let payload = crate::organ::call(
        &state.config,
        state.hallway_pool.as_ref(),
        &meta,
        request,
        &state.giga,
    )
    .await;
    response(
        state,
        &meta,
        ORGAN_RESULT,
        serde_json::to_value(payload).expect("native payload serializes"),
    )
    .await
}

async fn result_response(
    state: &AppState,
    meta: CommandMeta,
    kind: &str,
    result: Result<Value, String>,
) -> Responses {
    match result {
        Ok(result) => response(state, &meta, kind, json!({ "result": result })).await,
        Err(reason) => {
            let failure = format!("athanor.{}.command_failed", meta.projection_id);
            response(state, &meta, &failure, json!({
                "reason": reason,
                "error": { "code": "native_request_failed", "message": reason, "retryable": false }
            })).await
        }
    }
}

async fn response(state: &AppState, meta: &CommandMeta, kind: &str, payload: Value) -> Responses {
    let sequence = state.runtime.lock().await.cursor.sequence;
    let mut event = serde_json::to_value(event_meta_for_projection(
        state,
        Some(meta),
        &meta.message_id,
        &meta.idempotency_key,
        kind,
        &meta.projection_id,
        sequence,
        body_hash(&payload).expect("native payload hashes"),
        new_id(),
    ))
    .expect("native event metadata serializes");
    event
        .as_object_mut()
        .expect("event metadata is an object")
        .extend(
            payload
                .as_object()
                .expect("native payload is an object")
                .clone(),
        );
    Responses {
        direct: vec![serialize(&event)],
        delta: None,
    }
}
