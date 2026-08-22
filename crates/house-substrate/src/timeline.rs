//! Newest-first keyset reads for the Pulse panel: the memory scroller, the
//! single-memory fetch behind its click, and the lesson registry timeline.
//!
//! Contract shaped in guild-hall #183/#185. These are pure reads with no
//! identity parameters: the panel's bearer layer proves reach to the Host,
//! and nothing here acts as anyone. The memory timeline follows recall's
//! active-row discipline exactly (no archived, no superseded, no paper
//! boats); the by-id fetch returns historical rows too, with their authority
//! fields visible, because an explicit id is a provenance read. Lessons are
//! ordered by updated_at - the only time key the registry honestly carries
//! (migrated rows would smuggle an import date as a birthdate) - so a row
//! edited mid-scroll can move past a cursor; that instability is the named
//! v1 ceiling, tolerable for a registry that changes a few rows a day.

use crate::config::{AppError, ROOM_KEY_RE};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

const TIMELINE_EXCERPT_CHARS: i32 = 500;
const LESSON_FAMILIES: [&str; 5] = ["coding", "project", "writing", "design", "audio"];

fn refusal(code: &'static str, message: &'static str) -> AppError {
    AppError::Refusal { code, message }
}

fn validate_limit(limit: u32) -> Result<(), AppError> {
    if (1..=50).contains(&limit) {
        Ok(())
    } else {
        Err(AppError::Invalid(
            "limit must be an integer from 1 through 50".into(),
        ))
    }
}

const fn default_timeline_limit() -> u32 {
    50
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemoryTimelineCursor {
    pub created_at: DateTime<Utc>,
    pub id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemoryTimelineParams {
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub before: Option<MemoryTimelineCursor>,
    #[serde(default = "default_timeline_limit")]
    pub limit: u32,
}

impl MemoryTimelineParams {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(room) = &self.room
            && !ROOM_KEY_RE.is_match(room)
        {
            return Err(AppError::Invalid("room must be a lowercase slug".into()));
        }
        validate_limit(self.limit)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTimelineItem {
    pub id: i64,
    pub room: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub excerpt: String,
    pub source_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MemoryTimelineResult {
    pub ok: bool,
    pub memories: Vec<MemoryTimelineItem>,
}

pub async fn memory_timeline(
    pool: &PgPool,
    request: MemoryTimelineParams,
) -> Result<MemoryTimelineResult, AppError> {
    let (before_created_at, before_id) = match &request.before {
        Some(cursor) => (Some(cursor.created_at), Some(cursor.id)),
        None => (None, None),
    };
    let rows = sqlx::query(
        "SELECT id,room,type,COALESCE(title,'untitled') AS title,
                LEFT(body,$5) AS excerpt,source_path,created_at
         FROM memories
         WHERE archived_at IS NULL AND superseded_by IS NULL AND type<>'paper-boat'
           AND ($1::text IS NULL OR room=$1)
           AND ($2::timestamptz IS NULL OR (created_at,id)<($2,$3))
         ORDER BY created_at DESC,id DESC
         LIMIT $4",
    )
    .bind(&request.room)
    .bind(before_created_at)
    .bind(before_id)
    .bind(i64::from(request.limit))
    .bind(TIMELINE_EXCERPT_CHARS)
    .fetch_all(pool)
    .await?;
    let memories = rows
        .into_iter()
        .map(|row| {
            Ok(MemoryTimelineItem {
                id: row.try_get("id")?,
                room: row.try_get("room")?,
                kind: row.try_get("type")?,
                title: row.try_get("title")?,
                excerpt: row.try_get("excerpt")?,
                source_path: row.try_get("source_path")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(MemoryTimelineResult { ok: true, memories })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemoryReadParams {
    pub id: i64,
}

impl MemoryReadParams {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.id >= 1 {
            Ok(())
        } else {
            Err(AppError::Invalid("id must be a positive integer".into()))
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: i64,
    pub room: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: Option<String>,
    pub date: Option<NaiveDate>,
    pub source_path: String,
    pub threads: Vec<String>,
    /// Authority fields stay visible: a superseded or archived memory reads
    /// as history, never silently as the current record.
    pub superseded_by: Option<i64>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct MemoryReadResult {
    pub ok: bool,
    pub memory: MemoryRecord,
}

pub async fn memory_read(
    pool: &PgPool,
    request: MemoryReadParams,
) -> Result<MemoryReadResult, AppError> {
    let row = sqlx::query(
        "SELECT id,room,type,title,date,source_path,threads,superseded_by,
                archived_at,created_at,updated_at,body
         FROM memories WHERE id=$1",
    )
    .bind(request.id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| refusal("unknown_memory", "no memory has this id"))?;
    Ok(MemoryReadResult {
        ok: true,
        memory: MemoryRecord {
            id: row.try_get("id")?,
            room: row.try_get("room")?,
            kind: row.try_get("type")?,
            title: row.try_get("title")?,
            date: row.try_get("date")?,
            source_path: row.try_get("source_path")?,
            threads: row.try_get("threads")?,
            superseded_by: row.try_get("superseded_by")?,
            archived_at: row.try_get("archived_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            body: row.try_get("body")?,
        },
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonTimelineCursor {
    pub updated_at: DateTime<Utc>,
    pub id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonTimelineParams {
    /// Lesson family (lesson_key): coding, project, writing, design, audio.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub before: Option<LessonTimelineCursor>,
    #[serde(default = "default_timeline_limit")]
    pub limit: u32,
}

impl LessonTimelineParams {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(kind) = &self.kind
            && !LESSON_FAMILIES.contains(&kind.as_str())
        {
            return Err(AppError::Invalid(
                "type must be one of coding, project, writing, design, audio".into(),
            ));
        }
        validate_limit(self.limit)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonTimelineItem {
    pub id: i64,
    pub kind_path: String,
    pub title: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct LessonTimelineResult {
    pub ok: bool,
    pub lessons: Vec<LessonTimelineItem>,
}

pub async fn lesson_timeline(
    pool: &PgPool,
    request: LessonTimelineParams,
) -> Result<LessonTimelineResult, AppError> {
    let (before_updated_at, before_id) = match &request.before {
        Some(cursor) => (Some(cursor.updated_at), Some(cursor.id)),
        None => (None, None),
    };
    // The id tiebreak is not unique across families (0008 migrated each
    // family's serial ids side by side); a cross-family tie on the exact
    // same timestamp AND id could dup or skip one row at a page seam.
    // Tolerated and named rather than widened into a composite cursor.
    let rows = sqlx::query(
        "SELECT id,kind_path,title,updated_at
         FROM lessons
         WHERE ($1::text IS NULL OR lesson_key=$1)
           AND ($2::timestamptz IS NULL OR (updated_at,id)<($2,$3))
         ORDER BY updated_at DESC,id DESC
         LIMIT $4",
    )
    .bind(&request.kind)
    .bind(before_updated_at)
    .bind(before_id)
    .bind(i64::from(request.limit))
    .fetch_all(pool)
    .await?;
    let lessons = rows
        .into_iter()
        .map(|row| {
            Ok(LessonTimelineItem {
                id: row.try_get("id")?,
                kind_path: row.try_get("kind_path")?,
                title: row.try_get("title")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(LessonTimelineResult { ok: true, lessons })
}
