//! Controls: commands issued at one exact instance, delivered, acknowledged.

use super::*;

pub(super) async fn command_status(
    state: &AppState,
    now: DateTime<Utc>,
    command_id: Uuid,
) -> Result<Value, Refusal> {
    let mut registry = state.portable.lock().await;
    registry.expire(now);
    let command = registry
        .commands
        .get(&command_id)
        .ok_or_else(|| Refusal::new(StatusCode::NOT_FOUND, "command_unknown", "no such command"))?;
    Ok(json!({
        "commandId": command_id,
        "kind": command.kind,
        "targetInstanceId": command.target,
        "state": command.state.as_str(),
        "detail": command.detail,
        "restartId": command.restart_id,
    }))
}

pub(super) async fn poll(
    state: &AppState,
    now: DateTime<Utc>,
    instance_id: Uuid,
    conversation_id: String,
) -> Result<Value, Refusal> {
    let revision = home::context(&state.config.room_dir)
        .ok()
        .map(|context| context.revision);
    let mut registry = state.portable.lock().await;
    registry.expire(now);
    registry.bound(instance_id, &conversation_id, now)?;
    let order: Vec<Uuid> = registry.command_order.iter().copied().collect();
    let mut commands = Vec::new();
    for id in order {
        let Some(command) = registry.commands.get_mut(&id) else {
            continue;
        };
        if command.target != instance_id || !command.state.deliverable() {
            continue;
        }
        command.state = CommandState::Delivered;
        commands.push(json!({
            "commandId": id,
            "kind": command.kind,
            "payload": command.payload,
            "issuedAt": now_text(command.issued_at),
            "deadlineAt": now_text(command.deadline_at),
            "issuerInstanceId": command.issuer,
            "targetConversationId": command.conversation_id,
        }));
    }
    Ok(json!({"contextRevision": revision, "commands": commands}))
}

fn validate_payload(kind: &str, payload: &Value) -> Result<(), Refusal> {
    let object = payload
        .as_object()
        .ok_or_else(|| Refusal::invalid("payload must be an object"))?;
    let text = |key: &str| object.get(key).and_then(Value::as_str);
    let allowed: &[&str] = match kind {
        "deliver" => {
            let body =
                text("text").ok_or_else(|| Refusal::invalid("deliver needs payload.text"))?;
            if body.is_empty() || body.len() > 65536 {
                return Err(Refusal::invalid("payload.text must be 1..65536 bytes"));
            }
            if let Some(mode) = object.get("mode")
                && !matches!(mode.as_str(), Some("steer" | "followUp"))
            {
                return Err(Refusal::invalid(
                    "payload.mode must be 'steer' or 'followUp'",
                ));
            }
            &["text", "mode"]
        }
        "interrupt" => &[],
        "compact" => {
            if text("instructions").is_some_and(|value| value.len() > 4096) {
                return Err(Refusal::invalid("payload.instructions exceeds 4096 bytes"));
            }
            &["instructions"]
        }
        "close" => &["reason"],
        "restart" => {
            let reason =
                text("reason").ok_or_else(|| Refusal::invalid("restart needs payload.reason"))?;
            if reason.is_empty() || reason.len() > 1024 {
                return Err(Refusal::invalid("payload.reason must be 1..1024 bytes"));
            }
            if let Some(mode) = object.get("mode")
                && !matches!(mode.as_str(), Some("resume" | "fresh"))
            {
                return Err(Refusal::invalid("payload.mode must be 'resume' or 'fresh'"));
            }
            &["reason", "mode"]
        }
        other => return Err(Refusal::invalid(format!("unknown command kind {other}"))),
    };
    if let Some(extra) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(Refusal::invalid(format!(
            "payload field {extra} is not part of {kind}"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn command(
    state: &AppState,
    now: DateTime<Utc>,
    command_id: Uuid,
    (target, target_conversation): (Uuid, String),
    issuer: Option<Uuid>,
    kind: String,
    payload: Value,
    fingerprint: Value,
) -> Result<Value, Refusal> {
    validate_payload(&kind, &payload)?;
    let fingerprint = ledger::digest(&fingerprint);
    let mut registry = state.portable.lock().await;
    registry.expire(now);
    if let Some(existing) = registry.commands.get(&command_id) {
        if existing.fingerprint != fingerprint {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "command_conflict",
                "commandId was used for a different command",
            ));
        }
        return Ok(json!({
            "commandId": command_id,
            "state": existing.state.as_str(),
            "detail": existing.detail,
            "restartId": existing.restart_id,
            "deadlineAt": now_text(existing.deadline_at),
        }));
    }
    let instance = registry
        .instances
        .get(&target)
        .filter(|instance| instance.connected(now))
        .ok_or_else(|| {
            Refusal::new(
                StatusCode::CONFLICT,
                "target_not_live",
                "the target instance holds no live lease",
            )
        })?;
    if instance.conversation_id != target_conversation {
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "conversation_mismatch",
            "the target instance is bound to a different conversation now",
        ));
    }
    if !instance.controls.iter().any(|control| *control == kind) {
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "control_unsupported",
            format!("the target does not support {kind}"),
        ));
    }
    // Every check that needs no effect runs before the journal takes the id.
    let restart = if kind == "restart" {
        let launcher = instance
            .restart_support()
            .map_err(|reason| Refusal::new(StatusCode::CONFLICT, "launcher_unsupported", reason))?;
        let fresh = payload["mode"].as_str() == Some("fresh");
        if !fresh {
            restart::require_saved_session(&instance.session_file)?;
        }
        Some(RestartRecord {
            intent_id: String::new(),
            target,
            conversation_id: instance.conversation_id.clone(),
            workspace: instance.workspace.clone(),
            session_file: instance.session_file.clone(),
            reason: payload["reason"].as_str().unwrap_or_default().to_owned(),
            launcher,
            stage: RestartStage::Requested,
            detail: None,
            terminal: instance.terminal.clone(),
            launch: instance.launch.clone(),
            claim_token: None,
            successor_proof: None,
            expected_pane: None,
            relaunch_by: None,
            successor: None,
            fresh,
            reported: false,
            updated_at: now,
        })
    } else {
        None
    };
    // The claim and the verify secrets meet their fence before any intent
    // exists: a refusal here journals nothing and mints nothing, and the
    // session that asked keeps running.
    if restart.is_some() {
        restart::require_relaunch_authority(state).await?;
    }
    let deadline_at = now
        + chrono::Duration::seconds(if restart.is_some() {
            akasha::REQUESTED_TTL_SECS
        } else {
            COMMAND_ACK_SECS
        });
    // Durable identity first. The lock is held through the native request, so
    // no poll meets this command before the journal and the authority agree.
    registry.commands.insert(
        command_id,
        Command {
            fingerprint,
            kind,
            payload: payload.clone(),
            target,
            issuer,
            issued_at: now,
            deadline_at,
            state: CommandState::Pending,
            detail: None,
            restart_id: None,
            conversation_id: target_conversation,
        },
    );
    registry.command_order.push_back(command_id);
    while registry.command_order.len() > COMMAND_BOUND {
        let Some(old) = registry.command_order.pop_front() else {
            break;
        };
        if registry
            .commands
            .get(&old)
            .is_some_and(|command| !command.state.terminal())
        {
            registry.command_order.push_back(old);
            break;
        }
        registry.commands.remove(&old);
    }
    if let Err(error) = ledger::save(&state.config.state_dir, &registry) {
        // Nothing has happened yet: the id is free again and the caller may retry.
        registry.commands.remove(&command_id);
        registry.command_order.retain(|id| *id != command_id);
        return Err(Refusal::internal(format!(
            "the control journal could not record this command; nothing was requested: {error}"
        )));
    }
    let mut restart_id = None;
    if let Some(mut record) = restart {
        let requested = restart::request(
            state,
            command_id,
            target,
            &record.conversation_id,
            &record.workspace,
            &record.reason,
            record.fresh,
        )
        .await;
        let intent_id = match requested {
            Ok(intent_id) => intent_id,
            Err(refusal) => {
                // A named refusal from the authority minted no intent. Anything
                // else (a lost answer, an internal fault) may have: that
                // command stays outcomeUnknown and is never asked again; the
                // intent, if one exists, lapses by its own TTL.
                let settled = if refusal.code == "internal" {
                    CommandState::OutcomeUnknown
                } else {
                    CommandState::Refused
                };
                settle_pending(&mut registry, command_id, settled, &refusal.message);
                if let Err(error) = ledger::save(&state.config.state_dir, &registry) {
                    tracing::error!(%error, %command_id, "portable command receipt could not be persisted");
                }
                return Err(refusal);
            }
        };
        record.intent_id = intent_id;
        let id = Uuid::new_v4();
        let mut delivered = json!({
            "reason": record.reason,
            "restartId": id,
            "launcher": record.launcher.as_str(),
            "mode": if record.fresh { "fresh" } else { "resume" },
        });
        if record.launcher == LauncherKind::Keeper {
            delivered["exitCode"] = json!(KEEPER_EXIT_CODE);
        }
        registry.restarts.insert(id, record);
        if let Some(command) = registry.commands.get_mut(&command_id) {
            command.payload = delivered;
            command.restart_id = Some(id);
        }
        restart_id = Some(id);
    }
    if let Some(command) = registry.commands.get_mut(&command_id) {
        command.state = CommandState::Queued;
    }
    // The receipt commits before anyone may poll it. A journal that cannot
    // record the receipt leaves the command undeliverable: the intent, if any,
    // is never armed and lapses by its own TTL.
    if let Err(error) = ledger::save(&state.config.state_dir, &registry) {
        if let Some(id) = restart_id {
            registry.restarts.remove(&id);
        }
        settle_pending(
            &mut registry,
            command_id,
            CommandState::OutcomeUnknown,
            "the control journal could not record the receipt; nothing was delivered",
        );
        return Err(Refusal::internal(format!(
            "the control journal could not record this command's receipt; nothing was delivered: {error}"
        )));
    }
    Ok(
        json!({"commandId": command_id, "state": "queued", "restartId": restart_id, "deadlineAt": now_text(deadline_at)}),
    )
}

/// Close a journaled command that never became deliverable.
fn settle_pending(registry: &mut Registry, command_id: Uuid, state: CommandState, detail: &str) {
    if let Some(command) = registry.commands.get_mut(&command_id) {
        command.state = state;
        command.detail = Some(detail.chars().take(1024).collect());
        command.restart_id = None;
    }
}

pub(super) async fn acknowledge(
    state: &AppState,
    now: DateTime<Utc>,
    instance_id: Uuid,
    conversation_id: String,
    command_id: Uuid,
    outcome: String,
    detail: Option<String>,
) -> Result<Value, Refusal> {
    let reached = match outcome.as_str() {
        "applied" => CommandState::Applied,
        "refused" => CommandState::Refused,
        "failed" => CommandState::Failed,
        _ => {
            return Err(Refusal::invalid(
                "outcome must be applied, refused or failed",
            ));
        }
    };
    let detail = detail.map(|detail| detail.chars().take(1024).collect::<String>());
    let mut registry = state.portable.lock().await;
    registry.expire(now);
    registry.bound(instance_id, &conversation_id, now)?;
    let command = registry
        .commands
        .get(&command_id)
        .filter(|command| command.target == instance_id)
        .ok_or_else(|| {
            Refusal::new(
                StatusCode::NOT_FOUND,
                "command_unknown",
                "no such command for this instance",
            )
        })?;
    if command.state.terminal() {
        if command.state == reached {
            if reached == CommandState::Applied {
                if let Some(restart_id) = command.restart_id {
                    let record = registry.restarts.get(&restart_id).ok_or_else(|| {
                        Refusal::internal("the restart acknowledgement has no recorded intent")
                    })?;
                    if record.stage != RestartStage::AwaitingExit {
                        return Err(Refusal::new(
                            StatusCode::CONFLICT,
                            "restart_not_exiting",
                            "the restart no longer awaits this process exit",
                        ));
                    }
                    // A replay answered 200 lets the adapter exit too.
                    restart::require_relaunch_authority(state).await?;
                    restart::confirm_exit(state, record).await?;
                }
            }
            return Ok(json!({"commandId": command_id, "state": reached.as_str()}));
        }
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "command_conflict",
            format!("command already {}", command.state.as_str()),
        ));
    }
    let restart_id = command.restart_id;
    let record = restart_id.and_then(|id| registry.restarts.get(&id).cloned());
    if let (Some(id), Some(record)) = (restart_id, record) {
        if reached == CommandState::Applied {
            // One driver per restart: only a record still at Requested arms.
            if record.stage != RestartStage::Requested {
                return Err(Refusal::new(
                    StatusCode::CONFLICT,
                    "command_conflict",
                    format!("restart already {}", record.stage.as_str()),
                ));
            }
            // Arm before answering: the adapter exits only after this 200.
            if let Err(refusal) = restart::arm(state, &record).await {
                // No definitive receipt (authority unreachable, internal): the
                // command stays delivered and the same acknowledge may be retried.
                if matches!(refusal.code, "internal" | "restart_authority_unavailable") {
                    return Err(refusal.with("retryable", json!(true)));
                }
                if let Some(command) = registry.commands.get_mut(&command_id) {
                    command.state = CommandState::Failed;
                    command.detail = Some(refusal.message.clone());
                }
                if let Some(stored) = registry.restarts.get_mut(&id) {
                    stored.stage = RestartStage::Failed;
                    stored.detail = Some(refusal.message.clone());
                }
                drop(registry);
                restart::fail(state, &record, &refusal.message).await;
                return Err(refusal);
            }
            // The exit is armed in the authority. Commit the receipt before
            // any driver exists: a journal that cannot record it answers
            // retryable, the replayed acknowledge finds the arm already done,
            // and no uncommitted driver ever relaunches.
            if let Some(command) = registry.commands.get_mut(&command_id) {
                command.state = CommandState::Applied;
                command.detail = detail.clone();
            }
            if let Some(stored) = registry.restarts.get_mut(&id) {
                stored.stage = RestartStage::AwaitingExit;
                stored.updated_at = now;
            }
            if let Err(error) = ledger::save(&state.config.state_dir, &registry) {
                if let Some(command) = registry.commands.get_mut(&command_id) {
                    command.state = CommandState::Delivered;
                    command.detail = None;
                }
                if let Some(stored) = registry.restarts.get_mut(&id) {
                    stored.stage = RestartStage::Requested;
                }
                return Err(Refusal::internal(format!(
                    "the control journal could not record the applied restart; acknowledge again: {error}"
                ))
                .with("retryable", json!(true)));
            }
            state.tasks.spawn(restart::drive(state.clone(), id));
            return Ok(json!({"commandId": command_id, "state": reached.as_str()}));
        } else {
            let why = detail
                .clone()
                .unwrap_or_else(|| format!("target {outcome} the restart"));
            if let Some(stored) = registry.restarts.get_mut(&id) {
                stored.stage = RestartStage::Failed;
                stored.detail = Some(why.clone());
            }
            if let Some(command) = registry.commands.get_mut(&command_id) {
                command.state = reached;
                command.detail = detail;
            }
            drop(registry);
            restart::fail(state, &record, &why).await;
            return Ok(json!({"commandId": command_id, "state": reached.as_str()}));
        }
    }
    if let Some(command) = registry.commands.get_mut(&command_id) {
        command.state = reached;
        command.detail = detail;
    }
    Ok(json!({"commandId": command_id, "state": reached.as_str()}))
}
