//! The operator axis as one predicate every memories lane shares.
//!
//! A House with no `operators` rows renders no SQL and binds nothing, so its
//! recall is byte-for-byte what it was before the axis existed. Once operators
//! exist, a row is visible to reader R when R wrote it, when it is company-wide,
//! when it predates the axis (`operator IS NULL`), or when it travels `up` and R
//! sits at or above the writer's unit with a higher rank.
//!
//! enough: `memories.operator` is Host-validated text, not a foreign key, so a
//! House keeps remembering before anyone is registered. A row whose writer
//! has no `operators` row misses the writer join below: it reaches its writer
//! and, when company-wide, every reader, and nobody else. Registering the
//! writer later gives its `up` rows their reach; no row is ever rewritten.

use crate::config::AppError;
use sqlx::PgPool;
use sqlx::postgres::{PgArguments, Postgres};
use sqlx::query::{Query, QueryScalar};

pub(crate) const UNREGISTERED_OPERATOR_WARNING: &str = "operator not registered";

#[derive(Clone, Debug)]
pub(crate) struct ScopeFilter {
    operator: Option<String>,
    active: bool,
}

impl ScopeFilter {
    /// Two catalog-sized reads per recall. A reader the House does
    /// not know still reads company-wide and pre-axis rows, and the recall says
    /// why it saw no more. A schema older than 0035 has no operators at all, so
    /// it reads as an inactive axis rather than a failed recall.
    pub(crate) async fn load(
        pool: &PgPool,
        operator: Option<&str>,
        warnings: &mut Vec<String>,
    ) -> Result<Self, AppError> {
        let axis_exists: bool = sqlx::query_scalar("SELECT to_regclass('operators') IS NOT NULL")
            .fetch_one(pool)
            .await?;
        let (active, registered) = if axis_exists {
            sqlx::query_as::<_, (bool, bool)>(
                "SELECT EXISTS (SELECT 1 FROM operators),
                        EXISTS (SELECT 1 FROM operators WHERE key = $1)",
            )
            .bind(operator)
            .fetch_one(pool)
            .await?
        } else {
            (false, false)
        };
        if active && !registered {
            warnings.push(UNREGISTERED_OPERATOR_WARNING.to_owned());
        }
        Ok(Self {
            operator: operator.map(str::to_owned),
            active,
        })
    }

    /// `AND (...)` over the memories row aliased `alias`, reading the reader
    /// key from placeholder `$param`. Empty while the axis is inactive.
    pub(crate) fn predicate(&self, alias: &str, param: usize) -> String {
        if !self.active {
            return String::new();
        }
        format!(
            " AND ({alias}.operator IS NULL
                  OR {alias}.scope = 'company'
                  OR {alias}.operator = ${param}
                  OR ({alias}.scope = 'up' AND EXISTS (
                        SELECT 1
                        FROM operators writer
                        JOIN operators reader ON reader.key = ${param}
                        WHERE writer.key = {alias}.operator
                          AND reader.unit @> writer.unit
                          AND reader.rank > writer.rank)))"
        )
    }

    /// `superseded_by` of the memories row aliased `alias`, as the reader may
    /// see it: the successor's ID when the reader could recall that successor,
    /// NULL otherwise, so a visible row never names a hidden one. Reads the
    /// reader key from `$param` like [`Self::predicate`].
    pub(crate) fn visible_successor(&self, alias: &str, param: usize) -> String {
        if !self.active {
            return format!("{alias}.superseded_by");
        }
        format!(
            "(SELECT successor.id FROM memories successor
              WHERE successor.id = {alias}.superseded_by{})",
            self.predicate("successor", param)
        )
    }

    /// Bind the reader key as the next placeholder, only while the axis is
    /// active; pairs with [`Self::predicate`].
    pub(crate) fn bind<'q>(
        &'q self,
        query: Query<'q, Postgres, PgArguments>,
    ) -> Query<'q, Postgres, PgArguments> {
        if self.active {
            query.bind(self.operator.as_deref())
        } else {
            query
        }
    }

    pub(crate) fn bind_scalar<'q, O>(
        &'q self,
        query: QueryScalar<'q, Postgres, O, PgArguments>,
    ) -> QueryScalar<'q, Postgres, O, PgArguments> {
        if self.active {
            query.bind(self.operator.as_deref())
        } else {
            query
        }
    }
}
