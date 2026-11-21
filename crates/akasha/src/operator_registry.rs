//! Idempotent registration of a unit and its operator, without changing existing rows.

use hearth::OperatorKey;
use sqlx::PgPool;

use crate::AppError;

pub struct OperatorRegistration {
    pub unit: String,
    pub unit_name: String,
    pub key: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterOutcome {
    pub unit_created: bool,
    pub operator_created: bool,
}

impl OperatorRegistration {
    /// Checks identifiers before the CLI opens a database connection.
    pub fn validate(&self) -> Result<(), AppError> {
        OperatorKey::new(self.key.clone()).map_err(|error| AppError::Invalid(error.to_string()))?;

        if !self.unit.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        }) {
            return Err(AppError::Invalid(
                "unit must contain lowercase ltree labels ([a-z0-9_]+) joined by dots".into(),
            ));
        }

        Ok(())
    }
}

/// Adds the unit and the operator when they are absent. A second run changes nothing.
/// An existing operator in a different unit refuses.
pub async fn register_operator(
    pool: &PgPool,
    registration: &OperatorRegistration,
) -> Result<RegisterOutcome, AppError> {
    registration.validate()?;

    let mut tx = pool.begin().await?;
    let unit_created = sqlx::query(
        "INSERT INTO units (path, display_name) VALUES ($1::ltree, $2)
         ON CONFLICT (path) DO NOTHING",
    )
    .bind(&registration.unit)
    .bind(&registration.unit_name)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let operator_created = sqlx::query(
        "INSERT INTO operators (key, display_name, unit) VALUES ($1, $2, $3::ltree)
         ON CONFLICT (key) DO NOTHING",
    )
    .bind(&registration.key)
    .bind(&registration.display_name)
    .bind(&registration.unit)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    if !operator_created {
        let same_unit: bool =
            sqlx::query_scalar("SELECT unit = $2::ltree FROM operators WHERE key = $1 FOR UPDATE")
                .bind(&registration.key)
                .bind(&registration.unit)
                .fetch_one(&mut *tx)
                .await?;

        if !same_unit {
            tx.rollback().await?;

            return Err(AppError::Invalid(
                "operator is already registered in a different unit".into(),
            ));
        }
    }

    tx.commit().await?;

    Ok(RegisterOutcome {
        unit_created,
        operator_created,
    })
}
