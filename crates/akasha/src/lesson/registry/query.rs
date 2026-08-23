use crate::config::{AppError, ROOM_KEY_RE};
use crate::lesson::defaults::default_twelve;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::BTreeSet;
use super::family::LessonFamily;

const LESSON_SELECT: &str = "SELECT id,lesson_key,kind_path,scope,project,voice,register,shape,stage,title,lesson,trigger_context,proof_pattern,example_text,example_cmd,writers,tools,negation_of,language_keys,technology_keys,tags,thread_keys,always_on,condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs FROM lessons";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonQueryParams {
    pub room: String,
    #[serde(rename = "type")]
    pub family: LessonFamily,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub register: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub language_keys: Vec<String>,
    #[serde(default)]
    pub technology_keys: Vec<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub always_on: bool,
    #[serde(default = "default_twelve")]
    pub limit: u32,
}
impl LessonQueryParams {
    pub fn validate(&self) -> Result<(), AppError> {
        if !ROOM_KEY_RE.is_match(&self.room) {
            return Err(AppError::Invalid("room must be a lowercase slug".into()));
        }
        if self.family == LessonFamily::Project
            && self.project.as_deref().is_none_or(|v| v.trim().is_empty())
        {
            return Err(AppError::Invalid("project lessons require project".into()));
        }
        if !(1..=50).contains(&self.limit) {
            return Err(AppError::Invalid(
                "limit must be an integer from 1 through 50".into(),
            ));
        }
        Ok(())
    }
    fn scopes(&self) -> Vec<String> {
        if self.room == "house" {
            vec!["house".into()]
        } else {
            vec!["house".into(), self.room.clone()]
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonRecord {
    pub id: i64,
    #[serde(rename = "type")]
    pub family: String,
    pub kind_path: String,
    pub scope: String,
    pub project: Option<String>,
    pub voice: Option<String>,
    pub register: Vec<String>,
    pub shape: Option<String>,
    pub stage: Vec<String>,
    pub title: String,
    pub lesson: String,
    pub trigger_context: Option<String>,
    pub proof_pattern: Option<String>,
    pub example_text: Option<String>,
    pub example_command: Option<String>,
    pub writers: Vec<String>,
    pub tools: Vec<String>,
    pub negation_of: Option<i64>,
    pub language_keys: Vec<String>,
    pub technology_keys: Vec<String>,
    pub thread_keys: Vec<String>,
    pub tags: Vec<String>,
    pub always_on: bool,
    pub condition: Vec<String>,
    pub ast_condition: Vec<String>,
    pub trigger_scope: Vec<String>,
    pub interrupt_mode: Option<String>,
    pub repeat_cooldown_secs: Option<i32>,
}
fn lesson_record(row: &sqlx::postgres::PgRow) -> Result<LessonRecord, sqlx::Error> {
    Ok(LessonRecord {
        id: row.try_get("id")?,
        family: row.try_get("lesson_key")?,
        kind_path: row.try_get("kind_path")?,
        scope: row.try_get("scope")?,
        project: row.try_get("project")?,
        voice: row.try_get("voice")?,
        register: row.try_get("register")?,
        shape: row.try_get("shape")?,
        stage: row.try_get("stage")?,
        title: row.try_get("title")?,
        lesson: row.try_get("lesson")?,
        trigger_context: row.try_get("trigger_context")?,
        proof_pattern: row.try_get("proof_pattern")?,
        example_text: row.try_get("example_text")?,
        example_command: row.try_get("example_cmd")?,
        writers: row.try_get("writers")?,
        tools: row.try_get("tools")?,
        negation_of: row.try_get("negation_of")?,
        language_keys: row.try_get("language_keys")?,
        technology_keys: row.try_get("technology_keys")?,
        thread_keys: row.try_get("thread_keys")?,
        tags: row.try_get("tags")?,
        always_on: row.try_get("always_on")?,
        condition: row.try_get("condition")?,
        ast_condition: row.try_get("ast_condition")?,
        trigger_scope: row.try_get("trigger_scope")?,
        interrupt_mode: row.try_get("interrupt_mode")?,
        repeat_cooldown_secs: row.try_get("repeat_cooldown_secs")?,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonTaxonomy {
    pub kind_path: String,
    pub shape: Option<String>,
    pub count: i64,
    pub always_on_count: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonFilters {
    pub room: String,
    pub scopes: Vec<String>,
    pub shape: Option<String>,
    pub project: Option<String>,
    pub register: Option<String>,
    pub stage: Option<String>,
    pub language_keys: Vec<String>,
    pub technology_keys: Vec<String>,
    pub query: Option<String>,
    pub always_on: bool,
    pub limit: u32,
}
#[derive(Debug, Serialize)]
pub struct LessonQueryResult {
    pub ok: bool,
    #[serde(rename = "type")]
    pub family: String,
    pub filters: LessonFilters,
    pub lessons: Vec<LessonRecord>,
    pub taxonomy: Vec<LessonTaxonomy>,
}

fn eligibility(qb: &mut QueryBuilder<'_, Postgres>, language: &[String], technology: &[String]) {
    if language.is_empty() {
        qb.push(" AND cardinality(language_keys) = 0");
    } else {
        qb.push(" AND (cardinality(language_keys) = 0 OR language_keys && ")
            .push_bind(language.to_vec())
            .push(")");
    }
    if technology.is_empty() {
        qb.push(" AND cardinality(technology_keys) = 0");
    } else {
        qb.push(" AND (cardinality(technology_keys) = 0 OR technology_keys && ")
            .push_bind(technology.to_vec())
            .push(")");
    }
}

pub async fn lesson_query(
    pool: &PgPool,
    params: LessonQueryParams,
) -> Result<LessonQueryResult, AppError> {
    params.validate()?;
    let scopes = params.scopes();
    let mut qb = QueryBuilder::<Postgres>::new(LESSON_SELECT);
    qb.push(" WHERE lesson_key = ")
        .push_bind(params.family.as_str());
    if params.family == LessonFamily::Coding {
        qb.push(" AND scope = ANY(")
            .push_bind(scopes.clone())
            .push(")");
    }
    if let Some(project) = params.project.as_ref() {
        qb.push(" AND project = ").push_bind(project);
    }
    if let Some(shape) = params.shape.as_ref() {
        qb.push(" AND shape = ").push_bind(shape);
    }
    if let Some(register) = params.register.as_ref() {
        qb.push(" AND ")
            .push_bind(register)
            .push(" = ANY(register)");
    }
    if let Some(stage) = params.stage.as_ref() {
        qb.push(" AND ").push_bind(stage).push(" = ANY(stage)");
    }
    eligibility(&mut qb, &params.language_keys, &params.technology_keys);
    if params.always_on {
        qb.push(" AND always_on");
    }
    if let Some(query) = params.query.as_ref().filter(|v| !v.is_empty()) {
        qb.push(" AND lesson_tsv @@ plainto_tsquery(CASE WHEN lesson_key = 'audio' THEN 'english'::regconfig ELSE 'portuguese'::regconfig END, ").push_bind(query).push(")");
    }
    let rank = params.query.clone().unwrap_or_default();
    qb.push(" ORDER BY always_on DESC, CASE WHEN ").push_bind(rank.clone()).push(" <> '' THEN ts_rank(lesson_tsv, plainto_tsquery(CASE WHEN lesson_key = 'audio' THEN 'english'::regconfig ELSE 'portuguese'::regconfig END, ").push_bind(rank).push(")) ELSE 0 END DESC, updated_at DESC, id LIMIT ").push_bind(i64::from(params.limit));
    let mut lessons = qb
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(lesson_record)
        .collect::<Result<Vec<_>, _>>()?;

    let expansion_keys = lessons
        .iter()
        .flat_map(|row| row.thread_keys.iter())
        .filter(|key| !key.trim().is_empty())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !expansion_keys.is_empty() && lessons.len() < 50 {
        let ids = lessons.iter().map(|row| row.id).collect::<Vec<_>>();
        let mut expand = QueryBuilder::<Postgres>::new(LESSON_SELECT);
        expand
            .push(" WHERE lesson_key = ")
            .push_bind(params.family.as_str())
            .push(" AND thread_keys && ")
            .push_bind(expansion_keys)
            .push(" AND NOT (id = ANY(")
            .push_bind(ids)
            .push("))");
        if params.family == LessonFamily::Coding {
            expand
                .push(" AND scope = ANY(")
                .push_bind(scopes.clone())
                .push(")");
        }
        if let Some(project) = params.project.as_ref() {
            expand.push(" AND project = ").push_bind(project);
        }
        eligibility(&mut expand, &params.language_keys, &params.technology_keys);
        if params.always_on {
            expand.push(" AND always_on");
        }
        expand
            .push(" ORDER BY always_on DESC, updated_at DESC, id LIMIT ")
            .push_bind((50 - lessons.len()) as i64);
        let rows = expand.build().fetch_all(pool).await?;
        lessons.extend(
            rows.iter()
                .map(lesson_record)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    let mut taxonomy_q = QueryBuilder::<Postgres>::new(
        "SELECT kind_path,shape,COUNT(*) AS count,COUNT(*) FILTER (WHERE always_on) AS always_on_count FROM lessons WHERE lesson_key = ",
    );
    taxonomy_q.push_bind(params.family.as_str());
    if params.family == LessonFamily::Coding {
        taxonomy_q
            .push(" AND scope = ANY(")
            .push_bind(scopes.clone())
            .push(")");
    }
    if let Some(project) = params.project.as_ref() {
        taxonomy_q.push(" AND project = ").push_bind(project);
    }
    eligibility(
        &mut taxonomy_q,
        &params.language_keys,
        &params.technology_keys,
    );
    if params.always_on {
        taxonomy_q.push(" AND always_on");
    }
    taxonomy_q.push(" GROUP BY kind_path,shape ORDER BY count DESC,kind_path");
    let taxonomy = taxonomy_q
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| {
            Ok(LessonTaxonomy {
                kind_path: row.try_get("kind_path")?,
                shape: row.try_get("shape")?,
                count: row.try_get("count")?,
                always_on_count: row.try_get("always_on_count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(LessonQueryResult {
        ok: true,
        family: params.family.as_str().into(),
        filters: LessonFilters {
            room: params.room,
            scopes: if params.family == LessonFamily::Coding {
                scopes
            } else {
                vec![]
            },
            shape: params.shape,
            project: params.project,
            register: params.register,
            stage: params.stage,
            language_keys: params.language_keys,
            technology_keys: params.technology_keys,
            query: params.query,
            always_on: params.always_on,
            limit: params.limit,
        },
        lessons,
        taxonomy,
    })
}
