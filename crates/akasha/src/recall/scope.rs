//! The operator axis as one predicate every memories lane shares.
//!
//! A schema without the `operators` table predates this axis and keeps legacy
//! recall behavior. Once the table exists, row visibility does not depend on
//! whether any operators are registered. Each reader sees its own rows,
//! company-wide rows, and historical rows whose operator is null. The existing
//! registered hierarchy also grants qualifying `up` rows. An unregistered
//! writer's private and `up` rows remain hidden from other readers.
//!
//! `memories.operator` is Host-validated text, not a foreign key. A House can
//! still write before registration, but those rows do not become shared by
//! default. Registering the writer later gives its `up` rows their reach; no
//! row is ever rewritten.

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
    /// The migrated `operators` table enables the visibility predicate even
    /// when the registry is empty. A missing table predates this axis and
    /// keeps legacy recall behavior.
    pub(crate) async fn load(
        pool: &PgPool,
        operator: Option<&str>,
        warnings: &mut Vec<String>,
    ) -> Result<Self, AppError> {
        let axis_exists: bool = sqlx::query_scalar("SELECT to_regclass('operators') IS NOT NULL")
            .fetch_one(pool)
            .await?;
        let active = axis_exists;
        let (registry_populated, registered) = if axis_exists {
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
        if registry_populated && !registered {
            warnings.push(UNREGISTERED_OPERATOR_WARNING.to_owned());
        }
        Ok(Self {
            operator: operator.map(str::to_owned),
            active,
        })
    }

    /// `AND (...)` over the memories row aliased `alias`, reading the reader
    /// key from placeholder `$param`. Empty only before the axis exists.
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

    /// Bind the reader key as the next placeholder when the axis exists;
    /// pairs with [`Self::predicate`].
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
