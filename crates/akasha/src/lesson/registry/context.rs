use crate::config::{AppError, ROOM_KEY_RE};
use crate::lesson::defaults::default_eight;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonContextParams {
    pub room: String,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub shapes: Vec<String>,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default)]
    pub stages: Vec<String>,
    #[serde(default)]
    pub registers: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub technologies: Vec<String>,
    #[serde(default = "default_eight")]
    pub limit: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct LessonContextMatch {
    pub score: i32,
    pub matched: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct LessonContextRecord {
    pub id: i64,
    #[serde(rename = "type")]
    pub family: String,
    pub title: String,
    pub lesson: String,
    pub proof_pattern: String,
    pub trigger_context: String,
    pub scope: String,
    pub project: String,
    pub register: Vec<String>,
    pub shape: String,
    pub stage: Vec<String>,
    pub tags: Vec<String>,
    pub language_keys: Vec<String>,
    pub technology_keys: Vec<String>,
    #[serde(rename = "match")]
    pub match_info: LessonContextMatch,
}
#[derive(Debug, Serialize)]
pub struct LessonContextFilters {
    pub scopes: Vec<String>,
    pub projects: Vec<String>,
    pub terms: Vec<String>,
    pub shapes: Vec<String>,
    pub stages: Vec<String>,
    pub registers: Vec<String>,
    pub languages: Vec<String>,
    pub technologies: Vec<String>,
    pub limit: u32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonContextResult {
    pub coding_lessons: Vec<LessonContextRecord>,
    pub project_lessons: Vec<LessonContextRecord>,
    #[serde(rename = "match")]
    pub filters: LessonContextFilters,
}
fn normalized(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .collect()
}
pub(crate) fn intersects(values: &[String], context: &BTreeSet<String>) -> bool {
    values.is_empty()
        || values
            .iter()
            .any(|v| context.contains(&v.trim().to_lowercase()))
}
fn context_record(
    row: &sqlx::postgres::PgRow,
    terms: &BTreeSet<String>,
    shapes: &BTreeSet<String>,
    projects: &BTreeSet<String>,
) -> Result<LessonContextRecord, sqlx::Error> {
    let trigger: Option<String> = row.try_get("trigger_context")?;
    let tags: Vec<String> = row.try_get("tags")?;
    let shape: Option<String> = row.try_get("shape")?;
    let project: Option<String> = row.try_get("project")?;
    let trigger_tokens = trigger
        .as_deref()
        .unwrap_or("")
        .replace(',', " ")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<HashSet<_>>();
    let tag_set = tags
        .iter()
        .map(|v| v.trim().to_lowercase())
        .collect::<HashSet<_>>();
    let mut score = 0;
    let mut matched = vec![];
    if trigger_tokens.iter().any(|v| terms.contains(v)) {
        score += 32;
        matched.push("trigger".into());
    }
    if tag_set.iter().any(|v| terms.contains(v)) {
        score += 24;
        matched.push("tag".into());
    }
    if shape
        .as_ref()
        .is_some_and(|v| shapes.contains(&v.trim().to_lowercase()))
    {
        score += 16;
        matched.push("shape".into());
    }
    if project
        .as_ref()
        .is_some_and(|v| projects.contains(&v.trim().to_lowercase()))
    {
        score += 12;
        matched.push("project".into());
    }
    Ok(LessonContextRecord {
        id: row.try_get("id")?,
        family: row.try_get("lesson_key")?,
        title: row.try_get("title")?,
        lesson: row.try_get("lesson")?,
        proof_pattern: row
            .try_get::<Option<String>, _>("proof_pattern")?
            .unwrap_or_default(),
        trigger_context: trigger.unwrap_or_default(),
        scope: row.try_get("scope")?,
        project: project.unwrap_or_default(),
        register: row.try_get("register")?,
        shape: shape.unwrap_or_default(),
        stage: row.try_get("stage")?,
        tags,
        language_keys: row.try_get("language_keys")?,
        technology_keys: row.try_get("technology_keys")?,
        match_info: LessonContextMatch { score, matched },
    })
}
pub async fn lesson_context(
    pool: &PgPool,
    params: LessonContextParams,
) -> Result<LessonContextResult, AppError> {
    if !ROOM_KEY_RE.is_match(params.room.trim()) {
        return Err(AppError::Invalid("room must be a lowercase slug".into()));
    }
    if params.limit > 50 {
        return Err(AppError::Invalid("limit must be from 0 through 50".into()));
    }
    let scopes = if params.room == "house" {
        vec!["house".into()]
    } else {
        vec!["house".into(), params.room.clone()]
    };
    let projects = normalized(&params.projects);
    let shapes = normalized(&params.shapes);
    let terms = normalized(&params.terms);
    let stages = normalized(&params.stages);
    let registers = normalized(&params.registers);
    let languages = normalized(&params.languages);
    let technologies = normalized(&params.technologies);
    let filters = LessonContextFilters {
        scopes: scopes.clone(),
        projects: projects.iter().cloned().collect(),
        terms: terms.iter().cloned().collect(),
        shapes: shapes.iter().cloned().collect(),
        stages: stages.iter().cloned().collect(),
        registers: registers.iter().cloned().collect(),
        languages: languages.iter().cloned().collect(),
        technologies: technologies.iter().cloned().collect(),
        limit: params.limit,
    };
    if params.limit == 0 {
        return Ok(LessonContextResult {
            coding_lessons: vec![],
            project_lessons: vec![],
            filters,
        });
    }
    let rows = sqlx::query("SELECT id,lesson_key,title,lesson,proof_pattern,trigger_context,scope,project,register,shape,stage,tags,language_keys,technology_keys FROM lessons WHERE lesson_key='coding' AND scope=ANY($1)").bind(&scopes).fetch_all(pool).await?;
    let mut coding = vec![];
    for row in &rows {
        let project: Option<String> = row.try_get("project")?;
        let stage: Vec<String> = row.try_get("stage")?;
        let register: Vec<String> = row.try_get("register")?;
        let language: Vec<String> = row.try_get("language_keys")?;
        let technology: Vec<String> = row.try_get("technology_keys")?;
        if project
            .as_ref()
            .is_some_and(|v| !v.trim().is_empty() && !projects.contains(&v.trim().to_lowercase()))
            || !intersects(&stage, &stages)
            || !intersects(&register, &registers)
            || !intersects(&language, &languages)
            || !intersects(&technology, &technologies)
        {
            continue;
        }
        coding.push(context_record(row, &terms, &shapes, &projects)?);
    }
    let mut project_rows = vec![];
    if !projects.is_empty() {
        project_rows = sqlx::query("SELECT id,lesson_key,title,lesson,proof_pattern,trigger_context,scope,project,register,shape,stage,tags,language_keys,technology_keys FROM lessons WHERE lesson_key='project' AND project=ANY($1)").bind(projects.iter().cloned().collect::<Vec<_>>()).fetch_all(pool).await?;
    }
    let mut project_lessons = vec![];
    for row in &project_rows {
        let stage: Vec<String> = row.try_get("stage")?;
        let register: Vec<String> = row.try_get("register")?;
        let language: Vec<String> = row.try_get("language_keys")?;
        let technology: Vec<String> = row.try_get("technology_keys")?;
        if !intersects(&stage, &stages)
            || !intersects(&register, &registers)
            || !intersects(&language, &languages)
            || !intersects(&technology, &technologies)
        {
            continue;
        }
        project_lessons.push(context_record(row, &terms, &shapes, &projects)?);
    }
    let sort = |rows: &mut Vec<LessonContextRecord>| {
        rows.sort_by(|a, b| {
            b.match_info
                .score
                .cmp(&a.match_info.score)
                .then(a.id.cmp(&b.id))
                .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        rows.truncate(params.limit as usize);
    };
    sort(&mut coding);
    sort(&mut project_lessons);
    Ok(LessonContextResult {
        coding_lessons: coding,
        project_lessons,
        filters,
    })
}
