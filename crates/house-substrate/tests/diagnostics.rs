use athanor_substrate::{AppError, RecallParams};
use serde_json::Value;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn missing_dotenv_path(case: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("solarisael-substrate-{case}-{nonce}.env"))
}

fn jsonl_with_environment(
    environment: &[(&str, &str)],
    requests: &[&str],
) -> (Vec<Value>, PathBuf) {
    let dotenv_path = missing_dotenv_path("diagnostics");
    let mut command = Command::new(env!("CARGO_BIN_EXE_athanor-substrate"));
    command
        .env_clear()
        .env("SOLARISAEL_SUBSTRATE_DOTENV_PATH", &dotenv_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in environment {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("diagnostic binary must start");
    let mut stdin = child.stdin.take().expect("child stdin must be piped");
    for request in requests {
        writeln!(stdin, "{request}").expect("request write must succeed");
    }
    drop(stdin);
    let output = child
        .wait_with_output()
        .expect("diagnostic binary must exit after stdin closes");
    assert!(
        output.status.success(),
        "binary stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .expect("JSONL output must be UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each response must be JSON"))
        .collect();
    (responses, dotenv_path)
}

fn error_details(response: &Value) -> &Value {
    assert_eq!(response["protocol"], 1);
    let details = &response["error"]["details"];
    for key in [
        "category",
        "stage",
        "operation",
        "owner",
        "expected",
        "observed",
        "evidence",
        "targets",
        "next_checks",
        "execution",
    ] {
        assert!(details.get(key).is_some(), "missing diagnostic field {key}");
    }
    assert!(details["owner"].get("path").is_some());
    assert!(details["owner"].get("symbol").is_some());
    assert!(
        details["evidence"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        details["targets"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        details["next_checks"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    details
}

#[test]
fn missing_configuration_returns_complete_responses_and_keeps_jsonl_alive() {
    let request = r#"{"protocol":1,"id":"missing-1","method":"recall","params":{"room":"room","query":"needle"}}"#;
    let follow_up = r#"{"protocol":1,"id":"missing-2","method":"recall","params":{"room":"room","query":"needle"}}"#;
    let (responses, dotenv_path) = jsonl_with_environment(&[], &[request, follow_up]);
    assert_eq!(responses.len(), 2);
    for response in responses {
        assert_eq!(response["error"]["code"], "configuration");
        let details = error_details(&response);
        assert_eq!(details["category"], "configuration");
        assert_eq!(details["stage"], "configuration_load");
        assert_eq!(details["operation"], "recall");
        assert_eq!(details["execution"]["request_dispatched"], false);
        assert_eq!(details["execution"]["write_outcome"], "not_started");
        assert_eq!(details["execution"]["retry"], "after_change");
        assert_eq!(details["observed"]["dotenv"]["exists"], false);
        assert_eq!(
            details["observed"]["dotenv"]["target"],
            dotenv_path.to_string_lossy().as_ref()
        );
        assert!(
            details["observed"]["environment_keys"]["missing_keys"]
                .as_array()
                .is_some_and(|keys| keys.iter().any(|key| key == "DATABASE_URL"))
        );
    }
}

#[test]
fn invalid_embedding_dimension_reports_configuration_without_environment_values() {
    let request = r#"{"protocol":1,"id":"dimension","method":"recall","params":{"room":"room","query":"needle"}}"#;
    let (responses, _) = jsonl_with_environment(
        &[
            (
                "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL",
                "postgres://localhost/substrate",
            ),
            ("SOLARISAEL_EMBED_DIMENSION", "1024"),
        ],
        &[request],
    );
    let response = responses.first().expect("one response expected");
    assert_eq!(response["error"]["code"], "configuration");
    let details = error_details(response);
    assert_eq!(details["category"], "configuration");
    assert_eq!(details["stage"], "configuration_load");
    assert_eq!(details["expected"]["SOLARISAEL_EMBED_DIMENSION"], "2048");
    assert_eq!(
        details["observed"]["reason"],
        "embedding_dimension_incompatible"
    );
    let serialized = serde_json::to_string(response).expect("response serializes");
    assert!(!serialized.contains("1024"));
}

#[test]
fn database_and_configuration_failures_keep_their_categories_and_stages() {
    let configuration = AppError::Config(
        "invalid database configuration: postgres://user:supersecret@db/substrate".into(),
    )
    .protocol_error_body("recall");
    let configuration = serde_json::to_value(configuration).expect("error serializes");
    let configuration_response = serde_json::json!({
        "protocol": 1,
        "error": configuration,
    });
    let configuration_details = error_details(&configuration_response);
    assert_eq!(configuration_details["category"], "configuration");
    assert_eq!(configuration_details["stage"], "database_connect");

    let database = AppError::Database(sqlx::Error::PoolTimedOut).protocol_error_body("recall");
    let database = serde_json::to_value(database).expect("error serializes");
    let database_response = serde_json::json!({
        "protocol": 1,
        "error": database,
    });
    let database_details = error_details(&database_response);
    assert_eq!(database_details["category"], "database");
    assert_eq!(database_details["stage"], "database_query");
    assert_eq!(database_details["execution"]["retry"], "safe_now");
}

#[test]
fn validation_failure_has_request_owner_and_safe_execution() {
    let params = RecallParams {
        room: "not a room".into(),
        query: "needle".into(),
        semantic_top_k: 8,
        semantic_min_similarity: 0.5,
        content_top_k: 8,
        content_min_similarity: 0.3,
        temporal_decay: false,
    };
    let error = params
        .validate()
        .expect_err("invalid room must fail validation");
    let body = error.protocol_error_body("recall");
    let body = serde_json::to_value(body).expect("error serializes");
    let response = serde_json::json!({"protocol": 1, "error": body});
    let details = error_details(&response);
    assert_eq!(details["category"], "input");
    assert_eq!(details["stage"], "validation");
    assert_eq!(details["owner"]["path"], "src/recall.rs");
    assert_eq!(details["owner"]["symbol"], "RecallParams::validate");
    assert_eq!(details["execution"]["request_dispatched"], false);
    assert_eq!(details["execution"]["write_outcome"], "not_started");
    assert_eq!(details["execution"]["retry"], "never");
}

#[test]
fn diagnostics_redact_configuration_and_embedding_secrets() {
    let config = AppError::Config(
        "DATABASE_URL=postgres://user:supersecret@db/substrate?password=supersecret".into(),
    )
    .protocol_error_body("recall");
    let embedding = AppError::Embedding(
        "Authorization: Bearer supersecret endpoint=https://token:supersecret@example.test".into(),
    )
    .protocol_error_body("remember");
    let serialized = format!(
        "{}{}",
        serde_json::to_string(&config).expect("config error serializes"),
        serde_json::to_string(&embedding).expect("embedding error serializes"),
    );
    assert!(!serialized.contains("supersecret"));
    assert!(!serialized.contains("token:supersecret"));
}
