use crate::config::{AppError, ROOM_KEY_RE};
use chrono::{DateTime, Utc};
use hearth::lesson_triggers::{
    CompiledTriggerSet, LessonTriggerSpec, Surface, SurfaceKind, TriggerRow, cached_set,
    match_surfaces, store_set,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use std::collections::HashMap;

/// One payload the caller offers for matching.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonTriggerSurface {
    pub kind: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LessonTriggerMatchParams {
    pub room: String,
    pub session: String,
    pub surfaces: Vec<LessonTriggerSurface>,
    /// The caller's active project slug, when it has one. Absent keeps the
    /// universal fence: only project-agnostic lessons fire.
    #[serde(default)]
    pub project: Option<String>,
}

// enough: one turn offers at most this many surfaces — the edited file plus the
// latest assistant turn, with headroom. Upgrade path: raise the ceiling once a
// caller has a real reason to batch.
const MAX_TRIGGER_SURFACES: usize = 16;

impl LessonTriggerMatchParams {
    pub fn validate(&self) -> Result<(), AppError> {
        if !ROOM_KEY_RE.is_match(&self.room) {
            return Err(AppError::Invalid("room must be a lowercase slug".into()));
        }
        if self.session.trim().is_empty() {
            return Err(AppError::Invalid("session must not be empty".into()));
        }
        if self.surfaces.is_empty() {
            return Err(AppError::Invalid("surfaces must not be empty".into()));
        }
        if self.surfaces.len() > MAX_TRIGGER_SURFACES {
            return Err(AppError::Invalid(format!(
                "surfaces must contain at most {MAX_TRIGGER_SURFACES} entries"
            )));
        }
        Ok(())
    }

    /// The project slug in fence-normal form: trimmed, lowercased, spaces
    /// folded to dashes. Empty input means no project at all.
    fn normalized_project(&self) -> Option<String> {
        let raw = self.project.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        Some(
            raw.to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("-"),
        )
    }
    fn scopes(&self) -> Vec<String> {
        if self.room == "house" {
            vec!["house".into()]
        } else {
            vec!["house".into(), self.room.clone()]
        }
    }
}

/// A fired lesson keeps its typed store identity: family plus id, never a
/// flattened row.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonTriggerFired {
    pub family: String,
    pub id: i64,
    pub title: String,
    pub lesson: String,
    pub proof_pattern: Option<String>,
    pub urgency: String,
    pub surface: String,
    pub surface_index: usize,
    pub path: Option<String>,
    pub pattern_kind: String,
    pub pattern: String,
    pub match_start: Option<usize>,
    /// Ledger rows for this lesson in this room, including this fire. The
    /// cockpit renders it as `writing#408 ×15`; cooldown never reads it.
    pub fires: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonTriggerMatchResult {
    pub ok: bool,
    pub fired: Vec<LessonTriggerFired>,
    /// Always serialized, never skipped when empty: an adapter that reads
    /// warnings[0] must meet an array, not an absent field.
    pub warnings: Vec<String>,
}

const TRIGGER_SELECT: &str = "SELECT id,lesson_key,title,lesson,proof_pattern,condition,ast_condition,trigger_scope,interrupt_mode,repeat_cooldown_secs,language_keys,updated_at FROM lessons";

/// The one visibility clause for trigger-bearing lessons. Named once here and
/// pushed by every query that reads triggers, so the rule cannot drift.
fn trigger_eligibility(
    qb: &mut QueryBuilder<'_, Postgres>,
    scopes: &[String],
    project: Option<&str>,
) {
    qb.push(" WHERE (condition <> '{}' OR ast_condition <> '{}')");
    // A project lesson firing inside a foreign project is a false block, so the
    // fence admits it only when the caller stands in that project. The column
    // side is normalized here because the data carries slug drift ("The
    // Athanor" / "the-athanor"); the caller side arrives pre-normalized.
    match project {
        Some(slug) => {
            qb.push(" AND (project IS NULL OR lower(replace(project, ' ', '-')) = ")
                .push_bind(slug.to_owned())
                .push(")");
        }
        None => {
            qb.push(" AND project IS NULL");
        }
    }
    qb.push(" AND (lesson_key <> 'coding' OR scope = ANY(")
        .push_bind(scopes.to_vec())
        .push("))");
}

fn trigger_row(row: &sqlx::postgres::PgRow) -> Result<TriggerRow, sqlx::Error> {
    Ok(TriggerRow {
        family: row.try_get("lesson_key")?,
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        lesson: row.try_get("lesson")?,
        proof_pattern: row.try_get("proof_pattern")?,
        spec: LessonTriggerSpec {
            condition: row.try_get("condition")?,
            ast_condition: row.try_get("ast_condition")?,
            trigger_scope: row.try_get("trigger_scope")?,
            interrupt_mode: row.try_get("interrupt_mode")?,
            repeat_cooldown_secs: row.try_get("repeat_cooldown_secs")?,
            language_keys: row.try_get("language_keys")?,
        },
    })
}

pub async fn lesson_trigger_match(
    pool: &PgPool,
    params: LessonTriggerMatchParams,
) -> Result<LessonTriggerMatchResult, AppError> {
    params.validate()?;
    let mut surfaces = Vec::with_capacity(params.surfaces.len());
    for surface in &params.surfaces {
        surfaces.push(Surface {
            kind: SurfaceKind::parse(&surface.kind).map_err(AppError::Invalid)?,
            tool: surface.tool.as_deref(),
            path: surface.path.as_deref(),
            text: surface.text.as_str(),
        });
    }

    let project = params.normalized_project();
    let mut qb = QueryBuilder::<Postgres>::new(TRIGGER_SELECT);
    trigger_eligibility(&mut qb, &params.scopes(), project.as_deref());
    qb.push(" ORDER BY id");
    let rows = qb.build().fetch_all(pool).await?;
    let mut latest: Option<DateTime<Utc>> = None;
    for row in &rows {
        let updated: DateTime<Utc> = row.try_get("updated_at")?;
        if latest.is_none_or(|current| updated > current) {
            latest = Some(updated);
        }
    }
    let fingerprint = format!(
        "{}:{}",
        rows.len(),
        latest.map_or(0, |value| value.timestamp_micros())
    );
    // The compile cache fences by room AND project: the two fences see
    // different row sets, and the fingerprint alone cannot be trusted to
    // differ between them.
    let cache_key = match project.as_deref() {
        Some(slug) => format!("{}\u{0}{}", params.room, slug),
        None => params.room.clone(),
    };
    let cached = cached_set(&cache_key, &fingerprint);
    let set = match cached {
        Some(set) => set,
        None => {
            let compiled = rows
                .iter()
                .map(trigger_row)
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            store_set(
                &cache_key,
                CompiledTriggerSet::compile(fingerprint, &compiled),
            )
        }
    };

    let outcome = match_surfaces(&set, &surfaces);
    if outcome.hits.is_empty() {
        return Ok(LessonTriggerMatchResult {
            ok: true,
            fired: Vec::new(),
            warnings: outcome.warnings,
        });
    }

    // The repeat policy is read from the ledger, never from process memory: a
    // second substrate process must reach the same verdict.
    let candidates = outcome
        .hits
        .iter()
        .map(|hit| set.triggers()[hit.trigger].id)
        .collect::<Vec<_>>();
    let ledger = sqlx::query(
        "SELECT lesson_key,lesson_id,
                EXTRACT(EPOCH FROM (NOW() - MAX(fired_at) FILTER (WHERE session_id=$2)))::BIGINT AS age,
                COUNT(*) AS fires
         FROM lesson_trigger_events
         WHERE room=$1 AND lesson_id = ANY($3)
         GROUP BY lesson_key,lesson_id",
    )
    .bind(&params.room)
    .bind(&params.session)
    .bind(&candidates)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| -> Result<((String, i64), (Option<i64>, i64)), sqlx::Error> {
        // Age stays session-scoped (the FILTER) because repeat_cooldown_secs
        // means once-per-session; fires counts the whole room's history so the
        // card tells the operator how often this lesson has bitten here.
        Ok((
            (
                row.try_get::<String, _>("lesson_key")?,
                row.try_get::<i64, _>("lesson_id")?,
            ),
            (
                row.try_get::<Option<i64>, _>("age")?,
                row.try_get::<i64, _>("fires")?,
            ),
        ))
    })
    .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;

    let mut fired = Vec::new();
    let mut tx = pool.begin().await?;
    for hit in &outcome.hits {
        let trigger = &set.triggers()[hit.trigger];
        let entry = ledger.get(&(trigger.family.clone(), trigger.id)).copied();
        let age = entry.and_then(|(age, _)| age);
        if !trigger.cooldown.allows(age) {
            continue;
        }
        sqlx::query(
            "INSERT INTO lesson_trigger_events
             (lesson_key,lesson_id,room,session_id,surface,tool_name,path,pattern_kind,
              matched_pattern,urgency)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(&trigger.family)
        .bind(trigger.id)
        .bind(&params.room)
        .bind(&params.session)
        .bind(hit.surface.as_str())
        .bind(hit.tool.as_deref())
        .bind(hit.path.as_deref())
        .bind(hit.pattern_kind.as_str())
        .bind(&hit.pattern)
        .bind(trigger.urgency.as_str())
        .execute(&mut *tx)
        .await?;
        fired.push(LessonTriggerFired {
            family: trigger.family.clone(),
            id: trigger.id,
            title: trigger.title.clone(),
            lesson: trigger.lesson.clone(),
            proof_pattern: trigger.proof_pattern.clone(),
            urgency: trigger.urgency.as_str().to_owned(),
            surface: hit.surface.as_str().to_owned(),
            surface_index: hit.surface_index,
            path: hit.path.clone(),
            pattern_kind: hit.pattern_kind.as_str().to_owned(),
            pattern: hit.pattern.clone(),
            match_start: hit.match_start,
            fires: entry.map(|(_, fires)| fires).unwrap_or(0) + 1,
        });
    }
    tx.commit().await?;
    Ok(LessonTriggerMatchResult {
        ok: true,
        fired,
        warnings: outcome.warnings,
    })
}
