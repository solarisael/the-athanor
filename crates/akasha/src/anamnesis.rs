//! The substrate door for the Anamnesis Cabinet: store a drawer or a lived
//! repetition, read the wake set or consult by query.
//!
//! Requests arrive as `summoning` domain types, already refused or accepted
//! there; the database owns kind, fidelity, and activation through its CHECK
//! constraints. Nothing here validates twice.

use crate::backup;
use crate::config::{AppError, Config, EmbeddingMode, HTTP_CLIENT};
use crate::remember::embed;
use crate::settings::RoomSettings;
use chrono::NaiveDate;
use protocol::{AnamnesisResult, AnamnesisWriteResult};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use summoning::{
    AnamnesisAddRequest, AnamnesisAppendReceipt, AnamnesisAppendRequest, AnamnesisReadMode,
    AnamnesisReadRequest, AnamnesisReceipt, AnamnesisSeedRep, AnamnesisWriteRequest,
};

pub async fn anamnesis_write(
    pool: &PgPool,
    cfg: &Config,
    req: AnamnesisWriteRequest,
) -> Result<AnamnesisWriteResult, AppError> {
    match req {
        AnamnesisWriteRequest::Add(add) => add_drawer(pool, cfg, add).await,
        AnamnesisWriteRequest::AppendRep(append) => append_rep(pool, cfg, append).await,
    }
}

pub async fn anamnesis(
    pool: &PgPool,
    req: AnamnesisReadRequest,
) -> Result<AnamnesisResult, AppError> {
    let room = req.room().as_str();
    let settings = RoomSettings::load(pool, room).await?;
    let mut drawers = match req.mode() {
        AnamnesisReadMode::Wake => wake_set(pool, room, req.limit()).await?,
        AnamnesisReadMode::Consult => {
            let query = req.query().unwrap_or_default();
            consult(pool, room, query, req.limit(), &settings.house_language).await?
        }
    };

    let mut warnings = Vec::new();
    if req.mode() == AnamnesisReadMode::Wake {
        // A wake cycle with no verify note gives the live turn nothing to
        // check against, so wake leaves it in the drawer and says so.
        let (verifiable, unverifiable): (Vec<_>, Vec<_>) =
            drawers.into_iter().partition(Drawer::is_verifiable);
        drawers = verifiable;
        warnings.extend(
            unverifiable
                .iter()
                .map(|drawer| format!("excluded cycle {}: blank verify_note", drawer.id)),
        );
    }
    for drawer in &mut drawers {
        drawer.reps = recent_reps(pool, drawer.id).await?;
    }

    Ok(AnamnesisResult {
        ok: true,
        mode: req.mode().as_str().into(),
        room: room.into(),
        query: req.query().map(str::to_owned),
        found: !drawers.is_empty(),
        entries: drawers.iter().map(|drawer| json!(drawer)).collect(),
        warnings,
    })
}

// ---- write ----

async fn add_drawer(
    pool: &PgPool,
    cfg: &Config,
    add: AnamnesisAddRequest,
) -> Result<AnamnesisWriteResult, AppError> {
    let settings = RoomSettings::load(pool, add.room().as_str()).await?;
    let mut warnings = Vec::new();
    let embedding = drawer_embedding(cfg, &add).await?;
    if embedding.is_none()
        && let Some(warning) = embedding_warning(cfg.embedding_mode)
    {
        warnings.push(warning.into());
    }

    let mut tx = pool.begin().await?;
    let id = sqlx::query_scalar::<_, i64>(UPSERT_DRAWER)
        .bind(drawer_row(&add, embedding))
        .fetch_one(&mut *tx)
        .await?;
    if let Some(seed) = add.seed_rep() {
        upsert_rep(&mut tx, id, seed, None).await?;
    }
    tx.commit().await?;

    warnings.extend(backup_warning(pool, cfg, &settings).await);
    let receipt =
        AnamnesisReceipt::committed(add.room().clone(), add.title().into(), add.kind(), warnings)
            .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(receipt.into())
}

async fn append_rep(
    pool: &PgPool,
    cfg: &Config,
    append: AnamnesisAppendRequest,
) -> Result<AnamnesisWriteResult, AppError> {
    let settings = RoomSettings::load(pool, append.room().as_str()).await?;
    let title = append.title().trim();

    let mut tx = pool.begin().await?;
    let id = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM anamnesis WHERE room = $1 AND title = $2 AND kind = 'cycle'",
    )
    .bind(append.room().as_str())
    .bind(title)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("append-rep target cycle not found".into()))?;
    upsert_rep(&mut tx, id, append.rep(), append.source_paths().first()).await?;
    tx.commit().await?;

    let warnings = backup_warning(pool, cfg, &settings)
        .await
        .into_iter()
        .collect();
    let receipt = AnamnesisAppendReceipt::committed(
        append.room().clone(),
        title.into(),
        append.rep().number(),
        warnings,
    )
    .map_err(|error| AppError::Invalid(error.to_string()))?;
    Ok(receipt.into())
}

async fn upsert_rep(
    tx: &mut Transaction<'_, Postgres>,
    cabinet_id: i64,
    rep: &AnamnesisSeedRep,
    source_path: Option<&String>,
) -> Result<(), AppError> {
    sqlx::query(UPSERT_REP)
        .bind(rep_row(cabinet_id, rep, source_path))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// Keys are anamnesis column names; the table is the only schema. The row
// travels as one jsonb parameter so columns match by name, never by position.
fn drawer_row(add: &AnamnesisAddRequest, embedding: Option<String>) -> Value {
    json!({
        "room": add.room().as_str(),
        "kind": add.kind().as_str(),
        "fidelity": add.fidelity().as_str(),
        "activation": add.activation().as_str(),
        "active": !add.dormant(),
        "title": add.title(),
        "shape": add.shape(),
        "peak": add.peak(),
        "beginning": add.beginning(),
        "ramp": add.ramp(),
        "counsel": add.counsel(),
        "verify_note": add.verify_note(),
        "source_paths": add.source_paths(),
        "canon_links": add.canon(),
        "tags": add.tags(),
        "body_embedding": embedding,
    })
}

fn rep_row(cabinet_id: i64, rep: &AnamnesisSeedRep, source_path: Option<&String>) -> Value {
    json!({
        "cabinet_id": cabinet_id,
        "rep_number": rep.number(),
        "occurred_on": rep.occurred_on(),
        "how_it_went": rep.how_it_went(),
        "portal_pull": rep.portal_pull(),
        "lighter": rep.lighter(),
        "source_path": source_path,
    })
}

const UPSERT_DRAWER: &str = "
    INSERT INTO anamnesis (
        room, kind, fidelity, activation, active, title, shape, peak, beginning,
        ramp, counsel, verify_note, source_paths, canon_links, tags,
        body_embedding, embedded_at
    )
    SELECT
        room, kind, fidelity, activation, active, title, shape, peak, beginning,
        ramp, counsel, verify_note, source_paths, canon_links, tags,
        body_embedding, CASE WHEN body_embedding IS NULL THEN NULL ELSE NOW() END
    FROM jsonb_populate_record(NULL::anamnesis, $1)
    ON CONFLICT (room, title) DO UPDATE SET
        kind = EXCLUDED.kind,
        fidelity = EXCLUDED.fidelity,
        activation = EXCLUDED.activation,
        active = EXCLUDED.active,
        shape = EXCLUDED.shape,
        peak = EXCLUDED.peak,
        beginning = EXCLUDED.beginning,
        ramp = EXCLUDED.ramp,
        counsel = EXCLUDED.counsel,
        verify_note = EXCLUDED.verify_note,
        source_paths = EXCLUDED.source_paths,
        canon_links = EXCLUDED.canon_links,
        tags = EXCLUDED.tags,
        body_embedding = EXCLUDED.body_embedding,
        embedded_at = EXCLUDED.embedded_at
    RETURNING id
";

const UPSERT_REP: &str = "
    INSERT INTO anamnesis_reps (
        cabinet_id, rep_number, occurred_on, how_it_went, portal_pull, lighter, source_path
    )
    SELECT
        cabinet_id, rep_number, occurred_on, how_it_went, portal_pull, lighter, source_path
    FROM jsonb_populate_record(NULL::anamnesis_reps, $1)
    ON CONFLICT (cabinet_id, rep_number) DO UPDATE SET
        occurred_on = EXCLUDED.occurred_on,
        how_it_went = EXCLUDED.how_it_went,
        portal_pull = EXCLUDED.portal_pull,
        lighter = EXCLUDED.lighter,
        source_path = EXCLUDED.source_path
";

async fn drawer_embedding(
    cfg: &Config,
    add: &AnamnesisAddRequest,
) -> Result<Option<String>, AppError> {
    match cfg.embedding_mode {
        EmbeddingMode::Required => {}
        EmbeddingMode::Disabled | EmbeddingMode::DisabledForTest => return Ok(None),
    }
    let url = cfg
        .embed_url
        .as_deref()
        .ok_or_else(|| AppError::Embedding("embedding endpoint is required".into()))?;
    // The embedded text mirrors the body_tsv column expression in 0001_initial.sql.
    let text = [
        add.title(),
        add.shape().unwrap_or(""),
        add.ramp(),
        add.counsel().unwrap_or(""),
        add.peak().unwrap_or(""),
    ]
    .join("\n");
    let rows = embed(
        &HTTP_CLIENT,
        url,
        &cfg.embed_model,
        &[(text.clone(), 0, text.len(), None)],
        cfg.embed_dimension,
    )
    .await?;
    let literal = rows.first().map(|vector| {
        let parts: Vec<String> = vector.iter().map(ToString::to_string).collect();
        format!("[{}]", parts.join(","))
    });
    Ok(literal)
}

fn embedding_warning(mode: EmbeddingMode) -> Option<&'static str> {
    match mode {
        EmbeddingMode::Required => None,
        EmbeddingMode::Disabled => {
            Some("embedding disabled in production; cabinet embedding omitted")
        }
        EmbeddingMode::DisabledForTest => {
            Some("embedding disabled for isolated test; cabinet embedding omitted")
        }
    }
}

// The row is durable once committed; a failed backup is a warning, never a
// failed write.
async fn backup_warning(pool: &PgPool, cfg: &Config, settings: &RoomSettings) -> Option<String> {
    backup::run_post_write(pool, &cfg.database_url, settings.backup_keep_count)
        .await
        .err()
        .map(|error| format!("backup failed: {error}"))
}

// ---- read ----

#[derive(Serialize, sqlx::FromRow)]
struct Drawer {
    id: i64,
    room: String,
    kind: String,
    fidelity: String,
    activation: String,
    active: bool,
    title: String,
    shape: Option<String>,
    peak: Option<String>,
    beginning: Option<String>,
    ramp: String,
    counsel: Option<String>,
    verify_note: Option<String>,
    source_paths: Vec<String>,
    canon_links: Vec<String>,
    tags: Vec<String>,
    #[sqlx(skip)]
    reps: Vec<Rep>,
}

impl Drawer {
    fn is_verifiable(&self) -> bool {
        self.kind != "cycle"
            || self
                .verify_note
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty())
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct Rep {
    rep_number: i32,
    occurred_on: Option<NaiveDate>,
    how_it_went: String,
    portal_pull: String,
    lighter: String,
    source_path: Option<String>,
}

const DRAWER_COLUMNS: &str = "
    id, room, kind, fidelity, activation, active, title, shape, peak, beginning,
    ramp, counsel, verify_note, source_paths, canon_links, tags
";

// A room wakes with its own drawers and the House's.
fn scope(room: &str) -> Vec<String> {
    if room == "house" {
        vec!["house".into()]
    } else {
        vec![room.into(), "house".into()]
    }
}

async fn wake_set(pool: &PgPool, room: &str, limit: u32) -> Result<Vec<Drawer>, AppError> {
    let sql = format!(
        "SELECT {DRAWER_COLUMNS}
         FROM anamnesis
         WHERE room = ANY($1)
           AND activation = 'wake'
           AND (kind = 'pillar' OR active)
         ORDER BY kind = 'pillar' DESC, updated_at DESC, id DESC
         LIMIT $2"
    );
    let drawers = sqlx::query_as::<_, Drawer>(&sql)
        .bind(scope(room))
        .bind(i64::from(limit))
        .fetch_all(pool)
        .await?;
    Ok(drawers)
}

// Full-text rank first, trigram similarity as the tiebreak, both over the same
// text the body_tsv column indexes.
async fn consult(
    pool: &PgPool,
    room: &str,
    query: &str,
    limit: u32,
    language: &str,
) -> Result<Vec<Drawer>, AppError> {
    let sql = format!(
        "WITH drawer AS (
             SELECT {DRAWER_COLUMNS}, updated_at, body_tsv,
                    lower(concat_ws(' ', title, shape, ramp, counsel, peak,
                                    array_to_string(canon_links, ' '),
                                    array_to_string(tags, ' '))) AS haystack
             FROM anamnesis
             WHERE room = ANY($1)
         )
         SELECT {DRAWER_COLUMNS}
         FROM drawer
         WHERE body_tsv @@ plainto_tsquery($4::regconfig, $2)
            OR haystack LIKE '%' || lower($2) || '%'
         ORDER BY ts_rank_cd(body_tsv, plainto_tsquery($4::regconfig, $2)) * 10
                  + similarity(haystack, lower($2)) DESC,
                  updated_at DESC, title
         LIMIT $3"
    );
    let drawers = sqlx::query_as::<_, Drawer>(&sql)
        .bind(scope(room))
        .bind(query)
        .bind(i64::from(limit))
        .bind(language)
        .fetch_all(pool)
        .await?;
    Ok(drawers)
}

// The newest three repetitions, told oldest first.
async fn recent_reps(pool: &PgPool, cabinet_id: i64) -> Result<Vec<Rep>, AppError> {
    let mut reps = sqlx::query_as::<_, Rep>(
        "SELECT rep_number, occurred_on, how_it_went, portal_pull, lighter, source_path
         FROM anamnesis_reps
         WHERE cabinet_id = $1
         ORDER BY occurred_on DESC NULLS LAST, rep_number DESC, id DESC
         LIMIT 3",
    )
    .bind(cabinet_id)
    .fetch_all(pool)
    .await?;
    reps.reverse();
    Ok(reps)
}
