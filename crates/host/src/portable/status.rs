//! Status: what the Host registered, not what a lock file implies.

use super::*;

pub(super) async fn status(state: &AppState, now: DateTime<Utc>) -> Result<Value, Refusal> {
    let mut registry = state.portable.lock().await;
    registry.expire(now);
    let owner = registry.chat_owner_live(now);
    let sessions: Vec<Value> = registry
                .instances
                .iter()
                .map(|(id, instance)| {
                    let support = instance.restart_support();
                    let unsupported: Vec<&str> = CONTROLS
                        .iter()
                        .copied()
                        .filter(|control| {
                            !instance.controls.iter().any(|offered| offered == control)
                                || (*control == "restart" && support.is_err())
                        })
                        .collect();
                    json!({
                        "instanceId": id,
                        "conversationId": instance.conversation_id,
                        "workspace": instance.workspace,
                        "harness": "omp",
                        "processId": instance.process_id,
                        "sessionFile": instance.session_file,
                        "connected": instance.connected(now),
                        "processExited": instance.exited(),
                        "processObserved": instance.watch.is_some(),
                        "leaseExpiresAt": now_text(instance.lease_expires),
                        "attachedAt": now_text(instance.attached_at),
                        "controls": instance.controls,
                        "chatDelivery": owner == Some(*id),
                        "terminal": instance.terminal.as_ref().map(|terminal| json!({"kind": "tern", "session": terminal.session, "pane": terminal.pane, "socket": terminal.socket})),
                        "launcher": instance.launcher.map(|kind| match &support {
                            Ok(_) => json!({"kind": kind.as_str(), "restart": true}),
                            Err(reason) => json!({"kind": kind.as_str(), "restart": false, "reason": reason}),
                        }),
                        "unsupportedControls": unsupported,
                    })
                })
                .collect();
    let restarts: Vec<Value> = registry
        .restarts
        .iter()
        .map(|(id, record)| {
            json!({
                "restartId": id,
                "intentId": record.intent_id,
                "targetInstanceId": record.target,
                "conversationId": record.conversation_id,
                "launcher": record.launcher.as_str(),
                "state": record.stage.as_str(),
                "detail": record.detail,
                "successorInstanceId": record.successor,
                "updatedAt": now_text(record.updated_at),
            })
        })
        .collect();
    Ok(json!({
        "room": state.config.room,
        "spirit": state.config.spirit,
        "chatDeliveryOwner": owner,
        "restartAuthority": state.hallway_pool.is_some(),
        "sessions": sessions,
        "restarts": restarts,
    }))
}
