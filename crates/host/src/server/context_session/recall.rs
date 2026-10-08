use super::*;

pub(super) struct Evaluation {
    pub decision: Value,
    pub route: Value,
    pub policy: Value,
}

pub(super) async fn analysis(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    recognized_entities: Vec<String>,
) -> Result<hearth::context::ContextAnalysis, String> {
    let identity = state.room_store.identity(&state.config.spirit)?;
    let room_state = crate::room_state::execute(
        &state.room_store,
        &state.config.room_dir,
        &state.config.spirit,
        crate::room_state::RoomStateRequest::Read,
    )?;
    let input = hearth::context::ContextAnalysisRequest {
        prompt: request.prompt.clone(),
        recognized_entities,
        context_characters: request.context_characters,
        active_spirit: identity.spirit,
        operator: identity.operator,
        routing_mode_enabled: room_state["routingMode"]["enabled"] == true,
    };
    let payload = serde_json::to_value(&input).map_err(|error| error.to_string())?;
    let child = child_meta(meta, &request.operation_id, "context", "context");
    let answer = command(
        state,
        ClientCommand::AnalyzeContext {
            meta: child,
            request: input,
        },
        payload,
    )
    .await?;
    serde_json::from_value(answer["analysis"].clone()).map_err(|error| error.to_string())
}

pub(super) async fn evaluate(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    saved: &mut SavedSession,
    mut analysis: hearth::context::ContextAnalysis,
) -> Result<Evaluation, String> {
    let snapshot = command(
        state,
        ClientCommand::Subscribe {
            meta: child_meta(meta, &request.operation_id, "inspect", "recall_policy"),
        },
        Value::Null,
    )
    .await?;
    if snapshot["state"]["requestedMode"] != "quiet" && analysis.route.entity_resolution_suggested {
        if let Ok(result) = organ(
            state,
            meta,
            OrganOperation::EntityResolve,
            json!({"query":request.prompt,"limit":8}),
        )
        .await
        {
            let entities: Vec<String> = array(&result["matches"])
                .iter()
                .filter_map(|entry| entry["canonicalName"].as_str())
                .map(str::to_owned)
                .collect();
            if !entities.is_empty() {
                analysis = self::analysis(state, meta, request, entities).await?;
            }
        }
    }
    let route_value = serde_json::to_value(&analysis.route).map_err(|error| error.to_string())?;
    let route = analysis.route;
    let facts = protocol::RecallPolicyFacts {
        query_route: protocol::RecallQueryRoute {
            intent: route.intent,
            terms: route.terms,
            required_terms: route.required_terms,
            recognized_entities: route.recognized_entities,
        },
        active_project: request.active_project.clone(),
        conversation_tokens: request.context_characters.div_ceil(4),
        working_set_present: request
            .existing_block_kinds
            .contains(&BlockKind::RecallContext)
            || cache::has_kind(saved, BlockKind::RecallContext),
        tool_evidence: saved.work.observe(
            &request.tool_evidence_epoch,
            request.tool_evidence_revision,
            request.user_turn_ordinal,
        ),
    };
    let hash = serde_json::to_value(&facts).map_err(|error| error.to_string())?;
    let response = command(
        state,
        ClientCommand::Evaluate {
            meta: child_meta(meta, &request.operation_id, "evaluate", "recall_policy"),
            facts,
        },
        hash,
    )
    .await?;
    Ok(Evaluation {
        decision: response["decision"].clone(),
        policy: response["state"].clone(),
        route: route_value,
    })
}

pub(super) async fn retrieve(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    deadline: u64,
    evaluation: &Evaluation,
) -> Result<(Option<Block>, Value, Vec<String>), String> {
    let decision = &evaluation.decision;
    let policy = judge(
        state,
        JudgmentRequest::RecallPolicy {
            grant: RecallGrantKind::Recall,
        },
    )
    .await?;
    let enabled =
        policy["approved"] == true && matches!(text(&policy["mode"]), "active" | "shadow");
    let vault = !state.config.akasha_enabled();
    let mut params = if vault {
        json!({"query":decision["query"]})
    } else {
        json!({"query":decision["query"],"mode":decision["resolvedMode"],"temporal_decay":true})
    };
    if enabled && !vault {
        params["rerank_candidate_top_k"] = json!(64);
    }
    let share = deadline.saturating_sub(COMMIT_RESERVE_MS);
    let share_ms = share.saturating_sub(now_ms());
    let recalled = tokio::time::timeout(
        Duration::from_millis(share_ms),
        organ(
            state,
            meta,
            if vault {
                OrganOperation::VaultRecall
            } else {
                OrganOperation::Recall
            },
            params,
        ),
    )
    .await
    .unwrap_or_else(|_| {
        Err(protocol::ProtocolErrorBody::application(
            "automatic_recall_budget_exhausted",
            "automatic Recall exhausted its budget share",
        )
        .build())
    });
    let mut raw = match recalled {
        Ok(raw) => raw,
        Err(error) => {
            let budget_spent = error.code == "automatic_recall_budget_exhausted";
            let failure = if budget_spent {
                format!("automatic Recall ran out of its {share_ms} ms budget share")
            } else {
                error.message.clone()
            };
            record_point(
                state.insula_binding.as_ref(),
                "host",
                "host",
                "automatic_recall",
                if budget_spent {
                    OutcomeClass::Timeout
                } else {
                    OutcomeClass::Error
                },
                Some(if budget_spent {
                    "recall_budget_exhausted"
                } else {
                    "recall_failed"
                }),
                None,
            );
            command(
                state,
                ClientCommand::FailRefresh {
                    meta: child_meta(meta, &request.operation_id, "failed", "recall_policy"),
                    reason: failure.clone(),
                },
                json!(failure),
            )
            .await?;
            let native_failure = json!({"error":error.message,"code":error.code,"retryable":error.retryable,"details":error.details});
            let diagnostic = telemetry::diagnostic(
                &native_failure,
                request,
                Some(&evaluation.route),
                Some(text(&decision["query"])),
                "request_parse",
                budget_spent.then_some(share_ms),
            );
            telemetry::record(
                state,
                meta,
                request,
                telemetry::Observation {
                    status: if budget_spent {
                        "budget_exhausted"
                    } else {
                        "error"
                    },
                    route: Some(&evaluation.route),
                    viewport: Value::Null,
                    diagnostics: diagnostic,
                    error: Some(&failure),
                    query: Some(text(&decision["query"])),
                },
            )
            .await;
            return Ok((
                None,
                Value::Null,
                vec![if budget_spent {
                    "automatic Recall ran out of budget".into()
                } else {
                    "automatic Recall failed".into()
                }],
            ));
        }
    };
    let baseline = array(&raw["retrievalCandidates"]).to_vec();
    let sidecar = raw
        .get("rerankCandidates")
        .and_then(Value::as_array)
        .cloned();
    let rerank_deadline = judge_deadline(deadline);
    let fallback = |status: &str, reason: &str| json!({"retrievalCandidates":baseline,"receipt":{"schemaVersion":"jev-recall-receipt.v1","status":status,"reason":reason,"fallbackUsed":true}});
    let reranked = if enabled && sidecar.is_none() {
        fallback("failed", "pool-unavailable")
    } else if enabled && rerank_deadline.is_none() {
        fallback("refused", "deadline")
    } else {
        let exact_candidate_indexes = if sidecar.is_none() {
            baseline
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate["source"] == "exact_id")
                .map(|(index, _)| index)
                .collect()
        } else {
            Vec::new()
        };
        judge(
            state,
            JudgmentRequest::RecallRerank {
                grant: RecallGrantKind::Recall,
                query: text(&decision["query"]).into(),
                retrieval_candidates: baseline.clone(),
                rerank_candidates: sidecar,
                credential: request.credential.clone(),
                deadline: Some(rerank_deadline.unwrap_or(share)),
                exact_candidate_indexes,
            },
        )
        .await
        .unwrap_or_else(|_| fallback("failed", "rerank-failed"))
    };
    let receipt = reranked["receipt"].clone();
    raw = prepare_viewport(
        raw,
        &reranked,
        policy["mode"] == "active" && receipt["status"] == "active",
    );
    let input = serde_json::from_value(raw.clone())
        .map_err(|error| format!("invalid Recall viewport input: {error}"))?;
    let viewport = command(
        state,
        ClientCommand::ApplyRecallViewport {
            meta: child_meta(meta, &request.operation_id, "viewport", "context"),
            result: input,
            mode: protocol::RecallViewportMode::Automatic,
        },
        raw,
    )
    .await?;
    let presentation = &viewport["result"]["presentation"];
    let warnings = array(&presentation["warnings"]);
    let found = presentation["found"] == true || !warnings.is_empty();
    let entries = array(&presentation["retrievalCandidates"]).len()
        + array(&presentation["canonMatches"]).len()
        + array(&presentation["dateMatches"]).len();
    let refresh = protocol::RecallRefreshCompletion {
        query_terms: array(&decision["queryTerms"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        refresh_reason: text(&decision["refreshReason"]).into(),
        entries: entries as u64,
        has_working_set: found,
        warning: warnings
            .iter()
            .filter_map(Value::as_str)
            .find(|warning| !warning.starts_with("semantic lane empty"))
            .map(str::to_owned),
    };
    let hash = serde_json::to_value(&refresh).map_err(|error| error.to_string())?;
    let completed = command(
        state,
        ClientCommand::CompleteRefresh {
            meta: child_meta(meta, &request.operation_id, "complete", "recall_policy"),
            refresh,
        },
        hash,
    )
    .await?;
    let block = found.then(|| Block {
        kind: BlockKind::RecallContext,
        content: format!("<athanor-memories>\nRoom-local Athanor Recall working set ({}; {}).\nThis working set supersedes every earlier Athanor Recall working set in this conversation; use this copy as current.\n{}\n</athanor-memories>", text(&decision["resolvedMode"]), text(&decision["refreshReason"]), serde_json::to_string_pretty(presentation).expect("Recall presentation serializes")),
        details: json!({"found":presentation["found"],"warnings":warnings,"mode":decision["resolvedMode"],"refreshReason":decision["refreshReason"],"viewport":viewport["result"]["diagnostics"],"jev":receipt}), timestamp: now_ms(),
    });
    let mut notices = Vec::new();
    if policy["mode"] != "off" && !matches!(text(&receipt["status"]), "active" | "shadow") {
        notices.push(format!(
            "automatic Recall Jev baseline: {}",
            text(&receipt["reason"])
        ));
    }
    if let Some(warning) = warnings.first() {
        notices.push(format!("automatic Recall warning: {}", text(warning)));
    }
    let mut diagnostics = viewport["result"]["diagnostics"].clone();
    diagnostics["jev"] = receipt;
    diagnostics["policy"] = json!({"requestedMode":completed["state"]["requestedMode"],"resolvedMode":completed["state"]["resolvedMode"],"refreshReason":decision["refreshReason"]});
    telemetry::record(
        state,
        meta,
        request,
        telemetry::Observation {
            status: if presentation["found"] == true {
                "injected"
            } else {
                "empty"
            },
            route: Some(&evaluation.route),
            viewport: presentation.clone(),
            diagnostics,
            error: None,
            query: Some(text(&decision["query"])),
        },
    )
    .await;
    Ok((block, presentation.clone(), notices))
}

pub(super) fn prepare_viewport(mut raw: Value, reranked: &Value, active: bool) -> Value {
    raw["retrievalCandidates"] = reranked["retrievalCandidates"].clone();
    if active {
        raw["semanticChunks"] = json!([]);
        raw["contentChunks"] = json!([]);
        raw["found"] = json!(
            !array(&raw["retrievalCandidates"]).is_empty()
                || !array(&raw["canonMatches"]).is_empty()
                || !array(&raw["dateMatches"]).is_empty()
        );
    }
    if let Some(object) = raw.as_object_mut() {
        object.remove("rerankCandidates");
    }
    raw
}
