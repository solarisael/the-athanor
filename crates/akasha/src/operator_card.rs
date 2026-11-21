use crate::config::AppError;
use sqlx::PgPool;

#[derive(sqlx::FromRow)]
pub struct OperatorCardRow {
    pub entity_id: i64,
    pub name: String,
    pub kind: String,
    pub summary: String,
}

/// Active, weighty facts whose subject is `reader`, from `room` and `house`.
/// Order: room rows first, then `house`; then `name`, then `id`. At most `limit` rows.
pub async fn operator_card_rows(
    pool: &PgPool,
    room: &str,
    reader: &str,
    limit: i64,
) -> Result<Vec<OperatorCardRow>, AppError> {
    Ok(sqlx::query_as::<_, OperatorCardRow>(
        "SELECT id AS entity_id, name, kind, summary
         FROM named_entities
         WHERE authority = 'active' AND weighty AND subject = $2
           AND (room = $1 OR room = 'house')
         ORDER BY (room = $1) DESC, name, id
         LIMIT $3",
    )
    .bind(room)
    .bind(reader)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}
