use super::{RememberReceipt, RememberRequest, normalize_strings};
use crate::backup;
use crate::config::{AppError, Config};
use crate::settings::RoomSettings;
use hearth::lesson_triggers::LessonTriggerSpec;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};

// Every writer below hands Postgres one jsonb row keyed by `lessons` column
// names (0008_unified_lessons.sql:5, widened by 0013/0014/0019) — matched by
// NAME, so a renamed column dies as a NOT NULL refusal, not a shifted bind.
// Column lists stay written out because `kind_path` and `lesson_tsv` are
// GENERATED: `SELECT *` off the record would try to write them.

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
    let row = json!({
        "lesson_key": "coding",
        "scope": scope,
        "project": project,
        "voice": voice,
        "shape": shape,
        "title": title,
        "lesson": lesson,
        "trigger_context": trigger_context,
        "proof_pattern": proof_pattern,
        "language_keys": language_keys,
        "technology_keys": technology_keys,
        "thread_keys": thread_keys,
        "tags": tags,
        "source_memory_path": source_memory_path,
        "meta": meta,
        "condition": triggers.condition,
        "ast_condition": triggers.ast_condition,
        "trigger_scope": triggers.trigger_scope,
        "interrupt_mode": triggers.interrupt_mode,
        "repeat_cooldown_secs": triggers.repeat_cooldown_secs,
    });

    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,project,voice,shape,title,lesson,trigger_context,proof_pattern,
          language_keys,technology_keys,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         SELECT
          lesson_key,scope,project,voice,shape,title,lesson,trigger_context,proof_pattern,
          language_keys,technology_keys,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs
         FROM jsonb_populate_record(NULL::lessons, $1)
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
    .bind(row)
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
    let row = json!({
        "lesson_key": "project",
        "scope": "project",
        "project": project,
        "title": title,
        "lesson": lesson,
        "trigger_context": trigger_context,
        "proof_pattern": proof_pattern,
        "language_keys": language_keys,
        "technology_keys": technology_keys,
        "thread_keys": thread_keys,
        "tags": tags,
        "source_memory_path": source_memory_path,
        "meta": meta,
        "condition": triggers.condition,
        "ast_condition": triggers.ast_condition,
        "trigger_scope": triggers.trigger_scope,
        "interrupt_mode": triggers.interrupt_mode,
        "repeat_cooldown_secs": triggers.repeat_cooldown_secs,
    });

    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,project,title,lesson,trigger_context,proof_pattern,
          language_keys,technology_keys,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         SELECT
          lesson_key,scope,project,title,lesson,trigger_context,proof_pattern,
          language_keys,technology_keys,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs
         FROM jsonb_populate_record(NULL::lessons, $1)
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
    .bind(row)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
async fn write_writing_lesson_tx(
    tx: &mut Transaction<'_, Postgres>,
    voice: &str,
    register: &[String],
    shape: Option<&str>,
    title: &str,
    lesson: &str,
    trigger_context: Option<&str>,
    thread_keys: &[String],
    tags: &[String],
    source_memory_path: Option<&str>,
    triggers: &LessonTriggerSpec,
    meta: Value,
) -> Result<i64, AppError> {
    let row = json!({
        "lesson_key": "writing",
        "scope": "house",
        "voice": voice,
        "register": register,
        "shape": shape,
        "title": title,
        "lesson": lesson,
        "trigger_context": trigger_context,
        "thread_keys": thread_keys,
        "tags": tags,
        "source_memory_path": source_memory_path,
        "meta": meta,
        "condition": triggers.condition,
        "ast_condition": triggers.ast_condition,
        "trigger_scope": triggers.trigger_scope,
        "interrupt_mode": triggers.interrupt_mode,
        "repeat_cooldown_secs": triggers.repeat_cooldown_secs,
    });

    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,voice,register,shape,title,lesson,trigger_context,
          thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         SELECT
          lesson_key,scope,voice,register,shape,title,lesson,trigger_context,
          thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs
         FROM jsonb_populate_record(NULL::lessons, $1)
         ON CONFLICT (voice,title) WHERE lesson_key='writing' DO UPDATE
         SET register=EXCLUDED.register,shape=EXCLUDED.shape,lesson=EXCLUDED.lesson,
             trigger_context=EXCLUDED.trigger_context,thread_keys=EXCLUDED.thread_keys,
             tags=EXCLUDED.tags,source_memory_path=EXCLUDED.source_memory_path,
             meta=EXCLUDED.meta,condition=EXCLUDED.condition,
             ast_condition=EXCLUDED.ast_condition,trigger_scope=EXCLUDED.trigger_scope,
             interrupt_mode=EXCLUDED.interrupt_mode,
             repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs
         RETURNING id",
    )
    .bind(row)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
async fn write_design_lesson_tx(
    tx: &mut Transaction<'_, Postgres>,
    voice: &str,
    register: &[String],
    shape: Option<&str>,
    title: &str,
    lesson: &str,
    trigger_context: Option<&str>,
    proof_pattern: Option<&str>,
    example_text: Option<&str>,
    thread_keys: &[String],
    tags: &[String],
    source_memory_path: Option<&str>,
    triggers: &LessonTriggerSpec,
    meta: Value,
) -> Result<i64, AppError> {
    let row = json!({
        "lesson_key": "design",
        "scope": "house",
        "voice": voice,
        "register": register,
        "shape": shape,
        "title": title,
        "lesson": lesson,
        "trigger_context": trigger_context,
        "proof_pattern": proof_pattern,
        "example_text": example_text,
        "thread_keys": thread_keys,
        "tags": tags,
        "source_memory_path": source_memory_path,
        "meta": meta,
        "condition": triggers.condition,
        "ast_condition": triggers.ast_condition,
        "trigger_scope": triggers.trigger_scope,
        "interrupt_mode": triggers.interrupt_mode,
        "repeat_cooldown_secs": triggers.repeat_cooldown_secs,
    });

    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,voice,register,shape,title,lesson,trigger_context,
          proof_pattern,example_text,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         SELECT
          lesson_key,scope,voice,register,shape,title,lesson,trigger_context,
          proof_pattern,example_text,thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs
         FROM jsonb_populate_record(NULL::lessons, $1)
         ON CONFLICT (voice,title) WHERE lesson_key='design' DO UPDATE
         SET register=EXCLUDED.register,shape=EXCLUDED.shape,lesson=EXCLUDED.lesson,
             trigger_context=EXCLUDED.trigger_context,
             proof_pattern=EXCLUDED.proof_pattern,example_text=EXCLUDED.example_text,
             thread_keys=EXCLUDED.thread_keys,tags=EXCLUDED.tags,
             source_memory_path=EXCLUDED.source_memory_path,meta=EXCLUDED.meta,
             condition=EXCLUDED.condition,ast_condition=EXCLUDED.ast_condition,
             trigger_scope=EXCLUDED.trigger_scope,interrupt_mode=EXCLUDED.interrupt_mode,
             repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs
         RETURNING id",
    )
    .bind(row)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
async fn write_audio_lesson_tx(
    tx: &mut Transaction<'_, Postgres>,
    shape: Option<&str>,
    title: &str,
    lesson: &str,
    trigger_context: Option<&str>,
    thread_keys: &[String],
    tags: &[String],
    source_memory_path: Option<&str>,
    triggers: &LessonTriggerSpec,
    meta: Value,
) -> Result<i64, AppError> {
    let row = json!({
        "lesson_key": "audio",
        "scope": "house",
        "shape": shape,
        "title": title,
        "lesson": lesson,
        "trigger_context": trigger_context,
        "thread_keys": thread_keys,
        "tags": tags,
        "source_memory_path": source_memory_path,
        "meta": meta,
        "condition": triggers.condition,
        "ast_condition": triggers.ast_condition,
        "trigger_scope": triggers.trigger_scope,
        "interrupt_mode": triggers.interrupt_mode,
        "repeat_cooldown_secs": triggers.repeat_cooldown_secs,
    });

    sqlx::query_scalar(
        "INSERT INTO lessons
         (lesson_key,scope,shape,title,lesson,trigger_context,
          thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs)
         SELECT
          lesson_key,scope,shape,title,lesson,trigger_context,
          thread_keys,tags,source_memory_path,meta,
          condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs
         FROM jsonb_populate_record(NULL::lessons, $1)
         ON CONFLICT (title) WHERE lesson_key='audio' DO UPDATE
         SET shape=EXCLUDED.shape,lesson=EXCLUDED.lesson,
             trigger_context=EXCLUDED.trigger_context,thread_keys=EXCLUDED.thread_keys,
             tags=EXCLUDED.tags,source_memory_path=EXCLUDED.source_memory_path,
             meta=EXCLUDED.meta,condition=EXCLUDED.condition,
             ast_condition=EXCLUDED.ast_condition,trigger_scope=EXCLUDED.trigger_scope,
             interrupt_mode=EXCLUDED.interrupt_mode,
             repeat_cooldown_secs=EXCLUDED.repeat_cooldown_secs
         RETURNING id",
    )
    .bind(row)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

pub(super) async fn remember_lesson(
    pool: &PgPool,
    cfg: &Config,
    settings: &RoomSettings,
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
    let thread_keys = normalize_strings(&req.thread_keys);
    let register = normalize_strings(&req.register);
    // A voiced lesson with no register still has to satisfy the writing and
    // design readers, which filter on register; 'general' is that floor.
    let register = if register.is_empty()
        && matches!(req.kind.as_str(), "writing-lesson" | "design-lesson")
    {
        vec!["general".to_owned()]
    } else {
        register
    };
    let meta = json!({
        "origin": "direct-db-write",
        "kind": req.kind,
        "recorded_at": chrono::Utc::now().to_rfc3339(),
    });
    let triggers = req.trigger_spec();
    let mut tx = pool.begin().await?;
    let id = match req.kind.as_str() {
        "coding-lesson" => {
            write_coding_lesson_tx(
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
            .await?
        }
        "project-lesson" => {
            write_project_lesson_tx(
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
            .await?
        }
        "writing-lesson" => {
            write_writing_lesson_tx(
                &mut tx,
                req.voice.as_deref().unwrap_or("general"),
                &register,
                req.shape.as_deref(),
                &req.title,
                text,
                req.trigger_context.as_deref(),
                &thread_keys,
                &tags,
                req.source_memory_path.as_deref(),
                &triggers,
                meta,
            )
            .await?
        }
        "design-lesson" => {
            write_design_lesson_tx(
                &mut tx,
                req.voice.as_deref().unwrap_or("general"),
                &register,
                req.shape.as_deref(),
                &req.title,
                text,
                req.trigger_context.as_deref(),
                req.proof_pattern.as_deref(),
                req.example_text.as_deref(),
                &thread_keys,
                &tags,
                req.source_memory_path.as_deref(),
                &triggers,
                meta,
            )
            .await?
        }
        "audio-lesson" => {
            write_audio_lesson_tx(
                &mut tx,
                req.shape.as_deref(),
                &req.title,
                text,
                req.trigger_context.as_deref(),
                &thread_keys,
                &tags,
                req.source_memory_path.as_deref(),
                &triggers,
                meta,
            )
            .await?
        }
        _ => return Err(AppError::Invalid("unsupported remember kind".into())),
    };
    tx.commit().await?;
    let mut warnings = Vec::new();
    if req.backup
        && matches!(
            req.kind.as_str(),
            "project-lesson" | "audio-lesson" | "design-lesson"
        )
        && let Err(error) =
            backup::run_post_write(pool, &cfg.database_url, settings.backup_keep_count).await
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
