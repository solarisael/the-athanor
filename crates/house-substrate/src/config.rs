use house_protocol::{
    DiagnosticCategory, DiagnosticDetails, DiagnosticEvidence, DiagnosticExecution,
    DiagnosticNextCheck, DiagnosticOwner, DiagnosticRetry, DiagnosticStage, DiagnosticTarget,
    DiagnosticTargetKind, DiagnosticWriteOutcome, ProtocolErrorBody,
};
use regex::Regex;
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{
    collections::BTreeSet,
    env, fs, io,
    path::{Path, PathBuf},
    str::FromStr,
    sync::LazyLock,
    time::Duration,
};
use thiserror::Error;

const DEFAULT_EMBED_URL: &str = "http://127.0.0.1:11434/api/embed";
const DEFAULT_EMBED_MODEL: &str = "hf.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF:latest";
pub(crate) const EMBED_DIMENSION: usize = 2048;
pub(crate) static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(Client::new);
pub(crate) static ROOM_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").expect("room key regex must compile")
});
pub(crate) static PATH_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(20\d{2})[-_](\d{2})[-_](\d{2})").expect("path date regex must compile")
});
pub(crate) static STITCHED_PATH_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(20\d{2})[-_](\d{2})[-_](\d{2})[_-](\d{2})")
        .expect("stitched path date regex must compile")
});
pub(crate) static QUERY_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(20\d{2})-(\d{2})-(\d{2})\b").expect("query date regex must compile")
});

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database connection error: {0}")]
    DatabaseConnect(sqlx::Error),
    #[error("database schema query error: {0}")]
    DatabaseSchema(sqlx::Error),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub embed_url: Option<String>,
    pub embed_model: String,
    pub embed_dimension: usize,
    pub embed_required: bool,
    pub test_embedding_disabled: bool,
    pub giga_source_ledger_dir: Option<PathBuf>,
    pub giga_source_room: Option<String>,
}

const DOTENV_PATH_OVERRIDE: &str = "SOLARISAEL_SUBSTRATE_DOTENV_PATH";
const DATABASE_ENV_KEYS: &[&str] = &[
    "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL",
    "DATABASE_URL",
    "PGHOST",
    "PGPORT",
    "PGUSER",
    "PGPASSWORD",
    "PGDATABASE",
];
const EMBEDDING_ENV_KEYS: &[&str] = &[
    "SOLARISAEL_EMBED_URL",
    "SOLARISAEL_EMBED_MODEL",
    "SOLARISAEL_EMBED_DIMENSION",
    "SOLARISAEL_DISABLE_EMBEDDING",
    "SOLARISAEL_TEST_DISABLE_EMBEDDING",
];
const GIGA_SOURCE_ENV_KEYS: &[&str] = &[
    "SOLARISAEL_GIGA_SOURCE_LEDGER_DIR",
    "SOLARISAEL_GIGA_SOURCE_ROOM",
];

struct Dotenv {
    /// `None` when the state root could not be resolved, so there is no
    /// dotenv location to speak of. Distinct from "a path that holds no file".
    path: Option<PathBuf>,
    text: Option<String>,
}

impl Dotenv {
    fn load(path: Option<PathBuf>) -> Self {
        Self {
            text: path
                .as_deref()
                .and_then(|path| fs::read_to_string(path).ok()),
            path,
        }
    }

    fn value(&self, key: &str) -> Option<String> {
        self.text.as_deref()?.lines().find_map(|line| {
            let line = line.trim();
            let (found, value) = line.split_once('=')?;
            (found.trim() == key).then(|| {
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string()
            })
        })
    }

    fn has_key(&self, key: &str) -> bool {
        self.value(key).is_some()
    }

    fn display_path(&self) -> String {
        self.path
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("<unresolved: {} is not set>", crate::state::STATE_DIR))
    }
}

/// The dotenv this process reads: the explicit override, then an
/// executable-adjacent `.env` for portable bundles, then the mutable state
/// directory. Never inside the immutable product tree.
///
/// `None` means no dotenv location could be established at all — the state
/// root is unresolved. That is reported as absent configuration rather than
/// papered over with a guessed path.
fn dotenv_target() -> Option<PathBuf> {
    if let Some(path) = env::var_os(DOTENV_PATH_OVERRIDE).filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if let Some(candidate) = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(".env")))
        .filter(|path| path.is_file())
    {
        return Some(candidate);
    }
    crate::state::substrate_state_dir()
        .ok()
        .map(|dir| dir.join(".env"))
}

fn configured_value(key: &str, dotenv: &Dotenv) -> Option<String> {
    env::var(key).ok().or_else(|| dotenv.value(key))
}

fn configuration_observed(dotenv: &Dotenv, reason: &str) -> Value {
    let keys = DATABASE_ENV_KEYS
        .iter()
        .chain(EMBEDDING_ENV_KEYS)
        .chain(GIGA_SOURCE_ENV_KEYS)
        .copied()
        .collect::<BTreeSet<_>>();
    let dotenv_present_keys = keys
        .iter()
        .filter(|&&key| dotenv.has_key(key))
        .copied()
        .collect::<Vec<_>>();
    let effective_present_keys = keys
        .iter()
        .filter(|&&key| configured_value(key, dotenv).is_some())
        .copied()
        .collect::<Vec<_>>();
    let missing_keys = keys
        .iter()
        .filter(|&&key| configured_value(key, dotenv).is_none())
        .copied()
        .collect::<Vec<_>>();
    json!({
        "reason": reason,
        "dotenv": {
            "target": dotenv.display_path(),
            "exists": dotenv.text.is_some(),
            "present_keys": dotenv_present_keys,
        },
        "environment_keys": {
            "present_keys": effective_present_keys,
            "missing_keys": missing_keys,
        },
    })
}

fn is_write_operation(operation: &str) -> bool {
    matches!(
        operation,
        "canon_write"
            | "remember"
            | "paper_boat_sleep"
            | "anamnesis_write"
            | "cluster_maintenance"
            | "giga_event_ingest"
            | "giga_conversation_ingest"
            | "giga_event_claim"
            | "giga_event_finish"
            | "giga_event_replay"
            | "giga_queue_maintenance"
            | "giga_promote"
            | "giga_tool_promote"
            | "giga_review"
            | "giga_tool_review"
    )
}

fn validation_owner(operation: &str) -> (&'static str, &'static str) {
    match operation {
        "canon_write" => ("src/canon.rs", "canon_write"),
        "canon_read" => ("src/canon.rs", "canon_read"),
        "remember" => ("src/remember.rs", "RememberRequest::validate"),
        "paper_boat_sleep" => ("src/paper_boat.rs", "PaperBoatSleepRequest::new"),
        "paper_boat_wake" => ("src/paper_boat.rs", "PaperBoatWakeRequest::new"),
        "recall" => ("src/recall.rs", "RecallParams::validate"),
        "anamnesis" => ("src/anamnesis.rs", "AnamnesisParams::validate"),
        "anamnesis_write" => ("src/anamnesis.rs", "anamnesis_write"),
        "cluster_maintenance" => ("src/cluster.rs", "cluster_maintenance"),
        "giga_event_ingest" => ("src/giga.rs", "giga_event_ingest"),
        "giga_conversation_ingest" => ("src/giga.rs", "giga_conversation_ingest"),
        "giga_event_claim" => ("src/giga.rs", "giga_event_claim"),
        "giga_event_finish" => ("src/giga.rs", "giga_event_finish"),
        "giga_event_replay" => ("src/giga.rs", "giga_event_replay"),
        "giga_queue_maintenance" => ("src/giga.rs", "giga_queue_maintenance"),
        "giga_promote" => ("src/giga.rs", "giga_promote"),
        "giga_tool_promote" => ("src/giga.rs", "giga_tool_promote"),
        "giga_candidate_list" => ("src/giga.rs", "giga_candidate_list"),
        "giga_review" => ("src/giga.rs", "giga_review"),
        "giga_tool_review" => ("src/giga.rs", "giga_tool_review"),
        "giga_health" => ("src/giga.rs", "giga_health"),
        _ => ("src/main.rs", "decode_line"),
    }
}

fn validation_target(message: &str) -> &'static str {
    if message.contains("room") {
        "params.room"
    } else if message.contains("query") {
        "params.query"
    } else if message.contains("semantic_top_k") {
        "params.semanticTopK"
    } else if message.contains("content_top_k") {
        "params.contentTopK"
    } else if message.contains("title") {
        "params.title"
    } else if message.contains("body") || message.contains("lesson") {
        "params.body"
    } else if message.contains("embedding") {
        "params.embedding"
    } else {
        "params"
    }
}

fn database_owner(operation: &str) -> (&'static str, &'static str) {
    match operation {
        "remember" => ("src/remember.rs", "remember"),
        "paper_boat_sleep" => ("src/paper_boat.rs", "paper_boat_sleep"),
        "paper_boat_wake" => ("src/paper_boat.rs", "paper_boat_wake"),
        "recall" => ("src/recall.rs", "recall"),
        "anamnesis" => ("src/anamnesis.rs", "anamnesis"),
        "anamnesis_write" => ("src/anamnesis.rs", "anamnesis_write"),
        "cluster_maintenance" => ("src/cluster.rs", "cluster_maintenance"),
        "giga_event_ingest" => ("src/giga.rs", "giga_event_ingest"),
        "giga_conversation_ingest" => ("src/giga.rs", "giga_conversation_ingest"),
        "giga_event_claim" => ("src/giga.rs", "giga_event_claim"),
        "giga_event_finish" => ("src/giga.rs", "giga_event_finish"),
        "giga_event_replay" => ("src/giga.rs", "giga_event_replay"),
        "giga_queue_maintenance" => ("src/giga.rs", "giga_queue_maintenance"),
        "giga_promote" => ("src/giga.rs", "giga_promote"),
        "giga_tool_promote" => ("src/giga.rs", "giga_tool_promote"),
        "giga_candidate_list" => ("src/giga.rs", "giga_candidate_list"),
        "giga_review" => ("src/giga.rs", "giga_review"),
        "giga_tool_review" => ("src/giga.rs", "giga_tool_review"),
        "giga_health" => ("src/giga.rs", "giga_health"),
        _ => ("src/config.rs", "Config::pool"),
    }
}

fn embedding_owner(operation: &str) -> (&'static str, &'static str) {
    match operation {
        "recall" => ("src/recall.rs", "query_embedding"),
        "anamnesis" | "anamnesis_write" => ("src/anamnesis.rs", "anamnesis_embedding"),
        _ => ("src/remember.rs", "embed"),
    }
}

fn config_reason(message: &str) -> &'static str {
    if message.contains("DATABASE_URL") || message.contains("PG*") {
        "database_environment_missing_or_incomplete"
    } else if message.contains("SOLARISAEL_EMBED_DIMENSION") {
        "embedding_dimension_not_an_integer"
    } else if message.contains("cluster embedding dimension") {
        "cluster_embedding_schema_incompatible"
    } else if message.contains("embedding dimension") {
        "embedding_dimension_incompatible"
    } else if message.contains("invalid database configuration") {
        "database_configuration_invalid"
    } else if message.contains("memory_chunks.body_embedding") {
        "embedding_column_missing"
    } else if message.contains("incompatible embedding schema") {
        "embedding_schema_incompatible"
    } else {
        "configuration_invalid"
    }
}

impl AppError {
    pub fn protocol_error_body(&self, operation: &str) -> ProtocolErrorBody {
        ProtocolErrorBody::application(self.code(), self.safe_message())
            .retryable(self.retryable())
            .diagnostics(self.diagnostics(operation))
            .build()
    }

    pub fn diagnostics(&self, operation: &str) -> DiagnosticDetails {
        let component = "athanor-substrate";
        let write = is_write_operation(operation);
        match self {
            Self::Invalid(message) => {
                let (path, symbol) = validation_owner(operation);
                DiagnosticDetails::new(DiagnosticCategory::Input, DiagnosticStage::Validation)
                    .operation(operation)
                    .owner(DiagnosticOwner::new(component).path(path).symbol(symbol))
                    .expected(json!({"request_parameters": "must satisfy method validation rules"}))
                    .observed(json!({
                        "validation": "failed",
                        "request_field": validation_target(message),
                    }))
                    .evidence(
                        DiagnosticEvidence::new("validation_failure")
                            .summary("Request validation rejected one or more fields")
                            .data(json!({"request_field": validation_target(message)})),
                    )
                    .target(DiagnosticTarget::new(
                        DiagnosticTargetKind::RequestField,
                        validation_target(message),
                    ))
                    .target(DiagnosticTarget::new(DiagnosticTargetKind::Symbol, symbol))
                    .next_check(
                        DiagnosticNextCheck::new("inspect_request_field")
                            .target(DiagnosticTarget::new(
                                DiagnosticTargetKind::RequestField,
                                validation_target(message),
                            ))
                            .expected(json!({"valid": true})),
                    )
                    .next_check(DiagnosticNextCheck::new("retry_request").expected(json!({
                        "after": "request validation succeeds",
                    })))
                    .execution(DiagnosticExecution::new(
                        false,
                        DiagnosticWriteOutcome::NotStarted,
                        DiagnosticRetry::Never,
                    ))
            }
            Self::Config(message) => {
                let dotenv = Dotenv::load(dotenv_target());
                let reason = config_reason(message);
                let (stage, path, symbol) = match reason {
                    "database_configuration_invalid" => (
                        DiagnosticStage::DatabaseConnect,
                        "src/config.rs",
                        "Config::pool",
                    ),
                    "embedding_column_missing" | "embedding_schema_incompatible" => (
                        DiagnosticStage::DatabaseQuery,
                        "src/config.rs",
                        "Config::pool",
                    ),
                    "cluster_embedding_schema_incompatible" => (
                        DiagnosticStage::DatabaseQuery,
                        "src/cluster.rs",
                        "cluster_maintenance",
                    ),
                    _ => (
                        DiagnosticStage::ConfigurationLoad,
                        "src/config.rs",
                        "Config::from_env",
                    ),
                };
                let target = match reason {
                    "embedding_column_missing" | "embedding_schema_incompatible" => {
                        DiagnosticTarget::new(DiagnosticTargetKind::Migration, "0002")
                    }
                    "cluster_embedding_schema_incompatible" => DiagnosticTarget::new(
                        DiagnosticTargetKind::Table,
                        "memory_chunks.body_embedding",
                    ),
                    "database_configuration_invalid" => {
                        DiagnosticTarget::new(DiagnosticTargetKind::RequestField, "DATABASE_URL")
                    }
                    "embedding_dimension_not_an_integer" | "embedding_dimension_incompatible" => {
                        DiagnosticTarget::new(
                            DiagnosticTargetKind::RequestField,
                            "SOLARISAEL_EMBED_DIMENSION",
                        )
                    }
                    _ => DiagnosticTarget::new(DiagnosticTargetKind::File, dotenv.display_path()),
                };
                DiagnosticDetails::new(DiagnosticCategory::Configuration, stage)
                    .operation(operation)
                    .owner(
                        DiagnosticOwner::new(component)
                            .path(path)
                            .symbol(symbol),
                    )
                    .expected(match reason {
                        "database_environment_missing_or_incomplete" => json!({
                            "database_configuration": {
                                "accepted_sources": [
                                    "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL",
                                    "DATABASE_URL",
                                    "complete PGHOST, PGPORT, PGUSER, PGPASSWORD, PGDATABASE",
                                ],
                            },
                        }),
                        "embedding_dimension_not_an_integer" => {
                            json!({"SOLARISAEL_EMBED_DIMENSION": "integer"})
                        }
                        "embedding_dimension_incompatible" => {
                            json!({"SOLARISAEL_EMBED_DIMENSION": "2048"})
                        }
                        "embedding_column_missing"
                        | "embedding_schema_incompatible"
                        | "cluster_embedding_schema_incompatible" => {
                            json!({"memory_chunks.body_embedding": "vector(2048)"})
                        }
                        _ => json!({"database_configuration": "parseable PostgreSQL connection options"}),
                    })
                    .observed(configuration_observed(&dotenv, reason))
                    .evidence(
                        DiagnosticEvidence::new("configuration_source")
                            .summary("Environment key presence was checked without emitting values")
                            .data(configuration_observed(&dotenv, reason)),
                    )
                    .target(DiagnosticTarget::new(DiagnosticTargetKind::File, "src/config.rs"))
                    .target(target.clone())
                    .next_check(
                        DiagnosticNextCheck::new("inspect_configuration_source")
                            .target(target)
                            .expected(json!({"reason_resolved": reason})),
                    )
                    .next_check(
                        DiagnosticNextCheck::new("retry_request").expected(json!({
                            "after": "configuration is corrected",
                        })),
                    )
                    .execution(DiagnosticExecution::new(
                        false,
                        DiagnosticWriteOutcome::NotStarted,
                        DiagnosticRetry::AfterChange,
                    ))
            }
            Self::DatabaseConnect(_) | Self::DatabaseSchema(_) | Self::Database(_) => {
                let connection = matches!(self, Self::DatabaseConnect(_));
                let schema = matches!(self, Self::DatabaseSchema(_));
                let (owner_path, owner_symbol) = if connection || schema {
                    ("src/config.rs", "Config::pool")
                } else {
                    database_owner(operation)
                };
                let stage = if connection {
                    DiagnosticStage::DatabaseConnect
                } else if schema || !write {
                    DiagnosticStage::DatabaseQuery
                } else {
                    DiagnosticStage::Transaction
                };
                let (write_outcome, retry) = if schema {
                    (
                        DiagnosticWriteOutcome::NotStarted,
                        DiagnosticRetry::AfterChange,
                    )
                } else if write && !connection {
                    (
                        DiagnosticWriteOutcome::Unknown,
                        DiagnosticRetry::ReconcileFirst,
                    )
                } else {
                    (DiagnosticWriteOutcome::NotStarted, DiagnosticRetry::SafeNow)
                };
                let target = if schema {
                    DiagnosticTarget::new(DiagnosticTargetKind::Migration, "0002")
                } else {
                    DiagnosticTarget::new(DiagnosticTargetKind::Service, "PostgreSQL")
                };
                let first_check = if schema {
                    DiagnosticNextCheck::new("apply_migration")
                        .target(DiagnosticTarget::new(
                            DiagnosticTargetKind::Migration,
                            "0002",
                        ))
                        .expected(json!({"memory_chunks.body_embedding": "vector(2048)"}))
                } else {
                    DiagnosticNextCheck::new("check_database_connectivity")
                        .target(DiagnosticTarget::new(
                            DiagnosticTargetKind::Service,
                            "PostgreSQL",
                        ))
                        .expected(json!({"reachable": true}))
                };
                DiagnosticDetails::new(DiagnosticCategory::Database, stage)
                    .operation(operation)
                    .owner(
                        DiagnosticOwner::new(component)
                            .path(owner_path)
                            .symbol(owner_symbol),
                    )
                    .expected(if schema {
                        json!({"memory_chunks.body_embedding": "vector(2048)"})
                    } else if connection {
                        json!({"database": "reachable with configured connection options"})
                    } else {
                        json!({"database": "query and transaction complete"})
                    })
                    .observed(json!({
                        "database_error": if schema {
                            "schema_query_failed"
                        } else if connection {
                            "connection_failed"
                        } else {
                            "query_or_transaction_failed"
                        },
                    }))
                    .evidence(
                        DiagnosticEvidence::new("sqlx_error")
                            .summary("PostgreSQL failure details were omitted to protect connection data")
                            .data(json!({
                                "stage": if schema {
                                    "database_query"
                                } else if connection {
                                    "database_connect"
                                } else {
                                    "database_query_or_transaction"
                                },
                            })),
                    )
                    .target(DiagnosticTarget::new(DiagnosticTargetKind::File, owner_path))
                    .target(target.clone())
                    .next_check(first_check)
                    .next_check(
                        DiagnosticNextCheck::new(if write && !connection && !schema {
                            "reconcile_write"
                        } else {
                            "retry_request"
                        })
                        .expected(json!({"safe_retry": !write || connection || schema})),
                    )
                    .execution(DiagnosticExecution::new(
                        !connection && !schema,
                        write_outcome,
                        retry,
                    ))
            }
            Self::Embedding(_) => {
                let (owner_path, owner_symbol) = embedding_owner(operation);
                DiagnosticDetails::new(
                    DiagnosticCategory::Embedding,
                    DiagnosticStage::EmbeddingRequest,
                )
                .operation(operation)
                .owner(
                    DiagnosticOwner::new(component)
                        .path(owner_path)
                        .symbol(owner_symbol),
                )
                .expected(json!({
                    "embedding_response": {
                        "vectors": "present, numeric, and dimension 2048",
                    },
                }))
                .observed(json!({"embedding_response": "request_or_response_validation_failed"}))
                .evidence(
                    DiagnosticEvidence::new("embedding_failure")
                        .summary("Embedding service failure was redacted before serialization")
                        .data(json!({"endpoint_configuration": "SOLARISAEL_EMBED_URL"})),
                )
                .target(DiagnosticTarget::new(
                    DiagnosticTargetKind::Service,
                    "embedding service",
                ))
                .target(DiagnosticTarget::new(
                    DiagnosticTargetKind::RequestField,
                    "SOLARISAEL_EMBED_URL",
                ))
                .next_check(
                    DiagnosticNextCheck::new("check_embedding_service")
                        .target(DiagnosticTarget::new(
                            DiagnosticTargetKind::Service,
                            "embedding service",
                        ))
                        .expected(json!({"available": true, "dimension": 2048})),
                )
                .next_check(
                    DiagnosticNextCheck::new("retry_request")
                        .expected(json!({"after": "embedding service recovers"})),
                )
                .execution(DiagnosticExecution::new(
                    true,
                    DiagnosticWriteOutcome::NotStarted,
                    DiagnosticRetry::SafeNow,
                ))
            }
            Self::Protocol(_) => {
                DiagnosticDetails::new(DiagnosticCategory::Protocol, DiagnosticStage::RequestParse)
                    .operation(operation)
                    .owner(
                        DiagnosticOwner::new(component)
                            .path("src/main.rs")
                            .symbol("decode_line"),
                    )
                    .expected(json!({"request": "valid protocol v1 envelope"}))
                    .observed(json!({"request": "protocol conversion failed"}))
                    .evidence(
                        DiagnosticEvidence::new("protocol_conversion_failure")
                            .summary("Protocol conversion rejected the request"),
                    )
                    .target(DiagnosticTarget::new(
                        DiagnosticTargetKind::Symbol,
                        "decode_line",
                    ))
                    .next_check(
                        DiagnosticNextCheck::new("validate_protocol_envelope")
                            .expected(json!({"protocol": 1})),
                    )
                    .execution(DiagnosticExecution::new(
                        false,
                        DiagnosticWriteOutcome::NotStarted,
                        DiagnosticRetry::Never,
                    ))
            }
            Self::Io(error) => DiagnosticDetails::new(
                DiagnosticCategory::Filesystem,
                DiagnosticStage::RequestWrite,
            )
            .operation(operation)
            .owner(
                DiagnosticOwner::new(component)
                    .path("src/config.rs")
                    .symbol("filesystem operation"),
            )
            .expected(json!({"filesystem_operation": "complete successfully"}))
            .observed(json!({"io_error_kind": error.kind().to_string()}))
            .evidence(
                DiagnosticEvidence::new("io_error")
                    .summary("Filesystem error details were reduced to the error kind")
                    .data(json!({"io_error_kind": error.kind().to_string()})),
            )
            .target(DiagnosticTarget::new(DiagnosticTargetKind::File, ".env"))
            .next_check(
                DiagnosticNextCheck::new("check_filesystem_access")
                    .target(DiagnosticTarget::new(DiagnosticTargetKind::File, ".env"))
                    .expected(json!({"readable": true})),
            )
            .next_check(DiagnosticNextCheck::new("retry_request"))
            .execution(DiagnosticExecution::new(
                true,
                DiagnosticWriteOutcome::NotStarted,
                DiagnosticRetry::SafeNow,
            )),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid_params",
            Self::Config(_) => "configuration",
            Self::Protocol(_) => "protocol",
            Self::Embedding(_) => "embedding",
            Self::Database(_) | Self::DatabaseConnect(_) | Self::DatabaseSchema(_) => "database",
            Self::Io(_) => "io",
        }
    }

    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Embedding(_)
                | Self::Database(_)
                | Self::DatabaseConnect(_)
                | Self::DatabaseSchema(_)
                | Self::Io(_)
        )
    }

    fn safe_message(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid request parameters",
            Self::Config(message) if message.contains("embedding dimension") => {
                "configuration error: embedding dimension is incompatible"
            }
            Self::Config(message) if message.contains("SOLARISAEL_EMBED_DIMENSION") => {
                "configuration error: embedding dimension is invalid"
            }
            Self::Config(_) => "configuration error",
            Self::DatabaseConnect(_) => "database connection failed",
            Self::DatabaseSchema(_) => "database schema query failed",
            Self::Database(_) => "database operation failed",
            Self::Embedding(_) => "embedding request failed",
            Self::Protocol(_) => "protocol request failed",
            Self::Io(_) => "filesystem operation failed",
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        Self::from_dotenv(Dotenv::load(dotenv_target()))
    }

    pub fn from_env_file(path: &Path) -> Result<Self, AppError> {
        Self::from_dotenv(Dotenv::load(Some(path.to_path_buf())))
    }

    fn from_dotenv(dotenv: Dotenv) -> Result<Self, AppError> {
        let database_url = env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
            .ok()
            .or_else(|| configured_value("DATABASE_URL", &dotenv))
            .or_else(|| {
                let host = configured_value("PGHOST", &dotenv)?;
                let port = configured_value("PGPORT", &dotenv)
                    .unwrap_or_else(|| "5432".into())
                    .parse()
                    .ok()?;
                let user = configured_value("PGUSER", &dotenv)?;
                let password = configured_value("PGPASSWORD", &dotenv)?;
                let database = configured_value("PGDATABASE", &dotenv)?;
                let mut url = reqwest::Url::parse("postgres://localhost").ok()?;
                url.set_host(Some(&host)).ok()?;
                url.set_port(Some(port)).ok()?;
                url.set_username(&user).ok()?;
                url.set_password(Some(&password)).ok()?;
                url.set_path(&database);
                Some(url.to_string())
            })
            .ok_or_else(|| {
                AppError::Config("DATABASE_URL or complete PG* variables required".into())
            })?;
        let embed_url = Some(
            configured_value("SOLARISAEL_EMBED_URL", &dotenv)
                .unwrap_or_else(|| DEFAULT_EMBED_URL.into()),
        );
        let embed_dimension = configured_value("SOLARISAEL_EMBED_DIMENSION", &dotenv)
            .unwrap_or_else(|| EMBED_DIMENSION.to_string())
            .parse()
            .map_err(|_| {
                AppError::Config("SOLARISAEL_EMBED_DIMENSION must be an integer".into())
            })?;
        if embed_dimension != EMBED_DIMENSION {
            return Err(AppError::Config(
                "embedding dimension must be 2048 for migration 0002".into(),
            ));
        }
        let test_embedding_disabled =
            configured_value("SOLARISAEL_DISABLE_EMBEDDING", &dotenv).as_deref() == Some("1")
                || configured_value("SOLARISAEL_TEST_DISABLE_EMBEDDING", &dotenv).as_deref()
                    == Some("1");
        Ok(Self {
            database_url,
            embed_model: configured_value("SOLARISAEL_EMBED_MODEL", &dotenv)
                .unwrap_or_else(|| DEFAULT_EMBED_MODEL.into()),
            embed_dimension,
            embed_required: !test_embedding_disabled,
            test_embedding_disabled,
            embed_url,
            giga_source_ledger_dir: configured_value("SOLARISAEL_GIGA_SOURCE_LEDGER_DIR", &dotenv)
                .map(PathBuf::from),
            giga_source_room: configured_value("SOLARISAEL_GIGA_SOURCE_ROOM", &dotenv),
        })
    }

    pub async fn pool(&self) -> Result<PgPool, AppError> {
        let options = PgConnectOptions::from_str(&self.database_url)
            .map_err(|_| AppError::Config("invalid database configuration".into()))?;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(120))
            .connect_with(options)
            .await
            .map_err(AppError::DatabaseConnect)?;
        let shape: String = sqlx::query_scalar("SELECT format_type(a.atttypid, a.atttypmod) FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid WHERE c.relname='memory_chunks' AND a.attname='body_embedding' AND NOT a.attisdropped")
            .fetch_optional(&pool)
            .await
            .map_err(AppError::DatabaseSchema)?
            .ok_or_else(|| {
                AppError::Config(
                    "memory_chunks.body_embedding is missing; apply migration 0002".into(),
                )
            })?;
        if shape != "vector(2048)" {
            return Err(AppError::Config("incompatible embedding schema".into()));
        }
        Ok(pool)
    }
}
