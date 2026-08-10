use crate::backup;
use crate::config::{AppError, Config, HTTP_CLIENT, ROOM_KEY_RE};
use crate::remember::{default_backup, embed};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnamnesisParams {
    pub room: String,
    #[serde(default = "default_anamnesis_mode")]
    pub mode: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub limit: Option<u32>,
}
fn default_anamnesis_mode() -> String {
    "wake".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnamnesisSeed {
    pub number: i32,
    #[serde(default, alias = "occurredOn")]
    pub occurred_on: Option<NaiveDate>,
    #[serde(alias = "howItWent")]
    pub how_it_went: String,
    #[serde(alias = "portalPull")]
    pub portal_pull: String,
    pub lighter: String,
    #[serde(default, alias = "sourcePath")]
    pub source_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnamnesisWrite {
    pub room: String,
    pub operation: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub fidelity: Option<String>,
    #[serde(default)]
    pub activation: Option<String>,
    #[serde(default)]
    pub dormant: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub shape: Option<String>,
    pub ramp: Option<String>,
    pub counsel: Option<String>,
    pub peak: Option<String>,
    pub beginning: Option<String>,
    #[serde(default, alias = "verifyNote")]
    pub verify_note: Option<String>,
    #[serde(default, alias = "sourcePaths")]
    pub source_paths: Vec<String>,
    #[serde(default, alias = "canon")]
    pub canon_links: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, alias = "allowEmptyCycle")]
    pub allow_empty_cycle: bool,
    #[serde(default, alias = "seedRep")]
    pub seed_rep: Option<AnamnesisSeed>,
    #[serde(default = "default_backup")]
    pub backup: bool,
    #[serde(default, alias = "repNumber")]
    pub rep_number: Option<i32>,
    #[serde(default, alias = "occurredOn")]
    pub occurred_on: Option<NaiveDate>,
    #[serde(default, alias = "howItWent")]
    pub how_it_went: Option<String>,
    #[serde(default, alias = "portalPull")]
    pub portal_pull: Option<String>,
    pub lighter: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AnamnesisReceipt {
    pub operation: String,
    pub room: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(rename = "repNumber", skip_serializing_if = "Option::is_none")]
    pub rep_number: Option<i32>,
    pub durable: bool,
    pub authority: String,
    pub warnings: Vec<String>,
}

fn validate_anamnesis_room(room: &str) -> Result<(), AppError> {
    if !ROOM_KEY_RE.is_match(room) {
        return Err(AppError::Invalid("room must be a lowercase slug".into()));
    }
    Ok(())
}

impl AnamnesisParams {
    pub fn validate(&self) -> Result<(String, u32), AppError> {
        validate_anamnesis_room(&self.room)?;
        if self.mode != "wake" && self.mode != "consult" {
            return Err(AppError::Invalid(format!(
                "invalid anamnesis mode: {}",
                self.mode
            )));
        }
        if self.mode == "consult" && self.query.trim().is_empty() {
            return Err(AppError::Invalid(
                "consult requires a non-empty query".into(),
            ));
        }
        let limit = self.limit.unwrap_or(10).clamp(1, 50);
        Ok((self.mode.clone(), limit))
    }
}
async fn anamnesis_embedding(cfg: &Config, text: &str) -> Result<Option<String>, AppError> {
    if cfg.test_embedding_disabled {
        return Ok(None);
    }
    let url = cfg
        .embed_url
        .as_deref()
        .ok_or_else(|| AppError::Embedding("embedding endpoint is required".into()))?;
    let rows = embed(
        &HTTP_CLIENT,
        url,
        &cfg.embed_model,
        &[(text.to_owned(), 0, text.len(), None)],
        cfg.embed_dimension,
    )
    .await?;
    Ok(rows.first().map(|v| {
        format!(
            "[{}]",
            v.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }))
}
pub async fn anamnesis_write(
    pool: &PgPool,
    cfg: &Config,
    req: AnamnesisWrite,
) -> Result<AnamnesisReceipt, AppError> {
    validate_anamnesis_room(&req.room)?;
    let mut warnings = Vec::new();
    let mut tx = pool.begin().await?;
    let (id, kind, rep_number);
    match req.operation.as_str() {
        "add" => {
            let cabinet_kind = req
                .kind
                .as_deref()
                .ok_or_else(|| AppError::Invalid("kind is required".into()))?;
            if !["pillar", "cycle"].contains(&cabinet_kind) {
                return Err(AppError::Invalid("kind must be pillar or cycle".into()));
            }
            if cabinet_kind == "pillar" && req.seed_rep.is_some() {
                return Err(AppError::Invalid("pillar cannot include seedRep".into()));
            }
            let fidelity = req.fidelity.as_deref().unwrap_or("record");
            if !["record", "raw-material"].contains(&fidelity) {
                return Err(AppError::Invalid(
                    "fidelity must be record or raw-material".into(),
                ));
            }
            let activation = req.activation.as_deref().unwrap_or("fork");
            if !["wake", "fork"].contains(&activation) {
                return Err(AppError::Invalid("activation must be wake or fork".into()));
            }
            if req.title.trim().is_empty() {
                return Err(AppError::Invalid("title is required".into()));
            }
            let ramp = req.ramp.as_deref().unwrap_or("");
            if ramp.trim().is_empty() {
                return Err(AppError::Invalid("ramp is required".into()));
            }
            if cabinet_kind == "cycle" && !req.allow_empty_cycle && req.seed_rep.is_none() {
                return Err(AppError::Invalid(
                    "cycle requires seedRep unless allowEmptyCycle".into(),
                ));
            }
            if cabinet_kind == "cycle"
                && activation == "wake"
                && req.verify_note.as_deref().unwrap_or("").trim().is_empty()
            {
                return Err(AppError::Invalid("wake cycle requires verifyNote".into()));
            }
            let embedding = anamnesis_embedding(
                cfg,
                &[
                    &req.title,
                    req.shape.as_deref().unwrap_or(""),
                    ramp,
                    req.counsel.as_deref().unwrap_or(""),
                    req.peak.as_deref().unwrap_or(""),
                ]
                .join("\n"),
            )
            .await?;
            if embedding.is_none() && cfg.test_embedding_disabled {
                warnings
                    .push("embedding disabled for isolated test; cabinet embedding omitted".into());
            }
            id = sqlx::query_scalar::<_, i64>("INSERT INTO anamnesis (room,kind,fidelity,activation,active,title,shape,peak,beginning,ramp,counsel,verify_note,source_paths,canon_links,tags,body_embedding,embedded_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16::vector,CASE WHEN $16 IS NULL THEN NULL ELSE NOW() END) ON CONFLICT (room,title) DO UPDATE SET kind=EXCLUDED.kind,fidelity=EXCLUDED.fidelity,activation=EXCLUDED.activation,active=EXCLUDED.active,shape=EXCLUDED.shape,peak=EXCLUDED.peak,beginning=EXCLUDED.beginning,ramp=EXCLUDED.ramp,counsel=EXCLUDED.counsel,verify_note=EXCLUDED.verify_note,source_paths=EXCLUDED.source_paths,canon_links=EXCLUDED.canon_links,tags=EXCLUDED.tags,body_embedding=EXCLUDED.body_embedding,embedded_at=EXCLUDED.embedded_at RETURNING id")
                .bind(&req.room).bind(cabinet_kind).bind(fidelity).bind(activation).bind(!req.dormant).bind(&req.title).bind(&req.shape).bind(&req.peak).bind(&req.beginning).bind(ramp).bind(&req.counsel).bind(&req.verify_note).bind(&req.source_paths).bind(&req.canon_links).bind(&req.tags).bind(embedding).fetch_one(&mut *tx).await?;
            if let Some(seed) = req.seed_rep {
                sqlx::query("INSERT INTO anamnesis_reps (cabinet_id,rep_number,occurred_on,how_it_went,portal_pull,lighter,source_path) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (cabinet_id,rep_number) DO UPDATE SET occurred_on=EXCLUDED.occurred_on,how_it_went=EXCLUDED.how_it_went,portal_pull=EXCLUDED.portal_pull,lighter=EXCLUDED.lighter,source_path=EXCLUDED.source_path")
                    .bind(id).bind(seed.number).bind(seed.occurred_on).bind(seed.how_it_went).bind(seed.portal_pull).bind(seed.lighter).bind(seed.source_path).execute(&mut *tx).await?;
            }
            kind = Some(cabinet_kind.to_string());
            rep_number = None;
        }
        "append-rep" => {
            let number = req
                .rep_number
                .ok_or_else(|| AppError::Invalid("append-rep requires repNumber".into()))?;
            let how = req
                .how_it_went
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| AppError::Invalid("append-rep requires howItWent".into()))?;
            let portal = req
                .portal_pull
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| AppError::Invalid("append-rep requires portalPull".into()))?;
            let lighter = req
                .lighter
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| AppError::Invalid("append-rep requires lighter".into()))?;
            let title = req.title.trim();
            if title.is_empty() {
                return Err(AppError::Invalid("title is required".into()));
            }
            id = sqlx::query_scalar::<_, i64>(
                "SELECT id FROM anamnesis WHERE room=$1 AND title=$2 AND kind='cycle'",
            )
            .bind(&req.room)
            .bind(title)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::Invalid("append-rep target cycle not found".into()))?;
            sqlx::query("INSERT INTO anamnesis_reps (cabinet_id,rep_number,occurred_on,how_it_went,portal_pull,lighter,source_path) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (cabinet_id,rep_number) DO UPDATE SET occurred_on=EXCLUDED.occurred_on,how_it_went=EXCLUDED.how_it_went,portal_pull=EXCLUDED.portal_pull,lighter=EXCLUDED.lighter,source_path=EXCLUDED.source_path")
                .bind(id).bind(number).bind(req.occurred_on).bind(how).bind(portal).bind(lighter).bind(req.source_paths.first()).execute(&mut *tx).await?;
            kind = Some("cycle".into());
            rep_number = Some(number);
        }
        _ => {
            return Err(AppError::Invalid(
                "operation must be add or append-rep".into(),
            ));
        }
    }
    tx.commit().await?;
    if req.backup
        && let Err(error) = backup::run_post_write(pool, &cfg.database_url).await
    {
        warnings.push(format!("backup failed: {error}"));
    }
    Ok(AnamnesisReceipt {
        operation: req.operation,
        room: req.room,
        title: req.title,
        kind,
        rep_number,
        durable: true,
        authority: "substrate".into(),
        warnings,
    })
}

#[derive(Debug, Serialize)]
pub struct AnamnesisResult {
    pub ok: bool,
    pub mode: String,
    pub room: String,
    pub query: String,
    pub found: bool,
    pub entries: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
}

pub async fn anamnesis(
    pool: &PgPool,
    params: AnamnesisParams,
) -> Result<AnamnesisResult, AppError> {
    let (mode, limit) = params.validate()?;
    let rooms = if params.room == "house" {
        vec!["house".to_string()]
    } else {
        vec![params.room.clone(), "house".to_string()]
    };
    let rows = if mode == "wake" {
        sqlx::query("SELECT id,room,kind,fidelity,activation,active,title,shape,peak,beginning,ramp,counsel,verify_note,source_paths,canon_links,tags FROM anamnesis WHERE room=ANY($1::text[]) AND ((kind='pillar' AND activation='wake') OR (kind='cycle' AND activation='wake' AND active)) ORDER BY CASE WHEN kind='pillar' THEN 0 ELSE 1 END,updated_at DESC,id DESC LIMIT $2").bind(&rooms).bind(limit as i64).fetch_all(pool).await?
    } else {
        sqlx::query("SELECT id,room,kind,fidelity,activation,active,title,shape,peak,beginning,ramp,counsel,verify_note,source_paths,canon_links,tags FROM anamnesis WHERE room=ANY($1::text[]) AND (body_tsv @@ plainto_tsquery('portuguese',$2) OR lower(title||' '||coalesce(shape,'')||' '||ramp||' '||coalesce(counsel,'')||' '||coalesce(peak,'')||' '||array_to_string(canon_links,' ')||' '||array_to_string(tags,' ')) LIKE '%'||lower($2)||'%') ORDER BY (ts_rank_cd(body_tsv, plainto_tsquery('portuguese',$2)) * 10 + similarity(lower(title||' '||coalesce(shape,'')||' '||ramp||' '||coalesce(counsel,'')||' '||coalesce(peak,'')||' '||array_to_string(canon_links,' ')||' '||array_to_string(tags,' ')), lower($2))) DESC, updated_at DESC,title LIMIT $3").bind(&rooms).bind(&params.query).bind(limit as i64).fetch_all(pool).await?
    };
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let kind: String = row.try_get("kind")?;
        let verify: Option<String> = row.try_get("verify_note")?;
        if mode == "wake" && kind == "cycle" && verify.as_deref().unwrap_or("").trim().is_empty() {
            warnings.push(format!("excluded cycle {id}: blank verify_note"));
            continue;
        }
        let reps = sqlx::query("SELECT rep_number,occurred_on,how_it_went,portal_pull,lighter,source_path FROM anamnesis_reps WHERE cabinet_id=$1 ORDER BY occurred_on DESC NULLS LAST,rep_number DESC,id DESC LIMIT 3").bind(id).fetch_all(pool).await?;
        let reps = reps.into_iter().rev().map(|r| serde_json::json!({"rep_number":r.try_get::<i32,_>("rep_number").unwrap_or_default(),"occurred_on":r.try_get::<Option<NaiveDate>,_>("occurred_on").ok().flatten().map(|d|d.to_string()),"how_it_went":r.try_get::<String,_>("how_it_went").unwrap_or_default(),"portal_pull":r.try_get::<Option<String>,_>("portal_pull").ok().flatten(),"lighter":r.try_get::<Option<String>,_>("lighter").ok().flatten(),"source_path":r.try_get::<Option<String>,_>("source_path").ok().flatten()})).collect::<Vec<_>>();
        entries.push(serde_json::json!({"id":id,"room":row.try_get::<String,_>("room")?,"kind":kind,"fidelity":row.try_get::<String,_>("fidelity")?,"activation":row.try_get::<String,_>("activation")?,"active":row.try_get::<bool,_>("active")?,"title":row.try_get::<String,_>("title")?,"shape":row.try_get::<Option<String>,_>("shape")?,"peak":row.try_get::<Option<String>,_>("peak")?,"beginning":row.try_get::<Option<String>,_>("beginning")?,"ramp":row.try_get::<String,_>("ramp")?,"counsel":row.try_get::<Option<String>,_>("counsel")?,"verify_note":verify,"source_paths":row.try_get::<Vec<String>,_>("source_paths")?,"canon_links":row.try_get::<Vec<String>,_>("canon_links")?,"tags":row.try_get::<Vec<String>,_>("tags")?,"reps":reps}));
    }
    Ok(AnamnesisResult {
        ok: true,
        mode,
        room: params.room,
        query: params.query,
        found: !entries.is_empty(),
        entries,
        warnings,
    })
}
