use super::*;
use regex::Regex;
use std::io::Write;
use std::sync::LazyLock;

static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?-u:\b)Bearer\s+\S+").expect("fixed bearer pattern"));
static AUTHENTICATED_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)([a-z][a-z0-9+.-]*)://[^/\s:@]+(?::[^@\s]*)?@")
        .expect("fixed URL pattern")
});
static SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(password|secret|token|api[_-]?key|authorization)\s*[=:]\s*\S+")
        .expect("fixed secret pattern")
});
static SENSITIVE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(authorization|cookie|password|secret|token|api[_-]?key|prompt|query|payload|body|stdin|url)").expect("fixed diagnostic key pattern")
});

pub(super) struct Observation<'a> {
    pub status: &'static str,
    pub route: Option<&'a Value>,
    pub viewport: Value,
    pub diagnostics: Value,
    pub error: Option<&'a str>,
    pub query: Option<&'a str>,
}

#[derive(Serialize)]
struct Entry<'a> {
    schema_version: u8,
    captured_at: String,
    session_id: &'a str,
    room: &'a str,
    prompt_sha256: String,
    prompt_chars: usize,
    status: &'static str,
    route: Value,
    viewport_diagnostics: Value,
    viewport: Value,
    error: Option<String>,
}

pub(super) fn enabled(room_dir: &std::path::Path, override_value: Option<&str>) -> bool {
    if let Some(value) = override_value {
        return ["1", "true", "yes", "on"]
            .iter()
            .any(|enabled| value.trim().eq_ignore_ascii_case(enabled));
    }
    std::fs::read(room_dir.join(".athanor-room.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|marker| marker["recallTelemetry"] == true)
}

pub(super) fn redact_text(value: &str, private: &[&str], limit: usize) -> String {
    let mut redacted = value.to_owned();
    for value in private.iter().filter(|value| !value.is_empty()) {
        redacted = redacted.replace(*value, "[REDACTED]");
    }
    let redacted = BEARER.replace_all(&redacted, "Bearer [REDACTED]");
    let redacted = AUTHENTICATED_URL.replace_all(&redacted, "$1://[REDACTED]@");
    let redacted = SECRET.replace_all(&redacted, "$1=[REDACTED]");
    clip(&redacted, limit)
}

pub(super) fn redact_value(value: Value, private: &[&str], depth: usize) -> Value {
    if depth >= 6 {
        return json!("[TRUNCATED]");
    }
    match value {
        Value::String(value) => json!(redact_text(&value, private, 2_000)),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .take(24)
                .map(|value| redact_value(value, private, depth + 1))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let value = if SENSITIVE_KEY.is_match(&key) {
                        json!("[REDACTED]")
                    } else {
                        redact_value(value, private, depth + 1)
                    };
                    (key, value)
                })
                .collect(),
        ),
        value => value,
    }
}

fn route_summary(route: Option<&Value>) -> Value {
    let Some(route) = route else {
        return Value::Null;
    };
    json!({"intent":route["intent"],"should_auto_recall":route["shouldAutoRecall"]==true,"lanes":route["lanes"],
        "reasons":array(&route["reasons"]),"term_count":array(&route["terms"]).len(),
        "required_term_count":array(&route["requiredTerms"]).len(),"date_count":array(&route["dateTokens"]).len(),
        "recognized_entity_count":array(&route["recognizedEntities"]).len()})
}

pub(super) async fn record(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    observation: Observation<'_>,
) -> bool {
    if !enabled(
        &state.config.room_dir,
        request.recall_telemetry_override.as_deref(),
    ) {
        return false;
    }
    let private = [
        &*request.prompt,
        observation.query.unwrap_or_default(),
        observation
            .route
            .map(|route| text(&route["recallQuery"]))
            .unwrap_or_default(),
    ];
    let entry = Entry {
        schema_version: 1,
        captured_at: timestamp(),
        session_id: &meta.sender_session,
        room: &state.config.room,
        prompt_sha256: format!("{:x}", Sha256::digest(request.prompt.as_bytes())),
        prompt_chars: request.prompt.encode_utf16().count(),
        status: observation.status,
        route: route_summary(observation.route),
        viewport: observation.viewport,
        viewport_diagnostics: observation.diagnostics,
        error: observation
            .error
            .map(|error| redact_text(error, &private, 500))
            .filter(|error| !error.is_empty()),
    };
    let Ok(mut line) = serde_json::to_vec(&entry) else {
        return false;
    };
    line.push(b'\n');
    let target = state
        .config
        .room_dir
        .join(".omp")
        .join("runtime")
        .join("recall-turns.jsonl");
    let lock = state.context_sessions.lock().await.telemetry_lock.clone();
    // The blocking write owns the lock even if its waiting context is cancelled.
    let written = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let _guard = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(target)?
            .write_all(&line)
    })
    .await;
    if matches!(written, Ok(Ok(()))) {
        return true;
    }
    record_point(
        state.insula_binding.as_ref(),
        "host",
        "host",
        "recall_telemetry",
        OutcomeClass::Degraded,
        Some("append_failed"),
        None,
    );
    false
}

pub(super) fn diagnostic(
    failure: &Value,
    request: &ContextPrepareRequest,
    route: Option<&Value>,
    query: Option<&str>,
    stage: &str,
    budget: Option<u64>,
) -> Value {
    let private = [
        &*request.prompt,
        query.unwrap_or_default(),
        route
            .map(|route| text(&route["recallQuery"]))
            .unwrap_or_default(),
    ];
    let inherited = failure
        .get("details")
        .filter(|details| details.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut diagnostic = redact_value(inherited, &private, 0);
    diagnostic["code"] = failure
        .get("code")
        .cloned()
        .unwrap_or_else(|| json!("AUTO_CONTEXT_AUTOMATIC_RECALL_FAILED"));
    diagnostic["category"] = diagnostic
        .get("category")
        .cloned()
        .unwrap_or_else(|| json!("operation"));
    diagnostic["stage"] = diagnostic
        .get("stage")
        .cloned()
        .unwrap_or_else(|| json!(stage));
    diagnostic["operation"] = json!("automatic_recall");
    diagnostic["owner"] = json!({"component":"host","path":"crates/host/src/server/context_session/recall.rs","symbol":"retrieve"});
    diagnostic["expected"] =
        json!({"hidden_context":true,"display":false,"outcome":"injected_or_fail_open"});
    diagnostic["observed"] = json!({"outcome":"failed_open","route_intent":route.map(|route|&route["intent"]),"route_should_auto_recall":route.is_some_and(|route|route["shouldAutoRecall"]==true)});
    let mut evidence = array(&diagnostic["evidence"]).to_vec();
    evidence.push(json!({"kind":"automatic_context_failure","cause":redact_value(failure.clone(),&private,0)}));
    diagnostic["evidence"] = json!(evidence);
    diagnostic["targets"] = json!([
        "context_session::assemble",
        "context_session::recall::retrieve"
    ]);
    let execution = diagnostic.get("execution").cloned().unwrap_or(Value::Null);
    let write_outcome = text(&execution["write_outcome"]);
    let write_outcome =
        if ["not_started", "rolled_back", "committed", "unknown"].contains(&write_outcome) {
            write_outcome
        } else {
            "not_started"
        };
    let retry = text(&execution["retry"]);
    let retry = if ["safe_now", "after_change", "reconcile_first", "never"].contains(&retry) {
        retry
    } else if failure["retryable"] == true {
        "safe_now"
    } else {
        "after_change"
    };
    diagnostic["execution"] = json!({
        "request_dispatched":execution["request_dispatched"].as_bool().unwrap_or(stage!="configuration_load"),
        "write_outcome":write_outcome,"retry":retry
    });
    diagnostic["next_checks"] = json!([{"action":"inspect","target":"context_session::recall::retrieve"},{"action":"retry","condition":retry}]);
    if let Some(share) = budget {
        diagnostic["code"] = json!("AUTOMATIC_RECALL_BUDGET_EXHAUSTED");
        diagnostic["category"] = json!("budget");
        diagnostic["stage"] = json!("budget");
        diagnostic["budget"] = json!({"share_ms":share,"context_budget_ms":CONTEXT_BUDGET_MS});
    }
    diagnostic
}

pub(super) async fn record_failure(
    state: &AppState,
    meta: &CommandMeta,
    request: &ContextPrepareRequest,
    route: Option<&Value>,
    query: Option<&str>,
    error: &str,
    stage: &str,
) {
    let failure = json!({"error":error});
    let diagnostics = diagnostic(&failure, request, route, query, stage, None);
    record(
        state,
        meta,
        request,
        Observation {
            status: "error",
            route,
            viewport: Value::Null,
            diagnostics,
            error: Some(error),
            query,
        },
    )
    .await;
}
