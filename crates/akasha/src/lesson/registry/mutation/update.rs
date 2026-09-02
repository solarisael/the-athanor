use crate::config::AppError;
use crate::lesson::coerce::deserialize_i64;
use crate::lesson::validate_patterns;
use hearth::lesson_triggers::LessonTriggerSpec;
use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder};
use super::delete::LessonDeleteParams;
use super::receipt::{LessonMutationKind, LessonMutationReceipt, lesson_key, mutation_refusal};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonUpdateParams {
    pub kind: String,
    #[serde(deserialize_with = "deserialize_i64")]
    pub id: i64,
    pub expected_title: String,
    pub patch: Value,
}

/// The trigger columns a patch names, typed for core. Shape errors are
/// refusals here so the coercion below never has to guess.
pub(crate) fn patch_trigger_spec(
    fields: &serde_json::Map<String, Value>,
) -> Result<LessonTriggerSpec, AppError> {
    let strings = |field: &str| -> Result<Vec<String>, AppError> {
        match fields.get(field) {
            None => Ok(Vec::new()),
            Some(value) => serde_json::from_value::<Vec<String>>(value.clone())
                .map_err(|_| AppError::Invalid(format!("{field} must be an array of strings"))),
        }
    };
    let interrupt_mode = match fields.get("interruptMode") {
        None | Some(Value::Null) => None,
        Some(Value::String(mode)) => Some(mode.clone()),
        Some(_) => {
            return Err(AppError::Invalid(
                "interruptMode must be a string or null".into(),
            ));
        }
    };
    let repeat_cooldown_secs = match fields.get("repeatCooldownSecs") {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => Some(
            number
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| {
                    AppError::Invalid("repeatCooldownSecs must be a 32-bit positive integer".into())
                })?,
        ),
        Some(_) => {
            return Err(AppError::Invalid(
                "repeatCooldownSecs must be a number or null".into(),
            ));
        }
    };
    Ok(LessonTriggerSpec {
        condition: strings("condition")?,
        ast_condition: strings("astCondition")?,
        trigger_scope: strings("triggerScope")?,
        interrupt_mode,
        repeat_cooldown_secs,
        ..Default::default()
    })
}
pub async fn lesson_update(
    pool: &PgPool,
    p: LessonUpdateParams,
) -> Result<LessonMutationReceipt, AppError> {
    let delete = LessonDeleteParams {
        kind: p.kind.clone(),
        id: p.id,
        expected_title: p.expected_title.clone(),
    };
    let Some(key) = lesson_key(&p.kind) else {
        return Ok(mutation_refusal(
            &delete,
            "kind must be coding-lesson, project-lesson, writing-lesson, or design-lesson",
            None,
            LessonMutationKind::Update,
        ));
    };
    if p.id <= 0 {
        return Ok(mutation_refusal(
            &delete,
            "id must be a positive integer",
            None,
            LessonMutationKind::Update,
        ));
    }
    if p.expected_title.is_empty() {
        return Ok(mutation_refusal(
            &delete,
            "expected_title is required",
            None,
            LessonMutationKind::Update,
        ));
    }
    let Some(fields) = p.patch.as_object() else {
        return Ok(mutation_refusal(
            &delete,
            "patch must be an object",
            None,
            LessonMutationKind::Update,
        ));
    };
    if fields.is_empty() {
        return Ok(mutation_refusal(
            &delete,
            "at least one update field is required",
            None,
            LessonMutationKind::Update,
        ));
    }
    if fields.contains_key("project") && fields.contains_key("clearProject") {
        return Ok(mutation_refusal(
            &delete,
            "project and clearProject are mutually exclusive",
            None,
            LessonMutationKind::Update,
        ));
    }
    if fields.contains_key("clearProject") && !matches!(key, "coding" | "project") {
        return Ok(mutation_refusal(
            &delete,
            format!("clearProject is not allowed for {}", p.kind),
            None,
            LessonMutationKind::Update,
        ));
    }
    if let Some(clear) = fields.get("clearProject")
        && clear.as_bool() != Some(true)
    {
        return Ok(mutation_refusal(
            &delete,
            "clearProject must be true when provided",
            None,
            LessonMutationKind::Update,
        ));
    }
    let common = [
        "title",
        "body",
        "shape",
        "triggerContext",
        "tags",
        "threadKeys",
        "condition",
        "astCondition",
        "triggerScope",
        "interruptMode",
        "repeatCooldownSecs",
    ];
    let family: &[&str] = match key {
        "coding" => &[
            "voice",
            "scope",
            "project",
            "proofPattern",
            "languageKeys",
            "technologyKeys",
            "negationOf",
            "alwaysOn",
            "clearProject",
        ],
        "project" => &[
            "project",
            "clearProject",
            "proofPattern",
            "languageKeys",
            "technologyKeys",
        ],
        "writing" => &["voice", "register", "exampleText", "writers", "negationOf"],
        "design" => &[
            "voice",
            "register",
            "proofPattern",
            "exampleText",
            "languageKeys",
            "technologyKeys",
        ],
        _ => &[],
    };
    if let Some(invalid) = fields
        .keys()
        .find(|field| !common.contains(&field.as_str()) && !family.contains(&field.as_str()))
    {
        return Ok(mutation_refusal(
            &delete,
            format!("field not allowed for {}: {invalid}", p.kind),
            None,
            LessonMutationKind::Update,
        ));
    }
    // Trigger validation runs before a single row is locked: an uncompilable
    // trigger is a refusal, never a stored pattern that can never fire. A
    // patch is partial, so only the fields it names are judged.
    let patched = patch_trigger_spec(fields)?;
    patched.validate_fields().map_err(AppError::Invalid)?;
    validate_patterns(&patched).map_err(AppError::Invalid)?;
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
            &delete,
            "lesson not found",
            None,
            LessonMutationKind::Update,
        ));
    };
    if actual != p.expected_title {
        return Ok(mutation_refusal(
            &delete,
            "title mismatch",
            Some(actual),
            LessonMutationKind::Update,
        ));
    }
    let mut qb = QueryBuilder::<Postgres>::new("UPDATE lessons SET ");
    let mut separated = qb.separated(", ");
    for (field, value) in fields {
        let column = match field.as_str() {
            "body" => "lesson",
            "triggerContext" => "trigger_context",
            "threadKeys" => "thread_keys",
            "proofPattern" => "proof_pattern",
            "languageKeys" => "language_keys",
            "technologyKeys" => "technology_keys",
            "exampleText" => "example_text",
            "negationOf" => "negation_of",
            "alwaysOn" => "always_on",
            "clearProject" => "project",
            "astCondition" => "ast_condition",
            "triggerScope" => "trigger_scope",
            "interruptMode" => "interrupt_mode",
            "repeatCooldownSecs" => "repeat_cooldown_secs",
            value => value,
        };
        separated.push(format!("{column} = "));
        match field.as_str() {
            "tags" | "threadKeys" | "languageKeys" | "technologyKeys" | "register" | "writers"
            | "condition" | "astCondition" | "triggerScope" => {
                let values =
                    serde_json::from_value::<Vec<String>>(value.clone()).map_err(|_| {
                        AppError::Invalid(format!("{field} must be an array of strings"))
                    })?;
                separated.push_bind_unseparated(values);
            }
            "negationOf" => {
                let id = if value.is_null() {
                    None
                } else {
                    let parsed = match value {
                        Value::Number(number) => number.as_i64(),
                        Value::String(text) => text.parse::<i64>().ok(),
                        _ => None,
                    }
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        AppError::Invalid("negationOf must be null or a positive integer".into())
                    })?;
                    Some(parsed)
                };
                separated.push_bind_unseparated(id);
            }
            "interruptMode" => {
                let mode = match value {
                    Value::Null => None,
                    Value::String(mode) => Some(mode.clone()),
                    _ => {
                        return Err(AppError::Invalid(
                            "interruptMode must be a string or null".into(),
                        ));
                    }
                };
                separated.push_bind_unseparated(mode);
            }
            "repeatCooldownSecs" => {
                let seconds = match value {
                    Value::Null => None,
                    Value::Number(number) => Some(
                        number
                            .as_i64()
                            .and_then(|value| i32::try_from(value).ok())
                            .ok_or_else(|| {
                                AppError::Invalid(
                                    "repeatCooldownSecs must be a 32-bit positive integer".into(),
                                )
                            })?,
                    ),
                    _ => {
                        return Err(AppError::Invalid(
                            "repeatCooldownSecs must be a number or null".into(),
                        ));
                    }
                };
                separated.push_bind_unseparated(seconds);
            }
            "alwaysOn" => {
                let enabled = value
                    .as_bool()
                    .ok_or_else(|| AppError::Invalid("alwaysOn must be a boolean".into()))?;
                separated.push_bind_unseparated(enabled);
            }
            "clearProject" => {
                separated.push_bind_unseparated(Option::<String>::None);
            }
            _ => {
                let text = value
                    .as_str()
                    .ok_or_else(|| AppError::Invalid(format!("{field} must be a string")))?
                    .to_string();
                separated.push_bind_unseparated(text);
            }
        }
    }
    qb.push(" WHERE lesson_key=")
        .push_bind(key)
        .push(" AND id=")
        .push_bind(p.id)
        .push(" AND title=")
        .push_bind(&p.expected_title)
        .push(" RETURNING title, always_on, project");
    let Some((title, always_on, project)) = qb
        .build_query_as::<(String, bool, Option<String>)>()
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(mutation_refusal(
            &delete,
            "update affected an unexpected number of rows",
            None,
            LessonMutationKind::Update,
        ));
    };
    tx.commit().await?;
    Ok(LessonMutationReceipt::Updated {
        kind: p.kind,
        id: p.id,
        title,
        always_on,
        project,
    })
}
