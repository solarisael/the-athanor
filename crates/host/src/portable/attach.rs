//! Attachment: one exact live instance registers, keeps its lease, and leaves.

use super::*;

/// The attach request as the wire names it.
pub(super) struct Attach {
    pub(super) conversation_id: String,
    pub(super) instance_id: Uuid,
    pub(super) workspace: String,
    pub(super) harness: String,
    pub(super) session_file: String,
    pub(super) process_id: u32,
    pub(super) controls: Vec<String>,
    pub(super) chat_delivery: bool,
    pub(super) terminal: Option<Terminal>,
    pub(super) launcher: Option<LauncherRequest>,
}

pub(super) async fn attach(
    state: &AppState,
    now: DateTime<Utc>,
    request: Attach,
) -> Result<Value, Refusal> {
    let Attach {
        conversation_id,
        instance_id,
        workspace,
        harness,
        session_file,
        process_id,
        controls,
        chat_delivery,
        terminal,
        launcher,
    } = request;
    bounded(&conversation_id, "conversationId", 256)?;
    absolute(&workspace, "workspace")?;
    absolute(&session_file, "sessionFile")?;
    if harness != "omp" {
        return Err(Refusal::invalid("harness must be 'omp'"));
    }
    if !session_file.ends_with(".jsonl") {
        return Err(Refusal::invalid(
            "sessionFile must be a .jsonl session file",
        ));
    }
    if process_id == 0 {
        return Err(Refusal::invalid("processId must be positive"));
    }
    if let Some(unknown) = controls
        .iter()
        .find(|control| !CONTROLS.contains(&control.as_str()))
    {
        return Err(Refusal::invalid(format!("unknown control {unknown}")));
    }
    if let Some(terminal) = &terminal {
        for (value, field) in [
            (&terminal.session, "terminal.session"),
            (&terminal.pane, "terminal.pane"),
        ] {
            bounded(value, field, 64)?;
            if !value.chars().all(|c| c.is_ascii_digit()) {
                return Err(Refusal::invalid(format!(
                    "{field} must be the numeric Tern id as a decimal string"
                )));
            }
        }
        bounded(&terminal.socket, "terminal.socket", 4096)?;
        #[cfg(windows)]
        if terminal
            .socket
            .get(..9)
            .is_none_or(|prefix| !prefix.eq_ignore_ascii_case(r"\\.\pipe\"))
        {
            return Err(Refusal::invalid(
                "terminal.socket must name a local Windows pipe",
            ));
        }
        #[cfg(not(windows))]
        absolute(&terminal.socket, "terminal.socket")?;
        absolute(&terminal.executable, "terminal.executable")?;
    }
    let (launcher_kind, launch) = match launcher {
        None => (None, None),
        Some(LauncherRequest::Tern {
            program,
            args,
            environment,
        }) => {
            if terminal.is_none() {
                return Err(Refusal::invalid("a tern launcher needs terminal identity"));
            }
            (
                Some(LauncherKind::Tern),
                Some(launch::Launch::validated(&program, &args, environment)?),
            )
        }
        Some(LauncherRequest::Keeper { keeper_instance }) => {
            if keeper_instance != instance_id {
                return Err(Refusal::invalid(
                    "a keeper launcher registers under the keeper's child instance id",
                ));
            }
            (Some(LauncherKind::Keeper), None)
        }
    };
    let context = home::context(&state.config.room_dir)?;
    // Display identity is the room's bound state, read only: attach never
    // writes embodiedSpirit/operator, so it cannot skew later WS validation.
    let identity = state
        .room_store
        .identity(&state.config.spirit)
        .map_err(|error| {
            Refusal::new(StatusCode::SERVICE_UNAVAILABLE, "home_unavailable", error)
        })?;
    let mut registry = state.portable.lock().await;
    registry.expire(now);
    if registry.retired.contains(&instance_id) {
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "instance_stale",
            "this instance was retired",
        ));
    }
    if let Some(existing) = registry.instances.get(&instance_id)
        && existing.process_id == process_id
        && existing.conversation_id != conversation_id
    {
        // A rebind may not strand a restart that names the old conversation.
        let restart_live = registry.restarts.values().any(|record| {
            record.target == instance_id
                && matches!(
                    record.stage,
                    RestartStage::Requested
                        | RestartStage::AwaitingExit
                        | RestartStage::Relaunching
                )
        });
        if restart_live {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "restart_pending",
                "settle the pending restart before switching conversation",
            ));
        }
    }
    let reused_watch = match registry.instances.get(&instance_id) {
        Some(existing) if existing.process_id != process_id => {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "instance_conflict",
                "instanceId is registered to another process",
            ));
        }
        Some(existing) => existing.watch.clone(),
        None => None,
    };
    let owner = registry.chat_owner_live(now);
    if chat_delivery && owner.is_some_and(|owner| owner != instance_id) {
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "chat_owner_taken",
            "another live session owns room chat",
        ))
        .map_err(|refusal| refusal.with("owner", json!(owner)));
    }
    let watch = match reused_watch {
        Some(watch) => Some(watch),
        None => ProcessWatch::open(process_id).ok().map(Arc::new),
    };
    // Capability, not authority: a launch that does not match the observed
    // process image and the Host's own Tern is dropped, so restart reports
    // no usable launcher instead of running a caller-named executable.
    let launch = launch.and_then(|launch| {
        launch
            .bind(watch.as_deref(), terminal.as_ref())
            .inspect_err(|reason| {
                tracing::warn!(%instance_id, %reason, "portable tern launcher refused");
            })
            .ok()
    });
    let instance = Instance {
        conversation_id: conversation_id.clone(),
        workspace: workspace.clone(),
        session_file: session_file.clone(),
        process_id,
        watch,
        controls: controls.clone(),
        terminal: terminal.clone(),
        launcher: launcher_kind,
        launch,
        lease_expires: now + chrono::Duration::seconds(LEASE_SECS),
        attached_at: registry
            .instances
            .get(&instance_id)
            .map_or(now, |existing| existing.attached_at),
    };
    if instance.exited() {
        return Err(Refusal::invalid("processId is not a running process"));
    }
    let support = instance.restart_support();
    registry.instances.insert(instance_id, instance);
    if chat_delivery {
        registry.chat_owner = Some(instance_id);
    }
    let successor = restart::verify_successor(
        state,
        &mut registry,
        instance_id,
        &conversation_id,
        &session_file,
        &workspace,
        terminal.as_ref(),
    )
    .await?;
    if let Some(successor) = &successor
        && let Some(predecessor) = successor["predecessorInstanceId"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
    {
        registry.retire(predecessor, "restarted");
    }
    if let Some(successor) = &successor
        && let Some(id) = successor["restartId"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
        && let Some(record) = registry.restarts.get_mut(&id)
    {
        record.reported = true;
    }
    // Controls frozen to an earlier conversation never reach this one.
    for command in registry.commands.values_mut() {
        if command.target == instance_id
            && !command.state.terminal()
            && command.conversation_id != conversation_id
        {
            command.state = CommandState::Expired;
            command.detail = Some("the instance switched conversation".into());
        }
    }
    let mut answer = json!({
        "room": identity.room,
        "spirit": identity.spirit,
        "operator": identity.operator,
        "homeDir": state.config.room_dir.display().to_string(),
        "instanceId": instance_id,
        "conversationId": conversation_id,
        "workspace": workspace,
        "context": context_json(&context),
        "controls": controls,
        "leaseSecs": LEASE_SECS,
        "heartbeatSecs": HEARTBEAT_SECS,
        "leaseExpiresAt": now_text(now + chrono::Duration::seconds(LEASE_SECS)),
        "chatDelivery": registry.chat_owner == Some(instance_id),
        "restart": match support {
            Ok(kind) => json!({"supported": true, "launcher": kind.as_str()}),
            Err(reason) => json!({"supported": false, "launcher": launcher_kind.map(LauncherKind::as_str), "reason": reason}),
        },
    });
    if let Some(successor) = successor {
        answer["restartSuccessor"] = successor;
    }
    Ok(answer)
}

pub(super) async fn context(
    state: &AppState,
    now: DateTime<Utc>,
    instance_id: Uuid,
    conversation_id: String,
) -> Result<Value, Refusal> {
    state
        .portable
        .lock()
        .await
        .bound(instance_id, &conversation_id, now)?;
    Ok(json!({"context": context_json(&home::context(&state.config.room_dir)?)}))
}

pub(super) async fn heartbeat(
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
    let lease = registry
        .bound(instance_id, &conversation_id, now)?
        .lease_expires;
    let notice = registry
        .restarts
        .iter_mut()
        .find(|(_, record)| {
            record.stage == RestartStage::Verified
                && record.successor == Some(instance_id)
                && !record.reported
        })
        .map(|(id, record)| {
            record.reported = true;
            restart::successor_notice(*id, record)
        });
    let mut answer = json!({
        "leaseExpiresAt": now_text(lease),
        "contextRevision": revision,
        "pendingCommands": registry.pending_for(instance_id),
        "chatDelivery": registry.chat_owner_live(now) == Some(instance_id),
    });
    if let Some(notice) = notice {
        answer["restartSuccessor"] = notice;
    }
    Ok(answer)
}

pub(super) async fn detach(
    state: &AppState,
    instance_id: Uuid,
    conversation_id: String,
    reason: Option<String>,
) -> Result<Value, Refusal> {
    let mut registry = state.portable.lock().await;
    if registry.retired.contains(&instance_id) {
        return Ok(json!({"detached": true}));
    }
    match registry.instances.get(&instance_id) {
        Some(instance) if instance.conversation_id != conversation_id => {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "conversation_mismatch",
                "conversationId does not match the registration",
            ));
        }
        Some(_) => {}
        None => {
            return Err(Refusal::new(
                StatusCode::NOT_FOUND,
                "instance_unknown",
                "nothing is attached under this instanceId",
            ));
        }
    }
    let why = reason
        .map(|reason| reason.chars().take(512).collect::<String>())
        .unwrap_or_else(|| "detached".into());
    registry.retire(instance_id, &why);
    Ok(json!({"detached": true}))
}

pub(super) async fn chat_delivery(
    state: &AppState,
    now: DateTime<Utc>,
    instance_id: Uuid,
    conversation_id: String,
    claim: bool,
) -> Result<Value, Refusal> {
    let mut registry = state.portable.lock().await;
    registry.bound(instance_id, &conversation_id, now)?;
    let owner = registry.chat_owner_live(now);
    if claim {
        if owner.is_some_and(|owner| owner != instance_id) {
            return Err(Refusal::new(
                StatusCode::CONFLICT,
                "chat_owner_taken",
                "another live session owns room chat",
            )
            .with("owner", json!(owner)));
        }
        registry.chat_owner = Some(instance_id);
    } else if owner == Some(instance_id) {
        registry.chat_owner = None;
    }
    let owner = registry.chat_owner_live(now);
    Ok(json!({"chatDelivery": owner == Some(instance_id), "owner": owner}))
}
