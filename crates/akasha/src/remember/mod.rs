mod chunking;
mod dates;
mod embedding;
mod lessons;
mod memory_write;
mod normalize;
mod tokens;

pub(crate) use chunking::chunk_body;
pub(crate) use dates::derive_dates;
pub(crate) use embedding::embed;
pub(crate) use lessons::{write_coding_lesson_tx, write_project_lesson_tx};
// On the door because `prepare_memory_write` hands this type back: a caller outside
// this folder that names the returned value needs the path, though none does today.
#[allow(unused_imports)]
pub(crate) use memory_write::PreparedMemoryWrite;
pub(crate) use memory_write::{prepare_memory_write, write_memory_tx};
pub(crate) use normalize::{normalize_strings, normalize_threads};
pub(crate) use tokens::token_estimate;

use crate::backup;
use crate::config::{AppError, Config, ROOM_KEY_RE};
use chrono::Local;
use hearth::lesson_triggers::LessonTriggerSpec;
use lessons::remember_lesson;
use memory_write::write_continuations_tx;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashSet;

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
    /// The lesson's trigger columns as core sees them. Owned rather than
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
