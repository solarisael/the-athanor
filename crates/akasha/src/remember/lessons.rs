use super::{RememberReceipt, RememberRequest, normalize_strings};
use crate::backup;
use crate::config::{AppError, Config};
use crate::settings::RoomSettings;
use hearth::lesson_triggers::LessonTriggerSpec;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

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
