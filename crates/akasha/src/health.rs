use crate::{
    AppError, Config,
    backup::{backup_health, backup_health_in},
    migrations::{MigrationState, migration_pool_with_timeout, migration_state},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::{collections::BTreeSet, env, path::PathBuf, time::Duration};

const REQUIRED_TABLES: &[&str] = &[
    "memories",
    "threads",
    "thread_events",
    "memory_thread_refs",
    "thread_event_links",
    "memory_chunks",
    "named_entities",
    "lessons",
    "lesson_trigger_events",
    "semantic_vocabulary",
    "anamnesis",
    "anamnesis_reps",
    "design_documents",
    "crane_outbox",
    "crane_receipts",
    "crane_dead_letters",
    "insula.log",
    "insula.vitals_minute",
    "insula.retention_receipts",
    "insula.log_tombstones",
];
const REQUIRED_EXTENSIONS: &[&str] = &["pg_trgm", "pgcrypto", "vector"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateHealthOptions {
    pub skip_embedding: bool,
    pub max_backup_age_hours: f64,
    pub state_root: Option<PathBuf>,
    pub state_root_source: Option<String>,
    pub dotenv: Option<PathBuf>,
    pub substrate_dir: Option<PathBuf>,
    pub backup_directory: Option<PathBuf>,
}

impl Default for SubstrateHealthOptions {
    fn default() -> Self {
        Self {
            skip_embedding: false,
            max_backup_age_hours: 24.0,
            state_root: None,
            state_root_source: None,
            dotenv: None,
            substrate_dir: None,
            backup_directory: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateHealthResult {
    pub ok: bool,
    pub mode: String,
    pub substrate_api: u8,
    pub scripts: Value,
    pub database: Value,
    pub embedding: Value,
    pub retrieval: Value,
    pub backup: Value,
    pub topology: Value,
    pub degraded_reasons: Vec<String>,
}

fn topology(options: &SubstrateHealthOptions) -> Value {
    let executable = env::current_exe().ok();
    let resolved = crate::state::resolve_state_root();
    let (state_root, source, error) = if let Some(root) = options.state_root.clone() {
        (
            Some(root),
            options
                .state_root_source
                .clone()
                .unwrap_or_else(|| "argument".into()),
            None,
        )
    } else {
        match resolved {
            Ok((root, source)) => (
                Some(root),
                options
                    .state_root_source
                    .clone()
                    .unwrap_or_else(|| source.as_str().to_owned()),
                None,
            ),
            Err(error) => (
                None,
                options.state_root_source.clone().unwrap_or_default(),
                Some(error.to_string()),
            ),
        }
    };
    let substrate_dir = options
        .substrate_dir
        .clone()
        .or_else(|| env::var_os("ATHANOR_SUBSTRATE_ROOT").map(PathBuf::from));
    let dotenv = options
        .dotenv
        .clone()
        .or_else(|| env::var_os("SOLARISAEL_SUBSTRATE_DOTENV_PATH").map(PathBuf::from))
        .or_else(|| {
            state_root
                .as_ref()
                .map(|root| root.join("substrate").join(".env"))
        });
    json!({
        "ok": state_root.is_some(),
        "athanorRoot": substrate_dir.as_ref().and_then(|path| path.parent()).map(|path| path.to_string_lossy().into_owned()),
        "substrateDir": substrate_dir.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "executable": executable.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "executableFound": executable.as_ref().is_some_and(|path| path.is_file()),
        "executableSource": "current_process",
        "stateRoot": state_root.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "stateRootSource": if source.is_empty() { Value::Null } else { Value::String(source) },
        "substrateStateDir": state_root.as_ref().map(|path| path.join("substrate").to_string_lossy().into_owned()),
        "dotenv": dotenv.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "dotenvExists": dotenv.as_ref().is_some_and(|path| path.is_file()),
        "error": error,
    })
}

fn database_contract_complete(
    migrations: &MigrationState,
    missing_tables: &[&str],
    missing_extensions: &[&str],
    embedding_shape: Option<&str>,
) -> bool {
    migrations.complete
        && migrations.current_version == migrations.target_version
        && missing_tables.is_empty()
        && missing_extensions.is_empty()
        && embedding_shape == Some("vector(2048)")
}

async fn database_health(config: Option<&Config>) -> (Value, Option<PgPool>) {
    let Some(config) = config else {
        return (
            (json!({"ok": false, "reachable": false, "error": "database configuration is unavailable"})),
            None,
        );
    };
    let pool = match migration_pool_with_timeout(config, Duration::from_secs(3)).await {
        Ok(pool) => pool,
        Err(error) => {
            return (
                json!({"ok": false, "reachable": false, "error": error.to_string()}),
                None,
            );
        }
    };
    let identity = sqlx::query("SELECT current_database(), current_user")
        .fetch_one(&pool)
        .await;
    let row = match identity {
        Ok(row) => row,
        Err(error) => {
            return (
                json!({"ok": false, "reachable": false, "error": error.to_string()}),
                None,
            );
        }
    };
    let database: String = row.get(0);
    let user: String = row.get(1);
    let extensions = match sqlx::query_scalar::<_, String>(
        "SELECT extname FROM pg_extension WHERE extname = ANY($1) ORDER BY extname",
    )
    .bind(REQUIRED_EXTENSIONS)
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return (
                json!({"ok": false, "reachable": true, "database": database, "user": user, "error": error.to_string()}),
                Some(pool),
            );
        }
    };
    let tables = match sqlx::query_scalar::<_, String>(
        "SELECT CASE
                    WHEN table_schema = 'insula' THEN 'insula.' || table_name
                    ELSE table_name
                END
         FROM information_schema.tables
         WHERE (table_schema = current_schema() AND table_name = ANY($1))
            OR (table_schema = 'insula' AND 'insula.' || table_name = ANY($1))",
    )
    .bind(REQUIRED_TABLES)
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value.into_iter().collect::<BTreeSet<_>>(),
        Err(error) => {
            return (
                json!({"ok": false, "reachable": true, "database": database, "user": user, "extensions": extensions, "error": error.to_string()}),
                Some(pool),
            );
        }
    };
    let missing_tables = REQUIRED_TABLES
        .iter()
        .filter(|table| !tables.contains(**table))
        .copied()
        .collect::<Vec<_>>();
    let missing_extensions = REQUIRED_EXTENSIONS
        .iter()
        .filter(|extension| !extensions.iter().any(|found| found.as_str() == **extension))
        .copied()
        .collect::<Vec<_>>();
    let migrations = match migration_state(&pool).await {
        Ok(state) => state,
        Err(error) => {
            return (
                json!({
                    "ok": false,
                    "reachable": true,
                    "database": database,
                    "user": user,
                    "extensions": extensions,
                    "missingExtensions": missing_extensions,
                    "missingTables": missing_tables,
                    "error": error.to_string(),
                }),
                Some(pool),
            );
        }
    };
    let shape = sqlx::query_scalar::<_, String>("SELECT format_type(a.atttypid, a.atttypmod) FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid WHERE c.relname='memory_chunks' AND a.attname='body_embedding' AND NOT a.attisdropped")
        .fetch_optional(&pool)
        .await;
    let shape = match shape {
        Ok(shape) => shape,
        Err(error) => {
            return (
                json!({"ok": false, "reachable": true, "database": database, "user": user, "schemaVersion": migrations.current_version, "error": error.to_string()}),
                Some(pool),
            );
        }
    };
    let complete = database_contract_complete(
        &migrations,
        &missing_tables,
        &missing_extensions,
        shape.as_deref(),
    );
    (
        json!({
            "ok": complete,
            "reachable": true,
            "database": database,
            "user": user,
            "schemaVersion": migrations.current_version,
            "targetSchemaVersion": migrations.target_version,
            "schemaComplete": migrations.complete,
            "pendingMigrations": migrations.pending,
            "extensions": extensions,
            "missingExtensions": missing_extensions,
            "missingTables": missing_tables,
            "embeddingShape": shape,
            "error": (!complete).then_some("database schema is incomplete or incompatible"),
        }),
        Some(pool),
    )
}

async fn embedding_health(config: Option<&Config>, skip: bool) -> Value {
    if skip {
        return json!({"ok": null, "skipped": true});
    }
    let Some(config) = config else {
        return json!({"ok": false, "error": "embedding configuration is unavailable"});
    };
    let Some(url) = config.embed_url.as_ref() else {
        return json!({"ok": false, "model": config.embed_model, "expectedDimension": config.embed_dimension, "error": "embedding URL is unavailable"});
    };
    let response = reqwest::Client::new()
        .post(url)
        .timeout(Duration::from_secs(8))
        .json(&json!({"model": config.embed_model, "input": "passage: solarisael house health"}))
        .send()
        .await;
    let body = match response {
        Ok(response) => match response.error_for_status() {
            Ok(response) => match response.json::<Value>().await {
                Ok(body) => body,
                Err(error) => {
                    return json!({"ok": false, "url": url, "model": config.embed_model, "expectedDimension": config.embed_dimension, "error": error.to_string()});
                }
            },
            Err(error) => {
                return json!({"ok": false, "url": url, "model": config.embed_model, "expectedDimension": config.embed_dimension, "error": error.to_string()});
            }
        },
        Err(error) => {
            return json!({"ok": false, "url": url, "model": config.embed_model, "expectedDimension": config.embed_dimension, "error": error.to_string()});
        }
    };
    let dimension = body
        .get("embeddings")
        .and_then(Value::as_array)
        .and_then(|vectors| vectors.first())
        .and_then(Value::as_array)
        .map(Vec::len)
        .or_else(|| {
            body.get("data")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("embedding"))
                .and_then(Value::as_array)
                .map(Vec::len)
        });
    let ok = dimension == Some(config.embed_dimension);
    json!({
        "ok": ok,
        "url": url,
        "model": config.embed_model,
        "dimension": dimension,
        "expectedDimension": config.embed_dimension,
        "error": (!ok).then(|| format!("embedding dimension is {}, expected {}", dimension.map(|value| value.to_string()).unwrap_or_else(|| "missing".into()), config.embed_dimension)),
    })
}

pub async fn substrate_health(options: SubstrateHealthOptions) -> SubstrateHealthResult {
    substrate_health_with_config(options, Config::from_env()).await
}

pub async fn substrate_health_with_config(
    options: SubstrateHealthOptions,
    config: Result<Config, AppError>,
) -> SubstrateHealthResult {
    let topology = topology(&options);
    let config_error = config.as_ref().err().map(ToString::to_string);
    let config = config.ok();
    let (database, _pool) = database_health(config.as_ref()).await;
    let embedding = embedding_health(config.as_ref(), options.skip_embedding).await;
    let backup_result = if let Some(directory) = options.backup_directory.clone() {
        backup_health_in(directory, options.max_backup_age_hours)
    } else {
        backup_health(options.max_backup_age_hours)
    };
    let backup = match backup_result {
        Ok(health) => serde_json::to_value(health)
            .unwrap_or_else(|error| json!({"ok": false, "error": error.to_string()})),
        Err(error) => json!({"ok": false, "directory": null, "error": error.to_string()}),
    };
    let retrieval =
        json!({"ok": null, "skipped": true, "reason": "not requested by substrate health"});
    let scripts = json!({"ok": true, "missing": [], "owner": "rust"});
    let mut degraded_reasons = Vec::new();
    if topology.get("ok") != Some(&Value::Bool(true)) {
        degraded_reasons.push("Athanor state root is unresolved".into());
    }
    if let Some(error) = config_error {
        degraded_reasons.push(format!("substrate configuration is invalid: {error}"));
    }
    if database.get("ok") != Some(&Value::Bool(true)) {
        degraded_reasons.push("PostgreSQL substrate is unavailable or incomplete".into());
    }
    if embedding.get("ok") == Some(&Value::Bool(false)) {
        degraded_reasons.push("embedding service is unavailable or incompatible".into());
    }
    if backup.get("ok") == Some(&Value::Bool(false)) {
        degraded_reasons.push("backup safety net is stale or missing".into());
    }
    SubstrateHealthResult {
        ok: degraded_reasons.is_empty(),
        mode: if degraded_reasons.is_empty() {
            "full".into()
        } else {
            "degraded".into()
        },
        substrate_api: 1,
        scripts,
        database,
        embedding,
        retrieval,
        backup,
        topology,
        degraded_reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_configuration_is_a_structured_degraded_verdict() {
        let verdict = substrate_health_with_config(
            SubstrateHealthOptions {
                skip_embedding: true,
                ..Default::default()
            },
            Err(AppError::Config("missing test configuration".into())),
        )
        .await;
        assert_eq!(verdict.substrate_api, 1);
        assert!(verdict.database.get("reachable").is_some());
        assert!(verdict.topology.is_object());
        assert_eq!(verdict.mode, if verdict.ok { "full" } else { "degraded" });
    }

    #[test]
    fn database_contract_refuses_wrong_schema_extensions_tables_and_vector_shape() {
        let complete = MigrationState {
            current_version: 3,
            target_version: 3,
            applied: (1..=3).collect(),
            pending: Vec::new(),
            complete: true,
        };
        assert!(database_contract_complete(
            &complete,
            &[],
            &[],
            Some("vector(2048)")
        ));
        let wrong_schema = MigrationState {
            current_version: 2,
            ..complete.clone()
        };
        assert!(!database_contract_complete(
            &wrong_schema,
            &[],
            &[],
            Some("vector(2048)")
        ));
        assert!(!database_contract_complete(
            &complete,
            &[],
            &["pgcrypto"],
            Some("vector(2048)")
        ));
        assert!(!database_contract_complete(
            &complete,
            &["crane_outbox"],
            &[],
            Some("vector(2048)")
        ));
        assert!(!database_contract_complete(
            &complete,
            &[],
            &[],
            Some("vector(768)")
        ));
    }
}
