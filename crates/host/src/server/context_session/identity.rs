use super::*;

pub(super) struct IdentitySync {
    pub invalidation: Option<&'static str>,
    pub adoption: &'static str,
}

pub(super) async fn synchronize(
    state: &AppState,
    meta: &CommandMeta,
    context: &mut ContextSession,
    saved: &mut SavedSession,
    request: &mut ContextPrepareRequest,
) -> Result<IdentitySync, String> {
    let mut runtime = state.runtime.lock().await;
    let authentication = authenticate_presence(state, meta)?;
    let root = crate::room_state::execute(
        &state.room_store,
        &state.config.room_dir,
        &state.config.spirit,
        crate::room_state::RoomStateRequest::Read,
    )?;
    if root["embodiedSpirit"] != authentication.binding.spirit
        || root["operator"] != authentication.binding.operator
    {
        return Err(
            "room identity changed while context prepared; retry with its current binding".into(),
        );
    }
    let epoch = fingerprint(&root)?;
    adopt_presence_from_store(state, &mut runtime, &meta.sender_session)
        .await
        .map_err(|error| format!("Presence identity read failed: {error}"))?;
    let frame = runtime
        .presence
        .session_state(&meta.sender_session)
        .map(|(frame, ledger)| (frame.clone(), ledger.clone()));
    if frame.as_ref().is_some_and(|(frame, _)| {
        frame.binding.room != authentication.binding.room
            || frame.binding.session != authentication.binding.session
    }) {
        return Err("Presence frame belongs to another room or session; rotation refused".into());
    }
    let proven = frame
        .as_ref()
        .is_some_and(|(frame, _)| frame.binding == authentication.binding);
    let unlabelled = saved.identity_epoch.is_none();
    let changed = saved
        .identity_epoch
        .as_ref()
        .is_some_and(|previous| previous != &epoch);
    let foreign_frame = frame
        .as_ref()
        .is_some_and(|(frame, _)| frame.binding != authentication.binding);
    let cache_failure =
        cached_presence_failure(&runtime, saved, &request.turn_id, &authentication.binding);
    let invalidation = if changed || foreign_frame {
        Some("identity-epoch-changed")
    } else if cache_failure.is_some() {
        cache_failure
    } else if unlabelled && request.legacy_rejection.is_some() {
        Some("legacy-context-unpaired-surrogate")
    } else if unlabelled && !saved.turns.is_empty() && !proven {
        Some("legacy-context-identity-unverified")
    } else {
        None
    };
    let adoption = if unlabelled && invalidation.is_some() {
        "rebuilt"
    } else if unlabelled && !saved.turns.is_empty() {
        "preserved"
    } else {
        "native"
    };
    let displaced = invalidation.is_some() && !saved.turns.is_empty();
    if let Some(reason) = invalidation.filter(|_| unlabelled || changed || displaced) {
        saved.last_invalidation = Some(cache::CacheInvalidation {
            reason: reason.into(),
            prior_generation: saved.generation.clone(),
            prior_content_sha256: body_hash(
                &serde_json::to_value(&saved.turns).map_err(|error| error.to_string())?,
            )?,
        });
    }
    if invalidation.is_some() {
        saved.turns.clear();
        context.sieves.clear();
        request.existing_block_kinds.clear();
    }
    if unlabelled || changed || displaced || saved.generation.is_empty() {
        saved.identity_epoch = Some(epoch);
        saved.generation = new_id();
        cache::commit(state, &meta.sender_session, saved)
            .map_err(|error| format!("context identity journal failed: {error}"))?;
        context.saved = Some(saved.clone());
    }
    if foreign_frame && request.capabilities.top_level {
        let (old_frame, ledger) = frame.as_ref().expect("foreign frame exists");
        rotate(
            state,
            &mut runtime,
            meta,
            &authentication,
            old_frame,
            ledger,
            &saved.generation,
            request,
        )
        .await?;
    }
    request.prior_presence = runtime
        .presence
        .session_state(&meta.sender_session)
        .filter(|(frame, _)| frame.binding == authentication.binding)
        .map(|(frame, _)| PriorPresence {
            frame_id: frame.frame_id.clone(),
            frame_rendered: frame.rendered.clone(),
            frame_version: Some(frame.version),
        });
    Ok(IdentitySync {
        invalidation,
        adoption,
    })
}

async fn rotate(
    state: &AppState,
    runtime: &mut RuntimeState,
    meta: &CommandMeta,
    authentication: &PresenceAuthentication,
    frame: &summoning::presence::PresenceFrame,
    ledger: &PresenceLedger,
    generation: &str,
    request: &ContextPrepareRequest,
) -> Result<(), String> {
    let analysis = hearth::context::analyze_context(
        &authentication.binding.room,
        hearth::context::ContextAnalysisRequest {
            prompt: request.prompt.clone(),
            recognized_entities: Vec::new(),
            context_characters: request.context_characters,
            active_spirit: authentication.binding.spirit.clone(),
            operator: authentication.binding.operator.clone(),
            routing_mode_enabled: false,
        },
        0,
    )
    .map_err(|error| error.to_string())?;
    let body = clip(&analysis.room_reminder, 4096);
    let identity = materials::material(
        "identity:active-spirit".into(),
        json!({"kind":"identity","source":"active_spirit.md","sha256":format!("{:x}",Sha256::digest(body.as_bytes()))}),
        "identity",
        body,
        1000,
    );
    let open: PresenceOpenRequest = serde_json::from_value(json!({
        "binding":authentication.binding,"identity":[identity],"relationship":materials::pulse(state)?.into_iter().collect::<Vec<_>>(),
        "continuity":[],"anamnesis":[],"previousBoat":null,"uncertainties":[]
    })).map_err(|error| error.to_string())?;
    summoning::presence::open_presence(authentication.clone(), open.clone())
        .map_err(|error| error.to_string())?;
    let close = PresenceCloseRequest {
        frame_id: frame.frame_id.clone(),
        frame_version: frame.version,
        body: "Authenticated room identity changed.".into(),
    };
    // The durable close must succeed before the runtime retires its live contract.
    summoning::presence::close_presence(frame, ledger, close.clone())
        .map_err(|error| error.to_string())?;
    if let Some(pool) = state.hallway_pool.as_ref() {
        presence_session_close(pool, &meta.sender_session, ledger)
            .await
            .map_err(|error| {
                format!("Presence identity close failed; reopening blocked: {error}")
            })?;
    }
    runtime
        .presence
        .close(
            &meta.sender_session,
            &format!("context-identity-close:{generation}"),
            close,
        )
        .map_err(|error| error.to_string())?;
    runtime
        .presence
        .open_carrying(
            authentication,
            &format!("context-identity-open:{generation}"),
            open,
            Some(ledger.clone()),
        )
        .map_err(|error| error.to_string())?;
    let outcome = persist_presence(
        state,
        runtime,
        &meta.sender_session,
        &authentication.binding,
        "identity_reopen",
    )
    .await;
    presence_point(state, "presence_identity_reopen", outcome, None);
    Ok(())
}

fn fingerprint(root: &Value) -> Result<String, String> {
    body_hash(
        &json!({"room":root["room"],"agentName":root["agentName"],"embodiedSpirit":root["embodiedSpirit"],"operator":root["operator"]}),
    )
}

pub(super) async fn fence<'a>(
    state: &'a AppState,
    saved: &SavedSession,
) -> Result<tokio::sync::MutexGuard<'a, RuntimeState>, String> {
    let runtime = state.runtime.lock().await;
    let root = crate::room_state::execute(
        &state.room_store,
        &state.config.room_dir,
        &state.config.spirit,
        crate::room_state::RoomStateRequest::Read,
    )?;
    if saved.identity_epoch.as_deref() != Some(fingerprint(&root)?.as_str()) {
        return Err(
            "identity epoch changed before context publication; retry with its current binding"
                .into(),
        );
    }
    Ok(runtime)
}

fn cached_presence_failure(
    runtime: &RuntimeState,
    saved: &SavedSession,
    turn: &str,
    binding: &PresenceBinding,
) -> Option<&'static str> {
    let cached = saved
        .turns
        .iter()
        .find(|entry| entry.turn_id == turn)?
        .blocks
        .iter()
        .find(|block| block.kind == BlockKind::PresenceContext)?;
    let Some((frame, _)) = runtime.presence.session_state(&binding.session) else {
        return Some("presence-frame-unavailable");
    };
    if frame.binding != *binding || text(&cached.details["frameId"]) != frame.frame_id {
        return Some("presence-frame-unavailable");
    }
    if !runtime
        .presence
        .has_current_contract(&binding.session, text(&cached.details["contractId"]))
    {
        return Some("presence-contract-unavailable");
    }
    None
}

pub(super) async fn replay_eligible(
    state: &AppState,
    meta: &CommandMeta,
    saved: &SavedSession,
    turn: &str,
) -> Result<bool, String> {
    let mut runtime = state.runtime.lock().await;
    let authentication = authenticate_presence(state, meta)?;
    let root = crate::room_state::execute(
        &state.room_store,
        &state.config.room_dir,
        &state.config.spirit,
        crate::room_state::RoomStateRequest::Read,
    )?;
    if saved.identity_epoch.as_deref() != Some(fingerprint(&root)?.as_str())
        || !saved.turns.iter().any(|entry| entry.turn_id == turn)
    {
        return Ok(false);
    }
    adopt_presence_from_store(state, &mut runtime, &meta.sender_session)
        .await
        .map_err(|error| error.to_string())?;
    let frame_matches = runtime
        .presence
        .session_state(&meta.sender_session)
        .is_none_or(|(frame, _)| frame.binding == authentication.binding);
    Ok(frame_matches
        && cached_presence_failure(&runtime, saved, turn, &authentication.binding).is_none())
}
