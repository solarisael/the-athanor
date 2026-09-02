use super::receipt::{LessonMutationKind, LessonMutationReceipt, lesson_key, mutation_refusal};
use crate::config::AppError;
use crate::lesson::coerce::deserialize_i64;
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonDeleteParams {
    pub kind: String,
    #[serde(deserialize_with = "deserialize_i64")]
    pub id: i64,
    pub expected_title: String,
}
pub async fn lesson_delete(
    pool: &PgPool,
    p: LessonDeleteParams,
) -> Result<LessonMutationReceipt, AppError> {
    let Some(key) = lesson_key(&p.kind) else {
        return Ok(mutation_refusal(
            &p,
            "kind must be coding-lesson, project-lesson, writing-lesson, or design-lesson",
            None,
            LessonMutationKind::Delete,
        ));
    };
    if p.id <= 0 {
        return Ok(mutation_refusal(
            &p,
            "id must be a positive integer",
            None,
            LessonMutationKind::Delete,
        ));
    }
    if p.expected_title.is_empty() {
        return Ok(mutation_refusal(
            &p,
            "expected_title is required",
            None,
            LessonMutationKind::Delete,
        ));
    }
    let mut tx = pool.begin().await?;
    let actual = sqlx::query_scalar::<_, String>(
        "SELECT title FROM lessons WHERE lesson_key=$1 AND id=$2 FOR UPDATE",
    )
    .bind(key)
    .bind(p.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(actual) = actual else {
        return Ok(mutation_refusal(
            &p,
            "lesson not found",
            None,
            LessonMutationKind::Delete,
        ));
    };
    if actual != p.expected_title {
        return Ok(mutation_refusal(
            &p,
            "title mismatch",
            Some(actual),
            LessonMutationKind::Delete,
        ));
    }
    let changed = sqlx::query("DELETE FROM lessons WHERE lesson_key=$1 AND id=$2 AND title=$3")
        .bind(key)
        .bind(p.id)
        .bind(&p.expected_title)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if changed != 1 {
        return Ok(mutation_refusal(
            &p,
            "delete affected an unexpected number of rows",
            None,
            LessonMutationKind::Delete,
        ));
    }
    tx.commit().await?;
    Ok(LessonMutationReceipt::Deleted {
        kind: p.kind,
        id: p.id,
        title: p.expected_title,
    })
}
