//! The content lane: chunk bodies scored by `word_similarity` against the
//! raw query.
//!
//! The lane is expensive for one reason. `word_similarity(query, body)` costs
//! about 0.15 ms per chunk body on the live corpus, and the planner prices it
//! at one `cpu_operator_cost`. Everything here exists to keep that call off
//! rows that cannot appear in the result, and to let the planner spread the
//! calls it still has to make.
//!
//! Measured on solarisael_memory (7 718 chunks / 2 434 memories, read-only
//! `EXPLAIN ANALYZE`, query `recall latency in the content lane`):
//!
//! * The old shape scored all 7 715 chunks in a hash join: Seq Scan, `Rows
//!   Removed by Filter 6 898`, 1 419 ms.
//! * `$1 <% c.body` under `SET LOCAL pg_trgm.word_similarity_threshold` does
//!   reach `memory_chunks_body_trgm` (Bitmap Index Scan, 3.3 ms), but a prose
//!   query's trigrams match 7 335 of 7 718 chunks, and the lossy recheck pays
//!   the full `word_similarity` cost again: 1 745 ms, worse than what it
//!   replaced. So the lane keeps the plain `sim >= $3` comparison and does not
//!   use `<%`.
//! * Scoping first and scoring once cuts the calls to the 4 227 chunks that
//!   pass the room, archived, superseded and type guard: 761 ms.
//! * The three `SET LOCAL` planner settings below let that scan go parallel:
//!   105-121 ms with 6-7 workers.
//!
//! The settings compensate for one wrong number, not for the planner in
//! general. `word_similarity` really costs about 60 000 `cpu_operator_cost`
//! units, so a scan that the planner prices at 1 274 units actually runs for
//! 750 ms of CPU, and it declines a parallel plan that pays off 6-fold. The
//! durable repair is a truthful cost on the function itself; that is a global
//! change to a pg_trgm object and is not made here. `SET LOCAL` keeps the
//! compensation inside this lane's own transaction.

use crate::config::AppError;
use sqlx::PgPool;
use sqlx::postgres::PgRow;

/// One round trip that applies all three planner settings for the lane's
/// transaction. `set_config(..., true)` is `SET LOCAL`, so the settings end
/// with the transaction and never leak back into the pooled connection.
const LANE_PLANNER_SETTINGS: &str = "SELECT \
     set_config('min_parallel_table_scan_size','0',true),\
     set_config('parallel_setup_cost','10',true),\
     set_config('max_parallel_workers_per_gather','8',true)";

/// The lane SQL for `pattern_count` term patterns.
///
/// Parameters: `$1` query, `$2` rooms, `$3` minimum similarity, `$4` fetch
/// limit (`NULL` for no limit), `$5` the memory kind that recall excludes, and
/// then one parameter per term pattern from `$6`.
///
/// Two properties the old single-statement shape did not have. The empty-term
/// case is a different statement instead of a `$5 = '{}' OR ...` guard, so no
/// OR over a whole predicate reaches the planner. And `sim` is computed in the
/// inner target list, behind `OFFSET 0`, so the outer `sim >= $3` cannot be
/// pushed back down: `word_similarity` runs exactly once per scoped chunk
/// instead of once in the filter plus once in the select list.
pub(crate) fn content_lane_sql(pattern_count: usize) -> String {
    let term_filter = if pattern_count == 0 {
        String::new()
    } else {
        let ors = (0..pattern_count)
            .map(|index| format!("c.body ILIKE ${}", index + 6))
            .collect::<Vec<_>>()
            .join(" OR ");
        format!("\n             AND ({ors})")
    };
    format!(
        "SELECT memory_id,source_path,title,heading_path,body,char_start,char_end,chunk_index,meta,sim
         FROM (
           SELECT m.id AS memory_id,m.source_path,coalesce(m.title,'') AS title,
                  coalesce(c.heading_path,'') AS heading_path,c.body,c.char_start,c.char_end,
                  c.chunk_index,m.meta AS meta,
                  word_similarity($1,c.body)::double precision AS sim
           FROM memory_chunks c
           JOIN memories m ON m.id=c.memory_id
           WHERE m.room = ANY($2::text[])
             AND m.archived_at IS NULL
             AND m.superseded_by IS NULL
             AND COALESCE(m.type,'') <> $5{term_filter}
           OFFSET 0
         ) lane
         WHERE sim >= $3
         ORDER BY sim DESC,source_path,chunk_index
         LIMIT $4"
    )
}

/// Run the content lane in its own transaction, with the planner settings the
/// scan needs.
pub(crate) async fn content_lane_rows(
    pool: &PgPool,
    query: &str,
    rooms: &[String],
    min_similarity: f64,
    fetch_limit: Option<i64>,
    patterns: &[String],
    excluded_kind: &str,
) -> Result<Vec<PgRow>, AppError> {
    let sql = content_lane_sql(patterns.len());
    let mut tx = pool.begin().await?;
    sqlx::query(LANE_PLANNER_SETTINGS).execute(&mut *tx).await?;
    let mut lane = sqlx::query(&sql)
        .bind(query)
        .bind(rooms)
        .bind(min_similarity)
        .bind(fetch_limit)
        .bind(excluded_kind);
    for pattern in patterns {
        lane = lane.bind(pattern.as_str());
    }
    let rows = lane.fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{LANE_PLANNER_SETTINGS, content_lane_sql};

    #[test]
    fn no_terms_drops_the_term_filter_instead_of_guarding_it() {
        let sql = content_lane_sql(0);
        assert!(!sql.contains("ILIKE"));
        assert!(!sql.contains(" OR "));
        assert!(sql.contains("COALESCE(m.type,'') <> $5\n"));
    }

    #[test]
    fn one_parameter_per_term_starting_at_six() {
        let sql = content_lane_sql(3);
        assert!(sql.contains("AND (c.body ILIKE $6 OR c.body ILIKE $7 OR c.body ILIKE $8)"));
        assert!(!sql.contains("$9"));
        // The array form `ILIKE ANY($5)` is what the trigram index cannot use.
        assert!(!sql.contains("ANY($5"));
    }

    #[test]
    fn the_only_or_in_the_statement_is_the_term_group() {
        // A `$5 = '{}' OR <predicate>` guard would put an OR above the term
        // group and hide the whole WHERE clause from any index.
        assert_eq!(content_lane_sql(0).matches(" OR ").count(), 0);
        assert_eq!(content_lane_sql(1).matches(" OR ").count(), 0);
        assert_eq!(content_lane_sql(4).matches(" OR ").count(), 3);
    }

    #[test]
    fn similarity_is_computed_once_behind_offset_zero() {
        let sql = content_lane_sql(2);
        assert_eq!(sql.matches("word_similarity(").count(), 1);
        assert!(sql.contains("OFFSET 0"));
        assert!(sql.contains("WHERE sim >= $3"));
        assert!(sql.contains("ORDER BY sim DESC,source_path,chunk_index"));
    }

    #[test]
    fn planner_settings_are_transaction_local_and_one_round_trip() {
        assert_eq!(LANE_PLANNER_SETTINGS.matches("set_config").count(), 3);
        assert_eq!(LANE_PLANNER_SETTINGS.matches(",true)").count(), 3);
        assert!(LANE_PLANNER_SETTINGS.starts_with("SELECT "));
    }
}
