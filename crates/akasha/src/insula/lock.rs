use super::error::InsulaError;

pub(super) async fn lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    h: &str,
    exclusive: bool,
) -> Result<(), InsulaError> {
    let q = if exclusive {
        "SELECT pg_advisory_xact_lock(hashtextextended($1,723684291))"
    } else {
        "SELECT pg_advisory_xact_lock_shared(hashtextextended($1,723684291))"
    };
    sqlx::query(q).bind(h).execute(&mut **tx).await?;
    Ok(())
}
