use super::*;
use protocol::{
    Failure, FailureClass, JUDGMENT_RESULT, LIFECYCLE_RESULT, ORGAN_RESULT, ROOM_STATE_RESULT,
};

pub(super) async fn room(state: &AppState, meta: CommandMeta, request: Value) -> Responses {
    let result = {
        let _runtime = state.runtime.lock().await;
        serde_json::from_value(request)
            .map_err(|error| {
                Failure::new(
                    FailureClass::InputRefused,
                    "invalid_room_request",
                    format!("invalid room request: {error}"),
                )
            })
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
            crate::judgment::execute(
                &state.config.room_dir,
                state.config.house_dir.as_deref(),
                &state.config.room,
                request,
            )
            .await
        }
        Err(_) => Err(Failure::new(
            FailureClass::InputRefused,
            "invalid_judgment_request",
            "invalid judgment request",
        )),
    };
    result_response(state, meta, JUDGMENT_RESULT, result).await
}

pub(super) async fn lifecycle(state: &AppState, meta: CommandMeta, request: Value) -> Responses {
    let result = match serde_json::from_value(request) {
        Ok(request) => {
            if let crate::lifecycle::LifecycleRequest::ChatNext { instance_id } = &request
                && let Err(owner) = state
                    .portable
                    .lock()
                    .await
                    .chat_recipient(instance_id.as_deref())
            {
                // Two attached sessions must never both answer one room turn.
                let result = Ok(json!({ "next": null, "deliveryOwner": owner }));
                return result_response(state, meta, LIFECYCLE_RESULT, result).await;
            }
            let messages = if matches!(
                &request,
                crate::lifecycle::LifecycleRequest::ChatNext { .. }
            ) {
                state.runtime.lock().await.chat.snapshot()
            } else {
                Vec::new()
            };
            crate::lifecycle::execute(request, &messages)
        }
        Err(error) => Err(Failure::new(
            FailureClass::InputRefused,
            "invalid_lifecycle_request",
            format!("invalid lifecycle request: {error}"),
        )),
    };
    result_response(state, meta, LIFECYCLE_RESULT, result).await
}

pub(super) async fn organ(
    state: &AppState,
    meta: CommandMeta,
    request: protocol::organ::OrganRequest,
) -> Responses {
    // A task child's view needs no registered conversation: its worker lineage is verified.
    let needs_session = match request.operation {
        protocol::organ::OrganOperation::OrihonMode
        | protocol::organ::OrganOperation::OrihonCapture => true,
        protocol::organ::OrganOperation::OrihonTurn => {
            request.params.get("purpose").and_then(Value::as_str) != Some("subagent")
        }
        _ => false,
    };

    if needs_session
        && !state
            .portable
            .lock()
            .await
            .live_conversations()
            .contains(&meta.sender_session)
    {
        // The old code word stays; the session's registration, not the call, is what failed.
        let failure = Failure::new(
            FailureClass::Conflict,
            "invalid_params",
            "invalid parameters: Orihon mode, capture and live turns require an active registered conversation",
        );
        return response(state, &meta, ORGAN_RESULT, json!({ "failure": failure })).await;
    }

    let payload = match crate::organ::call(
        &state.config,
        state.hallway_pool.as_ref(),
        &meta,
        request,
        &state.giga,
        &state.orihon,
    )
    .await
    {
        Ok(result) => json!({ "result": result }),
        Err(failure) => json!({ "failure": failure }),
    };
    response(state, &meta, ORGAN_RESULT, payload).await
}

async fn result_response(
    state: &AppState,
    meta: CommandMeta,
    kind: &str,
    result: Result<Value, Failure>,
) -> Responses {
    match result {
        Ok(result) => response(state, &meta, kind, json!({ "result": result })).await,
        Err(failure) => {
            let failed = format!("athanor.{}.command_failed", meta.projection_id);
            response(state, &meta, &failed, json!({ "failure": failure })).await
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
