use super::*;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct PreviousRequest {
    pub trace_id: String,
    pub span_id: String,
    pub provider_request_id: Option<String>,
}

pub(super) async fn judgment(
    state: &AppState,
    meta: &CommandMeta,
    parent: Option<&PreviousRequest>,
    mode: bool,
    result: &Value,
) {
    let status = text(&result["status"]);
    if status == "disabled" {
        return;
    }
    let Some(pool) = state.hallway_pool.as_ref() else {
        return;
    };
    let provider = result["provider"].as_str().unwrap_or("none");
    let outcome = match status {
        "scored" => "ok",
        "refused" => "refused",
        "unavailable" => "degraded",
        _ => "error",
    };
    let prefix = if mode {
        "mode_request"
    } else {
        "verdict_request"
    };
    let mut operations = vec![format!("{prefix}.{provider}")];
    if status == "scored" {
        if mode {
            operations.push(format!("recall_mode.{}", text(&result["mode"])));
        } else {
            operations.push(format!("turn_verdict.{}", text(&result["turn"])));
        }
        if !result["recall"].is_null() {
            operations.push(format!("recall_fit.{}", text(&result["recall"]["fit"])));
            operations.push(format!("recall_use.{}", text(&result["recall"]["use"])));
        }
    }
    let writer = new_id();
    let trace = parent
        .map(|parent| parent.trace_id.clone())
        .unwrap_or_else(new_id);
    let events: Vec<Value> = operations.into_iter().enumerate().map(|(index, operation)| json!({
        "eventId":new_id(),"spanId":new_id(),"traceId":trace,"parentSpanId":parent.map(|parent| &parent.span_id),
        "writerId":writer,"writerSequence":index+1,"component":"host","layer":"host","operation":operation,
        "phase":"point","observedAt":timestamp(),"durationUs":if index==0 {result["latencyMs"].as_u64().map(|value|value*1000)}else{None},
        "outcomeClass":outcome,"errorClass":if status=="scored" {Value::Null}else{result["reason"].clone()},
        "bytesOut":if index==0 && status=="scored" {result["packetBytes"].as_u64().unwrap_or_default()}else{0},
        "toolCallId":null,"providerRequestId":parent.and_then(|parent|parent.provider_request_id.as_deref()),
        "idempotencyScope":"trace_span","receiptKind":null,"receiptId":null
    })).collect();
    let Ok(batch) = serde_json::from_value::<akasha::IngestBatch>(json!({"events":events})) else {
        return;
    };
    let binding = akasha::TrustedBinding {
        house_id: state.config.house_id.clone(),
        room: meta.sender_room.clone(),
        spirit: meta.sender_spirit.clone(),
        session_id: meta.sender_session.clone(),
    };
    if !matches!(
        tokio::time::timeout(
            Duration::from_millis(500),
            akasha::ingest_batch(pool, &binding, batch)
        )
        .await,
        Ok(Ok(_))
    ) {
        record_point(
            state.insula_binding.as_ref(),
            "host",
            "host",
            "context_judgment_observation",
            OutcomeClass::Degraded,
            Some("observation_write"),
            None,
        );
    }
}

pub(super) fn receipt(source: &Value) -> Value {
    let schema = source["schemaVersion"]
        .as_str()
        .unwrap_or("jev-recall-receipt.v1");
    let status = source["status"].as_str().unwrap_or("baseline");
    let mut receipt = json!({"schemaVersion":clip(schema,80),"status":clip(status,40)});
    let reason = text(&source["reason"])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !reason.is_empty() {
        receipt["reason"] = json!(clip(&reason, 160));
    }
    for key in ["provider", "model", "policyRevision"] {
        let value = text(&source[key]).trim();
        if !value.is_empty() {
            receipt[key] = json!(clip(value, 120));
        }
    }
    for key in ["latencyMs", "scored", "selected"] {
        if let Some(value) = source[key].as_u64() {
            receipt[key] = json!(value);
        }
    }
    if let Some(value) = source["fallbackUsed"].as_bool() {
        receipt["fallbackUsed"] = json!(value);
    }
    receipt
}
