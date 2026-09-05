use crate::{AppError, Config};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{collections::BTreeSet, str::FromStr, time::Duration};

#[derive(Clone, Copy)]
struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_initial.sql",
        sql: include_str!("../../../substrate/migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "0002_nemotron_2048.sql",
        sql: include_str!("../../../substrate/migrations/0002_nemotron_2048.sql"),
    },
    Migration {
        version: 3,
        name: "0003_giga.sql",
        sql: include_str!("../../../substrate/migrations/0003_giga.sql"),
    },
    Migration {
        version: 4,
        name: "0004_giga_runtime.sql",
        sql: include_str!("../../../substrate/migrations/0004_giga_runtime.sql"),
    },
    Migration {
        version: 5,
        name: "0005_giga_resonance.sql",
        sql: include_str!("../../../substrate/migrations/0005_giga_resonance.sql"),
    },
    Migration {
        version: 6,
        name: "0006_memory_thread_graph.sql",
        sql: include_str!("../../../substrate/migrations/0006_memory_thread_graph.sql"),
    },
    Migration {
        version: 7,
        name: "0007_giga_source_ordinal.sql",
        sql: include_str!("../../../substrate/migrations/0007_giga_source_ordinal.sql"),
    },
    Migration {
        version: 8,
        name: "0008_unified_lessons.sql",
        sql: include_str!("../../../substrate/migrations/0008_unified_lessons.sql"),
    },
    Migration {
        version: 9,
        name: "0009_bm25f_memory_search.sql",
        sql: include_str!("../../../substrate/migrations/0009_bm25f_memory_search.sql"),
    },
    Migration {
        version: 10,
        name: "0010_semantic_vocabulary.sql",
        sql: include_str!("../../../substrate/migrations/0010_semantic_vocabulary.sql"),
    },
    Migration {
        version: 11,
        name: "0011_design_lessons.sql",
        sql: include_str!("../../../substrate/migrations/0011_design_lessons.sql"),
    },
    Migration {
        version: 12,
        name: "0012_design_documents.sql",
        sql: include_str!("../../../substrate/migrations/0012_design_documents.sql"),
    },
    Migration {
        version: 13,
        name: "0013_lesson_eligibility_keys.sql",
        sql: include_str!("../../../substrate/migrations/0013_lesson_eligibility_keys.sql"),
    },
    Migration {
        version: 14,
        name: "0014_lesson_threads.sql",
        sql: include_str!("../../../substrate/migrations/0014_lesson_threads.sql"),
    },
    Migration {
        version: 15,
        name: "0015_canon_authority.sql",
        sql: include_str!("../../../substrate/migrations/0015_canon_authority.sql"),
    },
    Migration {
        version: 16,
        name: "0016_boat_ready_delivery.sql",
        sql: include_str!("../../../substrate/migrations/0016_boat_ready_delivery.sql"),
    },
    Migration {
        version: 17,
        name: "0017_crane_delivery.sql",
        sql: include_str!("../../../substrate/migrations/0017_crane_delivery.sql"),
    },
    Migration {
        version: 18,
        name: "0018_hallway_chatrooms.sql",
        sql: include_str!("../../../substrate/migrations/0018_hallway_chatrooms.sql"),
    },
    Migration {
        version: 19,
        name: "0019_lesson_triggers.sql",
        sql: include_str!("../../../substrate/migrations/0019_lesson_triggers.sql"),
    },
    Migration {
        version: 20,
        name: "0020_hallway_bell.sql",
        sql: include_str!("../../../substrate/migrations/0020_hallway_bell.sql"),
    },
    Migration {
        version: 21,
        name: "0021_hallway_knock.sql",
        sql: include_str!("../../../substrate/migrations/0021_hallway_knock.sql"),
    },
    Migration {
        version: 22,
        name: "0022_insula.sql",
        sql: include_str!("../../../substrate/migrations/0022_insula.sql"),
    },
    Migration {
        version: 23,
        name: "0023_docket.sql",
        sql: include_str!("../../../substrate/migrations/0023_docket.sql"),
    },
    Migration {
        version: 24,
        name: "0024_docket_capability.sql",
        sql: include_str!("../../../substrate/migrations/0024_docket_capability.sql"),
    },
    Migration {
        version: 25,
        name: "0025_docket_draft_abandon.sql",
        sql: include_str!("../../../substrate/migrations/0025_docket_draft_abandon.sql"),
    },
    Migration {
        version: 26,
        name: "0026_restart.sql",
        sql: include_str!("../../../substrate/migrations/0026_restart.sql"),
    },
    Migration {
        version: 27,
        name: "0027_restart_successor_proof.sql",
        sql: include_str!("../../../substrate/migrations/0027_restart_successor_proof.sql"),
    },
    Migration {
        version: 28,
        name: "0028_room_settings.sql",
        sql: include_str!("../../../substrate/migrations/0028_room_settings.sql"),
    },
    Migration {
        version: 29,
        name: "0029_insula_log_lane_spans.sql",
        sql: include_str!("../../../substrate/migrations/0029_insula_log_lane_spans.sql"),
    },
    Migration {
        version: 30,
        name: "0030_presence_sessions.sql",
        sql: include_str!("../../../substrate/migrations/0030_presence_sessions.sql"),
    },
];

/// The consolidated lineage as recorded in `schema_migrations`: every version
/// this binary knows, in order. Derived from the registry so the backup
/// allowlist can never drift behind a newly registered migration again.
pub(crate) fn consolidated_version_labels() -> Vec<String> {
    MIGRATIONS
        .iter()
        .map(|migration| migration.version.to_string())
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationState {
    pub current_version: i32,
    pub target_version: i32,
    pub applied: Vec<i32>,
    pub pending: Vec<i32>,
    pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub before: MigrationState,
    pub after: MigrationState,
    pub applied: Vec<String>,
}

pub async fn migration_pool(config: &Config) -> Result<PgPool, AppError> {
    migration_pool_with_timeout(config, Duration::from_secs(120)).await
}

pub async fn migration_pool_with_timeout(
    config: &Config,
    timeout: Duration,
) -> Result<PgPool, AppError> {
    let options = sqlx::postgres::PgConnectOptions::from_str(&config.database_url)
        .map_err(|_| AppError::Config("invalid database configuration".into()))?;
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(timeout)
        .connect_with(options)
        .await
        .map_err(AppError::DatabaseConnect)
}

async fn applied_versions(pool: &PgPool) -> Result<Vec<i32>, AppError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT to_regclass(current_schema() || '.schema_migrations') IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::DatabaseSchema)?;
    if !exists {
        return Ok(Vec::new());
    }
    sqlx::query_scalar::<_, i32>("SELECT version FROM schema_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .map_err(AppError::DatabaseSchema)
}

fn state_from(applied: Vec<i32>) -> Result<MigrationState, AppError> {
    let expected: Vec<i32> = MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .collect();
    let known: BTreeSet<i32> = expected.iter().copied().collect();
    let actual: BTreeSet<i32> = applied.iter().copied().collect();
    if applied.iter().any(|version| !known.contains(version)) {
        return Err(AppError::Config(format!(
            "database contains migrations not owned by this substrate: {}",
            applied
                .iter()
                .filter(|version| !known.contains(version))
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )));
    }
    let prefix = expected
        .iter()
        .take(applied.len())
        .copied()
        .collect::<Vec<_>>();
    if applied != prefix {
        return Err(AppError::Config(format!(
            "partial migration lineage refused: expected applied prefix {:?}, found {:?}",
            prefix, applied
        )));
    }
    let pending = expected
        .iter()
        .copied()
        .filter(|version| !actual.contains(version))
        .collect::<Vec<_>>();
    Ok(MigrationState {
        current_version: applied.last().copied().unwrap_or(0),
        target_version: expected.last().copied().unwrap_or(0),
        complete: pending.is_empty(),
        applied,
        pending,
    })
}

pub async fn migration_state(pool: &PgPool) -> Result<MigrationState, AppError> {
    state_from(applied_versions(pool).await?)
}

fn transactional_sql(migration: Migration) -> Result<String, AppError> {
    let lines = migration.sql.lines().collect::<Vec<_>>();
    let begin = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("BEGIN;"));
    let commit = lines
        .iter()
        .rposition(|line| line.trim().eq_ignore_ascii_case("COMMIT;"));
    let (Some(begin), Some(commit)) = (begin, commit) else {
        return Err(AppError::Config(format!(
            "migration {} lacks an explicit transaction",
            migration.name
        )));
    };
    if begin >= commit
        || lines[..begin].iter().any(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with("--")
        })
        || lines[commit + 1..].iter().any(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with("--")
        })
    {
        return Err(AppError::Config(format!(
            "migration {} has ambiguous transaction authority",
            migration.name
        )));
    }
    Ok(lines[begin + 1..commit].join("\n"))
}

pub async fn run_migrations(pool: &PgPool) -> Result<MigrationResult, AppError> {
    let before = migration_state(pool).await?;
    if before.complete {
        return Ok(MigrationResult {
            before: before.clone(),
            after: before,
            applied: Vec::new(),
        });
    }

    let mut tx = pool.begin().await.map_err(AppError::Database)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('athanor_schema_migrations'))")
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())")
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;

    let locked_applied =
        sqlx::query_scalar::<_, i32>("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::DatabaseSchema)?;
    let locked_state = state_from(locked_applied)?;
    let mut names = Vec::with_capacity(locked_state.pending.len());
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| locked_state.pending.contains(&migration.version))
    {
        let sql = transactional_sql(*migration)?;
        sqlx::raw_sql(&sql)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
        sqlx::query(
            "INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT (version) DO NOTHING",
        )
        .bind(migration.version)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;
        names.push(migration.name.to_owned());
    }
    tx.commit().await.map_err(AppError::Database)?;
    let after = migration_state(pool).await?;
    if !after.complete {
        return Err(AppError::Config(
            "migration execution ended without the complete owned lineage".into(),
        ));
    }
    Ok(MigrationResult {
        before,
        after,
        applied: names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_lineage_accepts_only_an_ordered_prefix() {
        assert!(state_from(vec![]).unwrap().pending.len() == MIGRATIONS.len());
        assert!(state_from(vec![1, 2, 3]).is_ok());
        assert!(state_from(vec![1, 3]).is_err());
        assert!(state_from(vec![1, 2, 99]).is_err());
    }

    #[test]
    fn every_embedded_migration_has_one_unambiguous_outer_transaction() {
        for migration in MIGRATIONS {
            let sql = transactional_sql(*migration).unwrap();
            assert!(!sql.trim().is_empty(), "{}", migration.name);
            assert!(
                !sql.lines()
                    .any(|line| line.trim().eq_ignore_ascii_case("BEGIN;")),
                "{}",
                migration.name
            );
            assert!(
                !sql.lines()
                    .any(|line| line.trim().eq_ignore_ascii_case("COMMIT;")),
                "{}",
                migration.name
            );
        }
    }
}
