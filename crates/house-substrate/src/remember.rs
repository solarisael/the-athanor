use crate::backup;
use crate::config::{
    AppError, Config, EmbeddingMode, HTTP_CLIENT, PATH_DATE_RE, ROOM_KEY_RE, STITCHED_PATH_DATE_RE,
};
use chrono::{Local, NaiveDate};
use house_core::lesson_triggers::LessonTriggerSpec;
use reqwest::Client;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{
    collections::{BTreeSet, HashSet},
    time::Duration,
};

#[derive(Debug, Deserialize)]
pub struct ThreadContinuation {
    pub thread: String,
    #[serde(alias = "previousMemoryId")]
    pub previous_memory_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct RememberRequest {
    pub room: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub lesson: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_memory_path: Option<String>,
    #[serde(default)]
    pub threads: Vec<String>,
    #[serde(default)]
    pub continues: Vec<ThreadContinuation>,
    #[serde(default)]
    pub supersedes: Vec<i64>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub register: Vec<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default, alias = "proofPattern")]
    pub proof_pattern: Option<String>,
    #[serde(default, alias = "triggerContext")]
    pub trigger_context: Option<String>,
    #[serde(default, alias = "exampleText")]
    pub example_text: Option<String>,
    #[serde(default, alias = "languageKeys")]
    pub language_keys: Vec<String>,
    #[serde(default, alias = "technologyKeys")]
    pub technology_keys: Vec<String>,
    #[serde(default, alias = "threadKeys")]
    pub thread_keys: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub condition: Vec<String>,
    #[serde(default, alias = "astCondition")]
    pub ast_condition: Vec<String>,
    #[serde(default, alias = "triggerScope")]
    pub trigger_scope: Vec<String>,
    #[serde(default, alias = "interruptMode")]
    pub interrupt_mode: Option<String>,
    #[serde(default, alias = "repeatCooldownSecs")]
    pub repeat_cooldown_secs: Option<i32>,
    #[serde(default = "default_backup")]
    pub backup: bool,
}
pub(crate) fn default_backup() -> bool {
    true
}

#[derive(Debug)]
pub struct RememberReceipt {
    pub memory_id: i64,
    pub lesson_id: i64,
    pub kind: String,
    pub room: String,
    pub source_path: String,
    pub durable: bool,
    pub authority: &'static str,
    pub warnings: Vec<String>,
}

impl Serialize for RememberReceipt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.memory_id != 0 {
            let mut out = serializer.serialize_struct("RememberReceipt", 6)?;
            out.serialize_field("memory_id", &self.memory_id)?;
            out.serialize_field("room", &self.room)?;
            out.serialize_field("source_path", &self.source_path)?;
            out.serialize_field("durable", &self.durable)?;
            out.serialize_field("authority", &self.authority)?;
            out.serialize_field("warnings", &self.warnings)?;
            out.end()
        } else {
            let mut out = serializer.serialize_struct("RememberReceipt", 5)?;
            out.serialize_field("lesson_id", &self.lesson_id)?;
            out.serialize_field("kind", &self.kind)?;
            out.serialize_field("durable", &self.durable)?;
            out.serialize_field("authority", &self.authority)?;
            out.serialize_field("warnings", &self.warnings)?;
            out.end()
        }
    }
}

impl RememberRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if !ROOM_KEY_RE.is_match(&self.room) || (self.room == "house" && self.kind != "memory") {
            return Err(AppError::Invalid(
                "room must be a lowercase slug; house accepts only memory writes".into(),
            ));
        }
        let lessons = [
            "coding-lesson",
            "project-lesson",
            "writing-lesson",
            "audio-lesson",
            "design-lesson",
        ];
        let text = self.lesson.as_deref().unwrap_or(&self.body);
        if self.title.trim().is_empty() {
            return Err(AppError::Invalid("title must not be empty".into()));
        }
        if text.trim().is_empty() {
            return Err(AppError::Invalid("body/lesson must not be empty".into()));
        }
        if self.kind == "memory" {
            if self.lesson.is_some()
                || self.source_memory_path.is_some()
                || self.shape.is_some()
                || self.voice.is_some()
                || !self.register.is_empty()
                || self.scope.is_some()
                || self.project.is_some()
                || self.proof_pattern.is_some()
                || self.trigger_context.is_some()
                || self.example_text.is_some()
                || !self.language_keys.is_empty()
                || !self.technology_keys.is_empty()
                || !self.thread_keys.is_empty()
                || !self.tags.is_empty()
                || !self.trigger_spec().is_empty()
            {
                return Err(AppError::Invalid(
                    "lesson-only fields are not valid for memory".into(),
                ));
            }
            if self
                .source_path
                .as_ref()
                .is_some_and(|p| p.trim().is_empty())
            {
                return Err(AppError::Invalid("source_path must not be empty".into()));
            }
            if self.supersedes.iter().any(|id| *id <= 0) {
                return Err(AppError::Invalid(
                    "supersedes must contain positive IDs".into(),
                ));
            }
            let threads = normalize_threads(&self.threads);
            let mut predecessors = HashSet::new();
            for continuation in &self.continues {
                let thread = continuation.thread.trim();
                if thread.is_empty() || continuation.previous_memory_id <= 0 {
                    return Err(AppError::Invalid(
                        "continues entries require a thread and positive previousMemoryId".into(),
                    ));
                }
                if !threads.iter().any(|candidate| candidate == thread) {
                    return Err(AppError::Invalid(
                        "continues thread must also be present in threads".into(),
                    ));
                }
                if !predecessors.insert(thread) {
                    return Err(AppError::Invalid(
                        "continues may name only one predecessor per thread".into(),
                    ));
                }
            }
        } else if lessons.contains(&self.kind.as_str()) {
            if !self.threads.is_empty()
                || !self.continues.is_empty()
                || !self.supersedes.is_empty()
                || self.source_path.is_some()
            {
                return Err(AppError::Invalid(
                    "threads/continues/supersedes/source_path are memory-only".into(),
                ));
            }
            if self
                .source_memory_path
                .as_ref()
                .is_some_and(|p| p.trim().is_empty())
            {
                return Err(AppError::Invalid(
                    "source_memory_path must not be empty".into(),
                ));
            }
            let unsupported = match self.kind.as_str() {
                "coding-lesson" => !self.register.is_empty() || self.example_text.is_some(),
                "project-lesson" => {
                    self.voice.is_some()
                        || !self.register.is_empty()
                        || self.scope.is_some()
                        || self.example_text.is_some()
                }
                "writing-lesson" => {
                    self.scope.is_some()
                        || self.project.is_some()
                        || self.proof_pattern.is_some()
                        || !self.language_keys.is_empty()
                        || !self.technology_keys.is_empty()
                }
                "audio-lesson" => {
                    self.voice.is_some()
                        || !self.register.is_empty()
                        || self.scope.is_some()
                        || self.project.is_some()
                        || self.proof_pattern.is_some()
                        || self.example_text.is_some()
                        || !self.language_keys.is_empty()
                        || !self.technology_keys.is_empty()
                }
                "design-lesson" => {
                    self.scope.is_some()
                        || self.project.is_some()
                        || !self.language_keys.is_empty()
                        || !self.technology_keys.is_empty()
                }
                _ => true,
            };
            if unsupported {
                return Err(AppError::Invalid(
                    "lesson fields are unsupported by this lesson key".into(),
                ));
            }
            if self
                .language_keys
                .iter()
                .chain(&self.technology_keys)
                .chain(&self.thread_keys)
                .any(|key| {
                    key.is_empty()
                        || key.starts_with('-')
                        || key.ends_with('-')
                        || key.contains("--")
                        || !key.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                        })
                })
            {
                return Err(AppError::Invalid(
                    "eligibility keys must be lowercase hyphenated slugs".into(),
                ));
            }
            if self.kind == "project-lesson"
                && self.project.as_deref().unwrap_or("").trim().is_empty()
            {
                return Err(AppError::Invalid(
                    "project is required for project lessons".into(),
                ));
            }
            self.trigger_spec().validate().map_err(AppError::Invalid)?;
        } else {
            return Err(AppError::Invalid("unsupported remember kind".into()));
        }
        Ok(())
    }
    pub(crate) fn source_path(&self) -> String {
        self.source_path.clone().unwrap_or_else(|| {
            let identity = json!({
                "room": self.room,
                "title": self.title,
                "body": self.body,
                "threads": normalize_threads(&self.threads),
                "continues": self.continues.iter().map(|item| {
                    (&item.thread, item.previous_memory_id)
                }).collect::<Vec<_>>(),
                "supersedes": self.supersedes,
            });
            format!(
                "db-only/{}/{:x}",
                self.room,
                Sha256::digest(identity.to_string())
            )
        })
    }
    fn lesson_body(&self) -> &str {
        self.lesson.as_deref().unwrap_or(&self.body)
    }
    /// The lesson's trigger columns as house-core sees them. Owned rather than
    /// borrowed: the spec is validated and then bound, never held across the
    /// insert.
    pub(crate) fn trigger_spec(&self) -> LessonTriggerSpec {
        LessonTriggerSpec {
            condition: self.condition.clone(),
            ast_condition: self.ast_condition.clone(),
            trigger_scope: self.trigger_scope.clone(),
            interrupt_mode: self.interrupt_mode.clone(),
            repeat_cooldown_secs: self.repeat_cooldown_secs,
            ..Default::default()
        }
    }
}

pub async fn remember(
    pool: &PgPool,
    cfg: &Config,
    req: RememberRequest,
) -> Result<RememberReceipt, AppError> {
    req.validate()?;
    if req.kind != "memory" {
        return remember_lesson(pool, cfg, &req).await;
    }
    let source_path = req.source_path();
    let mut prepared = prepare_memory_write(
        cfg,
        &source_path,
        &req.body,
        &req.threads,
        Local::now().date_naive(),
    )
    .await?;
    let mut warnings = std::mem::take(&mut prepared.warnings);
    let meta = serde_json::json!({
        "origin": "direct-db-write",
        "recorded_at": chrono::Utc::now().to_rfc3339(),
    });
    let mut tx = pool.begin().await?;
    let (memory_id, _) = write_memory_tx(
        &mut tx,
        &req.room,
        "memory",
        &req.title,
        &source_path,
        &req.body,
        &req.supersedes,
        meta,
        &prepared,
    )
    .await?;
    write_continuations_tx(&mut tx, &req.room, memory_id, &req.continues).await?;
    tx.commit().await?;
    if req.backup
        && let Err(error) = backup::run_post_write(pool, &cfg.database_url).await
    {
        warnings.push(format!("backup failed: {error}"));
    }
    Ok(RememberReceipt {
        memory_id,
        lesson_id: 0,
        kind: "memory".into(),
        room: req.room,
        source_path,
        durable: true,
        authority: "postgres",
        warnings,
    })
}

pub(crate) struct PreparedMemoryWrite {
    primary_date: NaiveDate,
    dates: Vec<NaiveDate>,
    threads: Vec<String>,
    chunks: Vec<(String, usize, usize, Option<String>)>,
    vectors: Option<Vec<Vec<f32>>>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) async fn prepare_memory_write(
    cfg: &Config,
    source_path: &str,
    body: &str,
    threads: &[String],
    primary_date: NaiveDate,
) -> Result<PreparedMemoryWrite, AppError> {
    let threads = normalize_threads(threads);
    let dates = derive_dates(source_path, primary_date);
    let chunks = chunk_body(body);
    let mut warnings = Vec::new();
    let vectors = match cfg.embedding_mode {
        EmbeddingMode::Disabled => {
            warnings
                .push("semantic embeddings disabled in production; lexical chunks retained".into());
            None
        }
        EmbeddingMode::DisabledForTest => {
            warnings.push(
                "semantic embeddings disabled for isolated test; lexical chunks retained".into(),
            );
            None
        }
        EmbeddingMode::Required => {
            let url = cfg
                .embed_url
                .as_deref()
                .ok_or_else(|| AppError::Embedding("embedding endpoint is required".into()))?;
            let vectors = embed(
                &HTTP_CLIENT,
                url,
                &cfg.embed_model,
                &chunks,
                cfg.embed_dimension,
            )
            .await?;
            if vectors.len() != chunks.len() {
                return Err(AppError::Embedding("embedding count mismatch".into()));
            }
            Some(vectors)
        }
    };
    Ok(PreparedMemoryWrite {
        primary_date,
        dates,
        threads,
        chunks,
        vectors,
        warnings,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_memory_tx(
    tx: &mut Transaction<'_, Postgres>,
    room: &str,
    memory_type: &str,
    title: &str,
    source_path: &str,
    body: &str,
    supersedes: &[i64],
    meta: Value,
    prepared: &PreparedMemoryWrite,
) -> Result<(i64, bool), AppError> {
    let (memory_id, inserted) = if memory_type == "paper-boat" {
        let inserted_id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO memories
             (room,type,date,dates,title,source_path,body,threads,meta)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (room,source_path) DO NOTHING
             RETURNING id",
        )
        .bind(room)
        .bind(memory_type)
        .bind(prepared.primary_date)
        .bind(&prepared.dates)
        .bind(title)
        .bind(source_path)
        .bind(body)
        .bind(&prepared.threads)
        .bind(&meta)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(memory_id) = inserted_id {
            (memory_id, true)
        } else {
            let existing: Option<(i64, String, String)> = sqlx::query_as(
                "SELECT id,type,body FROM memories
                 WHERE room=$1 AND source_path=$2
                 FOR KEY SHARE",
            )
            .bind(room)
            .bind(source_path)
            .fetch_optional(&mut **tx)
            .await?;
            match existing {
                Some((memory_id, existing_type, existing_body))
                    if existing_type == memory_type && existing_body == body =>
                {
                    return Ok((memory_id, false));
                }
                _ => {
                    return Err(AppError::Invalid(
                        "paper boat source identity conflicts with a different record".into(),
                    ));
                }
            }
        }
    } else {
        sqlx::query_as(
            "INSERT INTO memories
             (room,type,date,dates,title,source_path,body,threads,meta)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (room,source_path) DO UPDATE
             SET type=EXCLUDED.type,date=EXCLUDED.date,dates=EXCLUDED.dates,title=EXCLUDED.title,
                 body=EXCLUDED.body,threads=EXCLUDED.threads,meta=EXCLUDED.meta
             RETURNING id,(xmax=0) AS inserted",
        )
        .bind(room)
        .bind(memory_type)
        .bind(prepared.primary_date)
        .bind(&prepared.dates)
        .bind(title)
        .bind(source_path)
        .bind(body)
        .bind(&prepared.threads)
        .bind(&meta)
        .fetch_one(&mut **tx)
        .await?
    };
    for thread_key in &prepared.threads {
        let thread_id: i64 = sqlx::query_scalar(
            "INSERT INTO threads (room,thread_key) VALUES ($1,$2)
             ON CONFLICT (room,thread_key) DO UPDATE SET thread_key=EXCLUDED.thread_key
             RETURNING id",
        )
        .bind(room)
        .bind(thread_key)
        .fetch_one(&mut **tx)
        .await?;
        let event_id: i64 = sqlx::query_scalar(
            "INSERT INTO thread_events (thread_id,memory_id) VALUES ($1,$2)
             ON CONFLICT (thread_id,memory_id) DO UPDATE SET memory_id=EXCLUDED.memory_id
             RETURNING id",
        )
        .bind(thread_id)
        .bind(memory_id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO memory_thread_refs (event_id)
             SELECT $1 WHERE NOT EXISTS (
                 SELECT 1 FROM memory_thread_refs WHERE event_id=$1
             )",
        )
        .bind(event_id)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "DELETE FROM thread_events e USING threads t
         WHERE e.thread_id=t.id AND e.memory_id=$1 AND t.room=$2
           AND NOT (t.thread_key = ANY($3::text[]))",
    )
    .bind(memory_id)
    .bind(room)
    .bind(&prepared.threads)
    .execute(&mut **tx)
    .await?;
    for old_id in supersedes.iter().copied().collect::<BTreeSet<_>>() {
        sqlx::query("UPDATE memories SET superseded_by=$1 WHERE id=$2 AND id<>$1")
            .bind(memory_id)
            .bind(old_id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM memory_chunks WHERE memory_id=$1")
        .bind(memory_id)
        .execute(&mut **tx)
        .await?;
    for (index, (text, start, end, heading)) in prepared.chunks.iter().enumerate() {
        let vector_text = prepared.vectors.as_ref().map(|vectors| {
            format!(
                "[{}]",
                vectors[index]
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        });
        sqlx::query(
            "INSERT INTO memory_chunks
             (memory_id,chunk_index,heading_path,body,char_start,char_end,token_estimate,
              body_embedding,embedded_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8::vector,
                     CASE WHEN $8 IS NULL THEN NULL ELSE NOW() END)",
        )
        .bind(memory_id)
        .bind(
            i32::try_from(index)
                .map_err(|_| AppError::Invalid("memory has too many semantic chunks".into()))?,
        )
        .bind(heading)
        .bind(text)
        .bind(
            i32::try_from(*start).map_err(|_| {
                AppError::Invalid("memory chunk range exceeds database bounds".into())
            })?,
        )
        .bind(
            i32::try_from(*end).map_err(|_| {
                AppError::Invalid("memory chunk range exceeds database bounds".into())
            })?,
        )
        .bind(token_estimate(text))
        .bind(vector_text)
        .execute(&mut **tx)
        .await?;
    }
    Ok((memory_id, inserted))
}

async fn write_continuations_tx(
    tx: &mut Transaction<'_, Postgres>,
    room: &str,
    memory_id: i64,
    continuations: &[ThreadContinuation],
) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for continuation in continuations {
        let thread_key = continuation.thread.trim();
        if !seen.insert(thread_key) {
            continue;
        }
        if continuation.previous_memory_id == memory_id {
            return Err(AppError::Invalid("a memory cannot continue itself".into()));
        }
        let events = sqlx::query(
            "SELECT current_event.thread_id,
                    current_event.id AS next_event_id,
                    previous_event.id AS previous_event_id
             FROM threads t
             JOIN thread_events current_event
               ON current_event.thread_id=t.id AND current_event.memory_id=$3
             JOIN thread_events previous_event
               ON previous_event.thread_id=t.id AND previous_event.memory_id=$4
             JOIN memories previous_memory ON previous_memory.id=previous_event.memory_id
             WHERE t.room=$1 AND t.thread_key=$2 AND previous_memory.room=$1
             FOR KEY SHARE OF t,current_event,previous_event,previous_memory",
        )
        .bind(room)
        .bind(thread_key)
        .bind(memory_id)
        .bind(continuation.previous_memory_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(events) = events else {
            return Err(AppError::Invalid(format!(
                "previous memory {} must share room {room} and thread {thread_key}",
                continuation.previous_memory_id
            )));
        };
        let thread_id: i64 = sqlx::Row::try_get(&events, "thread_id")?;
        let next_event_id: i64 = sqlx::Row::try_get(&events, "next_event_id")?;
        let previous_event_id: i64 = sqlx::Row::try_get(&events, "previous_event_id")?;
        sqlx::query(
            "INSERT INTO thread_event_links (thread_id,previous_event_id,next_event_id)
             VALUES ($1,$2,$3)
             ON CONFLICT (thread_id,next_event_id) DO UPDATE
             SET previous_event_id=EXCLUDED.previous_event_id",
        )
        .bind(thread_id)
        .bind(previous_event_id)
        .bind(next_event_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_coding_lesson_tx(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
    project: Option<&str>,
    voice: Option<&str>,
    shape: Option<&str>,
    title: &str,
    lesson: &str,
    trigger_context: Option<&str>,
    proof_pattern: Option<&str>,
    language_keys: &[String],
    technology_keys: &[String],
    thread_keys: &[String],
    tags: &[String],
    source_memory_path: Option<&str>,
    triggers: &LessonTriggerSpec,
    meta: Value,
) -> Result<i64, AppError> {
    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,project,voice,shape,title,lesson,trigger_context,proof_pattern,
          language_keys,technology_keys,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         VALUES ('coding',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
         ON CONFLICT (scope,project,title) WHERE lesson_key='coding' DO UPDATE
         SET project=EXCLUDED.project,voice=EXCLUDED.voice,shape=EXCLUDED.shape,
             lesson=EXCLUDED.lesson,trigger_context=EXCLUDED.trigger_context,
             proof_pattern=EXCLUDED.proof_pattern,
             language_keys=EXCLUDED.language_keys,technology_keys=EXCLUDED.technology_keys,
             thread_keys=EXCLUDED.thread_keys,tags=EXCLUDED.tags,
             source_memory_path=EXCLUDED.source_memory_path,meta=EXCLUDED.meta,
             condition=EXCLUDED.condition,ast_condition=EXCLUDED.ast_condition,
             trigger_scope=EXCLUDED.trigger_scope,interrupt_mode=EXCLUDED.interrupt_mode,
             repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs
         RETURNING id",
    )
    .bind(scope)
    .bind(project)
    .bind(voice)
    .bind(shape)
    .bind(title)
    .bind(lesson)
    .bind(trigger_context)
    .bind(proof_pattern)
    .bind(language_keys)
    .bind(technology_keys)
    .bind(thread_keys)
    .bind(tags)
    .bind(source_memory_path)
    .bind(meta)
    .bind(triggers.condition.as_slice())
    .bind(triggers.ast_condition.as_slice())
    .bind(triggers.trigger_scope.as_slice())
    .bind(triggers.interrupt_mode.as_deref())
    .bind(triggers.repeat_cooldown_secs)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_project_lesson_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: &str,
    title: &str,
    lesson: &str,
    trigger_context: Option<&str>,
    proof_pattern: Option<&str>,
    language_keys: &[String],
    technology_keys: &[String],
    thread_keys: &[String],
    tags: &[String],
    source_memory_path: Option<&str>,
    triggers: &LessonTriggerSpec,
    meta: Value,
) -> Result<i64, AppError> {
    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,project,title,lesson,trigger_context,proof_pattern,
          language_keys,technology_keys,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         VALUES ('project','project',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         ON CONFLICT (project,title) WHERE lesson_key='project' DO UPDATE
         SET lesson=EXCLUDED.lesson,trigger_context=EXCLUDED.trigger_context,
             proof_pattern=EXCLUDED.proof_pattern,
             language_keys=EXCLUDED.language_keys,technology_keys=EXCLUDED.technology_keys,
             thread_keys=EXCLUDED.thread_keys,tags=EXCLUDED.tags,
             source_memory_path=EXCLUDED.source_memory_path,meta=EXCLUDED.meta,
             condition=EXCLUDED.condition,ast_condition=EXCLUDED.ast_condition,
             trigger_scope=EXCLUDED.trigger_scope,interrupt_mode=EXCLUDED.interrupt_mode,
             repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs
         RETURNING id",
    )
    .bind(project)
    .bind(title)
    .bind(lesson)
    .bind(trigger_context)
    .bind(proof_pattern)
    .bind(language_keys)
    .bind(technology_keys)
    .bind(thread_keys)
    .bind(tags)
    .bind(source_memory_path)
    .bind(meta)
    .bind(triggers.condition.as_slice())
    .bind(triggers.ast_condition.as_slice())
    .bind(triggers.trigger_scope.as_slice())
    .bind(triggers.interrupt_mode.as_deref())
    .bind(triggers.repeat_cooldown_secs)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn remember_lesson(
    pool: &PgPool,
    cfg: &Config,
    req: &RememberRequest,
) -> Result<RememberReceipt, AppError> {
    let text = req.lesson_body();
    let tags = req
        .tags
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let register = normalize_strings(&req.register);
    let thread_keys = normalize_strings(&req.thread_keys);
    let default_register =
        if matches!(req.kind.as_str(), "writing-lesson" | "design-lesson") && register.is_empty() {
            vec!["general".to_owned()]
        } else {
            Vec::new()
        };
    let meta = serde_json::json!({
        "origin": "direct-db-write",
        "kind": req.kind,
        "recorded_at": chrono::Utc::now().to_rfc3339(),
    });
    let triggers = req.trigger_spec();
    let mut tx = pool.begin().await?;
    let id = match req.kind.as_str() {
        "coding-lesson" => write_coding_lesson_tx(
            &mut tx,
            req.scope.as_deref().unwrap_or("house"),
            req.project.as_deref(),
            req.voice.as_deref(),
            req.shape.as_deref(),
            &req.title,
            text,
            req.trigger_context.as_deref(),
            req.proof_pattern.as_deref(),
            &req.language_keys,
            &req.technology_keys,
            &thread_keys,
            &tags,
            req.source_memory_path.as_deref(),
            &triggers,
            meta,
        )
        .await?,
        "project-lesson" => write_project_lesson_tx(
            &mut tx,
            req.project.as_deref().unwrap(),
            &req.title,
            text,
            req.trigger_context.as_deref(),
            req.proof_pattern.as_deref(),
            &req.language_keys,
            &req.technology_keys,
            &thread_keys,
            &tags,
            req.source_memory_path.as_deref(),
            &triggers,
            meta,
        )
        .await?,
        "writing-lesson" => sqlx::query_scalar::<_, i64>("INSERT INTO lessons (lesson_key,scope,voice,register,shape,title,lesson,trigger_context,thread_keys,tags,source_memory_path,meta,condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs) VALUES ('writing','house',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT (voice,title) WHERE lesson_key='writing' DO UPDATE SET register=EXCLUDED.register,shape=EXCLUDED.shape,lesson=EXCLUDED.lesson,trigger_context=EXCLUDED.trigger_context,thread_keys=EXCLUDED.thread_keys,tags=EXCLUDED.tags,source_memory_path=EXCLUDED.source_memory_path,meta=EXCLUDED.meta,condition=EXCLUDED.condition,ast_condition=EXCLUDED.ast_condition,trigger_scope=EXCLUDED.trigger_scope,interrupt_mode=EXCLUDED.interrupt_mode,repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs RETURNING id").bind(req.voice.as_deref().unwrap_or("general")).bind(if register.is_empty() { &default_register } else { &register }).bind(&req.shape).bind(&req.title).bind(text).bind(&req.trigger_context).bind(&thread_keys).bind(&tags).bind(&req.source_memory_path).bind(meta).bind(triggers.condition.as_slice()).bind(triggers.ast_condition.as_slice()).bind(triggers.trigger_scope.as_slice()).bind(triggers.interrupt_mode.as_deref()).bind(triggers.repeat_cooldown_secs).fetch_one(&mut *tx).await?,
        "design-lesson" => sqlx::query_scalar::<_, i64>("INSERT INTO lessons (lesson_key,scope,voice,register,shape,title,lesson,trigger_context,proof_pattern,example_text,thread_keys,tags,source_memory_path,meta,condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs) VALUES ('design','house',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) ON CONFLICT (voice,title) WHERE lesson_key='design' DO UPDATE SET register=EXCLUDED.register,shape=EXCLUDED.shape,lesson=EXCLUDED.lesson,trigger_context=EXCLUDED.trigger_context,proof_pattern=EXCLUDED.proof_pattern,example_text=EXCLUDED.example_text,thread_keys=EXCLUDED.thread_keys,tags=EXCLUDED.tags,source_memory_path=EXCLUDED.source_memory_path,meta=EXCLUDED.meta,condition=EXCLUDED.condition,ast_condition=EXCLUDED.ast_condition,trigger_scope=EXCLUDED.trigger_scope,interrupt_mode=EXCLUDED.interrupt_mode,repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs RETURNING id").bind(req.voice.as_deref().unwrap_or("general")).bind(if register.is_empty() { &default_register } else { &register }).bind(&req.shape).bind(&req.title).bind(text).bind(&req.trigger_context).bind(&req.proof_pattern).bind(&req.example_text).bind(&thread_keys).bind(&tags).bind(&req.source_memory_path).bind(meta).bind(triggers.condition.as_slice()).bind(triggers.ast_condition.as_slice()).bind(triggers.trigger_scope.as_slice()).bind(triggers.interrupt_mode.as_deref()).bind(triggers.repeat_cooldown_secs).fetch_one(&mut *tx).await?,
        "audio-lesson" => sqlx::query_scalar::<_, i64>("INSERT INTO lessons (lesson_key,scope,shape,title,lesson,trigger_context,thread_keys,tags,source_memory_path,meta,condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs) VALUES ('audio','house',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (title) WHERE lesson_key='audio' DO UPDATE SET shape=EXCLUDED.shape,lesson=EXCLUDED.lesson,trigger_context=EXCLUDED.trigger_context,thread_keys=EXCLUDED.thread_keys,tags=EXCLUDED.tags,source_memory_path=EXCLUDED.source_memory_path,meta=EXCLUDED.meta,condition=EXCLUDED.condition,ast_condition=EXCLUDED.ast_condition,trigger_scope=EXCLUDED.trigger_scope,interrupt_mode=EXCLUDED.interrupt_mode,repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs RETURNING id")
            .bind(&req.shape).bind(&req.title).bind(text).bind(&req.trigger_context).bind(&thread_keys).bind(&tags).bind(&req.source_memory_path).bind(meta).bind(triggers.condition.as_slice()).bind(triggers.ast_condition.as_slice()).bind(triggers.trigger_scope.as_slice()).bind(triggers.interrupt_mode.as_deref()).bind(triggers.repeat_cooldown_secs).fetch_one(&mut *tx).await?,
        _ => return Err(AppError::Invalid("unsupported remember kind".into())),
    };
    tx.commit().await?;
    let mut warnings = Vec::new();
    if req.backup
        && matches!(
            req.kind.as_str(),
            "project-lesson" | "audio-lesson" | "design-lesson"
        )
        && let Err(error) = backup::run_post_write(pool, &cfg.database_url).await
    {
        warnings.push(format!("backup failed: {error}"));
    }
    Ok(RememberReceipt {
        memory_id: 0,
        lesson_id: id,
        kind: req.kind.clone(),
        room: req.room.clone(),
        source_path: String::new(),
        durable: true,
        authority: "postgres",
        warnings,
    })
}

pub(crate) fn normalize_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && seen.insert(*value))
        .map(str::to_string)
        .collect()
}

pub(crate) fn normalize_threads(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && seen.insert(*value))
        .map(str::to_string)
        .collect()
}

pub(crate) fn derive_dates(path: &str, primary_date: NaiveDate) -> Vec<NaiveDate> {
    let mut out = vec![primary_date];
    for captures in PATH_DATE_RE.captures_iter(path) {
        if let (Ok(year), Ok(month), Ok(day)) = (
            captures[1].parse(),
            captures[2].parse(),
            captures[3].parse(),
        ) && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            out.push(date);
        }
    }
    for captures in STITCHED_PATH_DATE_RE.captures_iter(path) {
        if let (Ok(year), Ok(month), Ok(day), Ok(hour)) = (
            captures[1].parse::<i32>(),
            captures[2].parse::<u32>(),
            captures[3].parse::<u32>(),
            captures[4].parse::<u32>(),
        ) && hour < 24
            && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            out.push(date + chrono::Duration::days(1));
        }
    }
    out.sort();
    out.dedup();
    out
}

pub(crate) fn chunk_body(body: &str) -> Vec<(String, usize, usize, Option<String>)> {
    if body.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = body.chars().collect();
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut offset = 0usize;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        if content
            .strip_prefix("## ")
            .is_some_and(|h| !h.trim().is_empty())
        {
            headings.push((offset, content.trim().to_string()));
        }
        offset += line.chars().count();
    }
    let mut sections = Vec::new();
    if headings.is_empty() {
        sections.push((0, chars.len(), "__preamble__".to_string()));
    } else {
        if headings[0].0 > 0 {
            sections.push((0, headings[0].0, "__preamble__".to_string()));
        }
        for (i, (start, heading)) in headings.iter().enumerate() {
            sections.push((
                *start,
                headings.get(i + 1).map(|(s, _)| *s).unwrap_or(chars.len()),
                heading.clone(),
            ));
        }
    }
    let byte_at = |char_index: usize| -> usize {
        body.char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(body.len())
    };
    let mut out = Vec::new();
    for (start, end, heading) in sections {
        let text: String = chars[start..end].iter().collect();
        if text.chars().count() <= 4000 {
            if !text.trim().is_empty() {
                out.push((text, byte_at(start), byte_at(end), Some(heading)));
            }
            continue;
        }
        let mut paragraphs = Vec::new();
        let mut paragraph_start = start;
        for i in start..end.saturating_sub(1) {
            if chars[i] == '\n' && chars[i + 1] == '\n' {
                paragraphs.push((paragraph_start, i));
                paragraph_start = i + 2;
            }
        }
        paragraphs.push((paragraph_start, end));
        let mut pieces = Vec::new();
        let mut buf_start = paragraphs[0].0;
        let mut buf_end = paragraphs[0].1;
        for &(paragraph_start, paragraph_end) in paragraphs.iter().skip(1) {
            if buf_end - buf_start + (paragraph_end - paragraph_start) + 2 > 2200 {
                pieces.push((buf_start, buf_end));
                buf_start = buf_end.saturating_sub(200);
                buf_end = paragraph_end;
            } else {
                buf_end = paragraph_end;
            }
        }
        pieces.push((buf_start, buf_end));
        for (piece_start, piece_end) in pieces {
            let piece: String = chars[piece_start..piece_end].iter().collect();
            if !piece.trim().is_empty() {
                out.push((
                    piece,
                    byte_at(piece_start),
                    byte_at(piece_end),
                    Some(heading.clone()),
                ));
            }
        }
    }
    out
}
pub(crate) fn token_estimate(text: &str) -> i32 {
    (text.chars().count() / 4).max(1) as i32
}

pub(crate) async fn embed(
    client: &Client,
    url: &str,
    model: &str,
    chunks: &[(String, usize, usize, Option<String>)],
    dim: usize,
) -> Result<Vec<Vec<f32>>, AppError> {
    #[derive(Serialize)]
    struct Input<'a> {
        model: &'a str,
        input: Vec<String>,
    }
    let input = chunks.iter().map(|x| format!("passage: {}", x.0)).collect();
    let value: serde_json::Value = client
        .post(url)
        .timeout(Duration::from_secs(20))
        .json(&Input { model, input })
        .send()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let arr = value
        .get("embeddings")
        .or_else(|| value.get("data"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Embedding("response lacks embeddings".into()))?;
    let mut out = Vec::new();
    for item in arr {
        let v = item
            .as_array()
            .or_else(|| item.get("embedding").and_then(|x| x.as_array()))
            .ok_or_else(|| AppError::Embedding("invalid embedding vector".into()))?;
        let row: Vec<f32> = v
            .iter()
            .map(|x| {
                x.as_f64()
                    .map(|n| n as f32)
                    .ok_or_else(|| AppError::Embedding("non-numeric embedding".into()))
            })
            .collect::<Result<_, _>>()?;
        if row.len() != dim {
            return Err(AppError::Embedding(format!(
                "embedding dimension {} != {}",
                row.len(),
                dim
            )));
        }
        out.push(row);
    }
    Ok(out)
}
