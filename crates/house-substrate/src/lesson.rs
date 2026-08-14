use crate::config::{AppError, ROOM_KEY_RE};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::{BTreeSet, HashSet};

const LESSON_SELECT: &str = "SELECT id,lesson_key,kind_path,scope,project,voice,register,shape,stage,title,lesson,trigger_context,proof_pattern,example_text,example_cmd,writers,tools,negation_of,language_keys,technology_keys,tags,thread_keys,always_on FROM lessons";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LessonFamily {
    Coding,
    Project,
    Writing,
    Design,
    Audio,
}
impl LessonFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Project => "project",
            Self::Writing => "writing",
            Self::Design => "design",
            Self::Audio => "audio",
        }
    }
}

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
    #[serde(default = "default_twelve")]
    pub limit: u32,
}
fn default_twelve() -> u32 {
    12
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
            limit: params.limit,
        },
        lessons,
        taxonomy,
    })
}

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
fn default_eight() -> u32 {
    8
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
fn intersects(values: &[String], context: &BTreeSet<String>) -> bool {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignDocumentQueryParams {
    pub system: String,
    #[serde(default)]
    pub doc_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub include_superseded: bool,
    #[serde(default = "default_twelve")]
    pub limit: u32,
}
#[derive(Debug, Serialize)]
pub struct DesignDocument {
    pub id: i64,
    pub system: String,
    pub doc_type: String,
    pub name: String,
    pub group_name: Option<String>,
    pub values: Value,
    pub body: String,
    pub provenance: Value,
    pub tags: Vec<String>,
    pub superseded_by: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentTaxonomy {
    pub doc_type: String,
    pub count: i64,
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentFilters {
    pub doc_type: Option<String>,
    pub name: Option<String>,
    pub group: Option<String>,
    pub query: Option<String>,
    pub include_superseded: bool,
    pub limit: u32,
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentQueryResult {
    pub ok: bool,
    pub system: String,
    pub filters: DesignDocumentFilters,
    pub documents: Vec<DesignDocument>,
    pub taxonomy: Vec<DesignDocumentTaxonomy>,
}
fn valid_doc_type(value: &str) -> bool {
    matches!(value, "token" | "component" | "contract" | "guideline")
}
pub async fn design_document_query(
    pool: &PgPool,
    params: DesignDocumentQueryParams,
) -> Result<DesignDocumentQueryResult, AppError> {
    if params.system.trim().is_empty() {
        return Err(AppError::Invalid("system is required".into()));
    }
    if params.doc_type.as_ref().is_some_and(|v| !valid_doc_type(v)) {
        return Err(AppError::Invalid(
            "docType must be token, component, contract, or guideline".into(),
        ));
    }
    if !(1..=50).contains(&params.limit) {
        return Err(AppError::Invalid(
            "limit must be an integer from 1 through 50".into(),
        ));
    }
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT id,system,doc_type,name,group_name,\"values\",body,provenance,tags,superseded_by,created_at,updated_at FROM design_documents WHERE system=",
    );
    qb.push_bind(&params.system);
    if let Some(v) = params.doc_type.as_ref() {
        qb.push(" AND doc_type=").push_bind(v);
    }
    if let Some(v) = params.name.as_ref() {
        qb.push(" AND name=").push_bind(v);
    }
    if let Some(v) = params.group.as_ref() {
        qb.push(" AND group_name=").push_bind(v);
    }
    if !params.include_superseded {
        qb.push(" AND superseded_by IS NULL");
    }
    if let Some(v) = params.query.as_ref().filter(|v| !v.is_empty()) {
        qb.push(" AND search_tsv @@ plainto_tsquery('portuguese',")
            .push_bind(v)
            .push(")");
    }
    let rank = params.query.clone().unwrap_or_default();
    qb.push(" ORDER BY CASE WHEN ")
        .push_bind(rank.clone())
        .push("<>'' THEN ts_rank(search_tsv,plainto_tsquery('portuguese',")
        .push_bind(rank)
        .push(")) ELSE 0 END DESC,updated_at DESC,id LIMIT ")
        .push_bind(i64::from(params.limit));
    let documents = qb
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|r| {
            Ok(DesignDocument {
                id: r.try_get("id")?,
                system: r.try_get("system")?,
                doc_type: r.try_get("doc_type")?,
                name: r.try_get("name")?,
                group_name: r.try_get("group_name")?,
                values: r.try_get("values")?,
                body: r.try_get("body")?,
                provenance: r.try_get("provenance")?,
                tags: r.try_get("tags")?,
                superseded_by: r.try_get("superseded_by")?,
                created_at: r.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                updated_at: r.try_get::<DateTime<Utc>, _>("updated_at")?.to_rfc3339(),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let mut tq = QueryBuilder::<Postgres>::new(
        "SELECT doc_type,COUNT(*) AS count FROM design_documents WHERE system=",
    );
    tq.push_bind(&params.system);
    if let Some(v) = params.doc_type.as_ref() {
        tq.push(" AND doc_type=").push_bind(v);
    }
    if !params.include_superseded {
        tq.push(" AND superseded_by IS NULL");
    }
    tq.push(" GROUP BY doc_type ORDER BY count DESC,doc_type");
    let taxonomy = tq
        .build()
        .fetch_all(pool)
        .await?
        .iter()
        .map(|r| {
            Ok(DesignDocumentTaxonomy {
                doc_type: r.try_get("doc_type")?,
                count: r.try_get("count")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(DesignDocumentQueryResult {
        ok: true,
        system: params.system,
        filters: DesignDocumentFilters {
            doc_type: params.doc_type,
            name: params.name,
            group: params.group,
            query: params.query,
            include_superseded: params.include_superseded,
            limit: params.limit,
        },
        documents,
        taxonomy,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DesignDocumentWriteParams {
    pub system: String,
    pub doc_type: String,
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default = "empty_object")]
    pub values: Value,
    #[serde(default)]
    pub body: String,
    #[serde(default = "empty_object")]
    pub provenance: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub supersedes: Option<i64>,
    #[serde(default)]
    pub allow_identity_change: bool,
}
fn empty_object() -> Value {
    serde_json::json!({})
}
fn parse_i64_value<E: serde::de::Error>(value: Value) -> Result<i64, E> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| E::custom("ID must fit PostgreSQL BIGINT")),
        Value::String(text) => text
            .parse::<i64>()
            .map_err(|_| E::custom("ID must be a decimal PostgreSQL BIGINT")),
        _ => Err(E::custom("ID must be a decimal PostgreSQL BIGINT")),
    }
}
fn deserialize_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    parse_i64_value(Value::deserialize(deserializer)?)
}
fn deserialize_optional_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        Ok(None)
    } else {
        parse_i64_value(value).map(Some)
    }
}
#[derive(Debug, Serialize)]
pub struct DesignDocumentWriteReceipt {
    pub ok: bool,
    pub system: String,
    pub doc_type: String,
    pub name: String,
    pub superseded: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
fn design_refusal(
    p: &DesignDocumentWriteParams,
    error: impl Into<String>,
) -> DesignDocumentWriteReceipt {
    DesignDocumentWriteReceipt {
        ok: false,
        system: p.system.clone(),
        doc_type: p.doc_type.clone(),
        name: p.name.clone(),
        superseded: vec![],
        id: None,
        error: Some(error.into()),
    }
}
pub async fn design_document_write(
    pool: &PgPool,
    p: DesignDocumentWriteParams,
) -> Result<DesignDocumentWriteReceipt, AppError> {
    if p.system.trim().is_empty() {
        return Ok(design_refusal(&p, "system is required"));
    }
    if !valid_doc_type(&p.doc_type) {
        return Ok(design_refusal(
            &p,
            "doc_type must be one of: token, component, contract, guideline",
        ));
    }
    if p.name.trim().is_empty() {
        return Ok(design_refusal(&p, "name is required"));
    }
    if !p.values.is_object() {
        return Ok(design_refusal(&p, "values must be a JSON object"));
    }
    if !p.provenance.is_object() {
        return Ok(design_refusal(&p, "provenance must be a JSON object"));
    }
    if p.supersedes.is_some_and(|id| id <= 0) {
        return Ok(design_refusal(&p, "supersedes must be a positive integer"));
    }
    let system = p.system.trim().to_string();
    let name = p.name.trim().to_string();
    let mut tx = pool.begin().await?;
    if let Some(old) = p.supersedes {
        let row = sqlx::query("SELECT system,doc_type,name,superseded_by FROM design_documents WHERE id=$1 FOR UPDATE")
            .bind(old).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return Ok(design_refusal(&p, "superseded document not found"));
        };
        let superseded: Option<i64> = row.try_get("superseded_by")?;
        if superseded.is_some() {
            return Ok(design_refusal(
                &p,
                "superseded document is already superseded",
            ));
        }
        let previous_system: String = row.try_get("system")?;
        let previous_type: String = row.try_get("doc_type")?;
        let previous_name: String = row.try_get("name")?;
        let same =
            previous_system == system && previous_type == p.doc_type && previous_name == name;
        if !same && !p.allow_identity_change {
            return Ok(design_refusal(
                &p,
                "superseded document identity differs; pass --allow-identity-change",
            ));
        }
    }
    let id: i64 = sqlx::query_scalar("INSERT INTO design_documents(system,doc_type,name,group_name,\"values\",body,provenance,tags) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(&system).bind(&p.doc_type).bind(&name).bind(&p.group).bind(&p.values)
        .bind(&p.body).bind(&p.provenance).bind(&p.tags).fetch_one(&mut *tx).await?;
    if let Some(old) = p.supersedes {
        let changed = sqlx::query(
            "UPDATE design_documents SET superseded_by=$1 WHERE id=$2 AND superseded_by IS NULL",
        )
        .bind(id)
        .bind(old)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed != 1 {
            return Ok(design_refusal(
                &p,
                "supersession affected an unexpected number of rows",
            ));
        }
    }
    tx.commit().await?;
    Ok(DesignDocumentWriteReceipt {
        ok: true,
        system,
        doc_type: p.doc_type,
        name,
        superseded: p.supersedes.into_iter().collect(),
        id: Some(id),
        error: None,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonDeleteParams {
    pub kind: String,
    #[serde(deserialize_with = "deserialize_i64")]
    pub id: i64,
    pub expected_title: String,
}
#[derive(Debug, Serialize)]
pub struct LessonMutationReceipt {
    pub ok: bool,
    pub kind: String,
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<bool>,
    #[serde(rename = "alwaysOn", skip_serializing_if = "Option::is_none")]
    pub always_on: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
fn lesson_key(kind: &str) -> Option<&'static str> {
    match kind {
        "coding-lesson" => Some("coding"),
        "project-lesson" => Some("project"),
        "writing-lesson" => Some("writing"),
        "design-lesson" => Some("design"),
        _ => None,
    }
}
fn mutation_refusal(
    p: &LessonDeleteParams,
    error: impl Into<String>,
    actual: Option<String>,
    update: bool,
) -> LessonMutationReceipt {
    LessonMutationReceipt {
        ok: false,
        kind: p.kind.clone(),
        id: p.id,
        title: None,
        actual_title: actual,
        updated: update.then_some(false),
        deleted: (!update).then_some(false),
        always_on: None,
        project: None,
        error: Some(error.into()),
    }
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
            false,
        ));
    };
    if p.id <= 0 {
        return Ok(mutation_refusal(
            &p,
            "id must be a positive integer",
            None,
            false,
        ));
    }
    if p.expected_title.is_empty() {
        return Ok(mutation_refusal(
            &p,
            "expected_title is required",
            None,
            false,
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
        return Ok(mutation_refusal(&p, "lesson not found", None, false));
    };
    if actual != p.expected_title {
        return Ok(mutation_refusal(&p, "title mismatch", Some(actual), false));
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
            false,
        ));
    }
    tx.commit().await?;
    Ok(LessonMutationReceipt {
        ok: true,
        kind: p.kind,
        id: p.id,
        title: Some(p.expected_title),
        actual_title: None,
        updated: None,
        deleted: Some(true),
        always_on: None,
        project: None,
        error: None,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonUpdateParams {
    pub kind: String,
    #[serde(deserialize_with = "deserialize_i64")]
    pub id: i64,
    pub expected_title: String,
    pub patch: Value,
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
            true,
        ));
    };
    if p.id <= 0 {
        return Ok(mutation_refusal(
            &delete,
            "id must be a positive integer",
            None,
            true,
        ));
    }
    if p.expected_title.is_empty() {
        return Ok(mutation_refusal(
            &delete,
            "expected_title is required",
            None,
            true,
        ));
    }
    let Some(fields) = p.patch.as_object() else {
        return Ok(mutation_refusal(
            &delete,
            "patch must be an object",
            None,
            true,
        ));
    };
    if fields.is_empty() {
        return Ok(mutation_refusal(
            &delete,
            "at least one update field is required",
            None,
            true,
        ));
    }
    if fields.contains_key("project") && fields.contains_key("clearProject") {
        return Ok(mutation_refusal(
            &delete,
            "project and clearProject are mutually exclusive",
            None,
            true,
        ));
    }
    if fields.contains_key("clearProject") && !matches!(key, "coding" | "project") {
        return Ok(mutation_refusal(
            &delete,
            format!("clearProject is not allowed for {}", p.kind),
            None,
            true,
        ));
    }
    if let Some(clear) = fields.get("clearProject")
        && clear.as_bool() != Some(true)
    {
        return Ok(mutation_refusal(
            &delete,
            "clearProject must be true when provided",
            None,
            true,
        ));
    }
    let common = [
        "title",
        "body",
        "shape",
        "triggerContext",
        "tags",
        "threadKeys",
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
        "design" => &["voice", "register", "proofPattern", "exampleText"],
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
            true,
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
        return Ok(mutation_refusal(&delete, "lesson not found", None, true));
    };
    if actual != p.expected_title {
        return Ok(mutation_refusal(
            &delete,
            "title mismatch",
            Some(actual),
            true,
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
            value => value,
        };
        separated.push(format!("{column} = "));
        match field.as_str() {
            "tags" | "threadKeys" | "languageKeys" | "technologyKeys" | "register" | "writers" => {
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
    drop(separated);
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
            true,
        ));
    };
    tx.commit().await?;
    Ok(LessonMutationReceipt {
        ok: true,
        kind: p.kind,
        id: p.id,
        title: Some(title),
        actual_title: None,
        updated: Some(true),
        deleted: None,
        always_on: Some(always_on),
        project: Some(project),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lesson_query_preserves_typed_filters_and_bounds() {
        let params: LessonQueryParams = serde_json::from_value(serde_json::json!({
            "room": "kintsu",
            "type": "audio",
            "stage": "mix",
            "languageKeys": [],
            "technologyKeys": [],
            "limit": 12
        }))
        .unwrap();
        assert_eq!(params.family, LessonFamily::Audio);
        assert_eq!(params.stage.as_deref(), Some("mix"));
        assert!(params.validate().is_ok());
        let invalid: LessonQueryParams = serde_json::from_value(serde_json::json!({
            "room": "kintsu", "type": "project", "limit": 12
        }))
        .unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn context_eligibility_requires_declared_axis_overlap() {
        let rust = BTreeSet::from([String::from("rust")]);
        assert!(intersects(&[], &rust));
        assert!(intersects(&[String::from("rust")], &rust));
        assert!(!intersects(&[String::from("python")], &rust));
        assert!(!intersects(&[String::from("rust")], &BTreeSet::new()));
    }

    #[test]
    fn bigint_guards_accept_decimal_strings_without_javascript_precision_loss() {
        let delete: LessonDeleteParams = serde_json::from_value(serde_json::json!({
            "kind": "coding-lesson",
            "id": "9223372036854775807",
            "expectedTitle": "Exact"
        }))
        .unwrap();
        assert_eq!(delete.id, i64::MAX);
        let design: DesignDocumentWriteParams = serde_json::from_value(serde_json::json!({
            "system": "solarisael",
            "docType": "token",
            "name": "color.accent",
            "supersedes": "42"
        }))
        .unwrap();
        assert_eq!(design.supersedes, Some(42));
    }

    #[test]
    fn mutation_receipts_keep_exact_family_identity() {
        let receipt = LessonMutationReceipt {
            ok: true,
            kind: "design-lesson".into(),
            id: 9,
            title: Some("Keyboard floor".into()),
            actual_title: None,
            updated: Some(true),
            deleted: None,
            always_on: None,
            project: None,
            error: None,
        };
        assert_eq!(
            serde_json::to_value(receipt).unwrap(),
            serde_json::json!({
                "ok": true,
                "kind": "design-lesson",
                "id": 9,
                "title": "Keyboard floor",
                "updated": true
            })
        );
    }
}
