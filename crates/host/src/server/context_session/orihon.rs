use super::*;

pub(super) async fn prepare(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    mode: &str,
    query: Option<&str>,
    deadline: u64,
) -> (Option<String>, Vec<String>) {
    if !request.capabilities.top_level {
        return (None, Vec::new());
    }
    let mut warnings = Vec::new();
    let mut navigation = None;

    if mode == "work" {
        if let Some(query) = query {
            match navigate(state, meta, request, query, deadline).await {
                Ok(value) => navigation = value,
                Err(error) => warnings.push(format!("Orihon navigation unavailable: {error}")),
            }
        }
    }

    (navigation, warnings)
}

async fn bounded_organ(
    state: &AppState,
    meta: &CommandMeta,
    operation: OrganOperation,
    params: Value,
    deadline: u64,
) -> Result<Value, String> {
    let remaining = deadline
        .saturating_sub(COMMIT_RESERVE_MS)
        .saturating_sub(now_ms());

    tokio::time::timeout(
        Duration::from_millis(remaining),
        organ(state, meta, operation, params),
    )
    .await
    .map_err(|_| "context deadline reached".to_owned())?
    .map_err(|error| error.message)
}

async fn navigate(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    query: &str,
    deadline: u64,
) -> Result<Option<String>, String> {
    let value = bounded_organ(
        state,
        meta,
        OrganOperation::Orihon,
        json!({"query":query,"credential":request.credential,"excludeSession":meta.sender_session,
            "deadline":deadline.saturating_sub(COMMIT_RESERVE_MS)}),
        deadline,
    )
    .await?;

    if value["found"] != true {
        return Ok(None);
    }
    let text = value.to_string();

    if text.len() > 16000 {
        return Err("whole candidates exceed the context budget; use orihon(query)".into());
    }

    Ok(Some(format!(
        "\n<athanor-orihon>\nConversation provenance only. Local-context candidates require reading the selected page. An abstention is not a match.\n{text}\n</athanor-orihon>"
    )))
}
