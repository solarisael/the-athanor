mod cli;
pub mod model;

pub use cli::run;

use chrono::Utc;
use hearth::orihon::{ReadRequest, Record};
use model::{FolderConfig, PROMPT_VERSION};
use origami::orihon::{
    self,
    folds::{self, Fold, SourceRef},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{error::Error, path::PathBuf};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_PAGE_RECORDS: usize = 4096;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTEXT_BYTES: usize = 32 * 1024;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const SUMMARY_BYTES: usize = 512;

pub enum Book {
    Postgres(PgPool),
    File(PathBuf),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub authority: &'static str,
    pub room: String,
    pub turn_no: i64,
    pub source_through: i64,
    pub source_records: usize,
    pub model_calls: usize,
    pub reused: usize,
    pub page: Fold,
}

impl Book {
    async fn page(&self, room: &str, turn_no: i64) -> Result<Vec<Record>> {
        let mut records = Vec::new();
        let mut after_seq = None;
        let mut bytes = 0;
        loop {
            let request = ReadRequest {
                after_seq,
                until_seq: None,
                turn_no: Some(turn_no),
                day: None,
                limit: hearth::orihon::MAX_READ_RECORDS,
            };
            let receipt = match self {
                Self::Postgres(pool) => orihon::read_pg(pool, room, request).await?,
                Self::File(directory) => orihon::read_file(directory, room, request)?,
            };
            for record in receipt.rows {
                bytes += record.text.len();
                if records.len() == MAX_PAGE_RECORDS || bytes > MAX_PAGE_BYTES {
                    return Err(
                        "Orihon page exceeds the bounded folder input; no page fold was written"
                            .into(),
                    );
                }
                records.push(record);
            }
            if !receipt.has_more {
                break;
            }
            after_seq = Some(
                receipt
                    .next_after
                    .ok_or("Orihon read omitted its continuation")?,
            );
        }
        if records.is_empty() {
            return Err("Orihon page does not exist".into());
        }
        Ok(records)
    }

    async fn existing(&self, fold: &Fold) -> Result<Option<Fold>> {
        let result = match self {
            Self::Postgres(pool) => {
                folds::read_pg(
                    pool,
                    &fold.room,
                    fold.level,
                    fold.index,
                    &fold.source_digest,
                    &fold.recipe_digest,
                )
                .await?
            }
            Self::File(directory) => folds::read_file(
                directory,
                &fold.room,
                fold.level,
                fold.index,
                &fold.source_digest,
                &fold.recipe_digest,
            )?,
        };
        Ok(result)
    }

    async fn save(&self, fold: &Fold) -> Result<Fold> {
        match self {
            Self::Postgres(pool) => Ok(folds::append_pg(pool, fold).await?),
            Self::File(directory) => Ok(folds::append_file(directory, fold)?),
        }
    }
}

fn recipe(config: &FolderConfig, context: &str) -> String {
    let value = serde_json::json!({
        "promptVersion": PROMPT_VERSION,
        "endpoint": config.endpoint.as_str(),
        "model": config.model,
        "numCtx": config.num_ctx,
        "context": context,
        "format": "orihon-message-page-v2",
    });
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

fn draft(room: &str, level: i16, index: i64, records: &[Record], recipe_digest: String) -> Fold {
    Fold {
        room: room.to_owned(),
        level,
        index,
        source_digest: folds::source_digest(records),
        recipe_digest,
        sources: records
            .iter()
            .map(|record| SourceRef {
                seq: record.seq,
                content_digest: record.content_digest.clone(),
            })
            .collect(),
        text: String::new(),
        model: None,
        tries: 0,
        built_at: Utc::now().to_rfc3339(),
    }
}

fn line(record: &Record, text: &str) -> String {
    #[derive(Serialize)]
    struct Line<'a> {
        kind: hearth::orihon::Kind,
        speaker: &'a str,
        text: &'a str,
    }

    let mut line = serde_json::to_string(&Line {
        kind: record.kind,
        speaker: &record.speaker,
        text,
    })
    .expect("in-memory source serialization");
    line.push('\n');
    line
}

fn append_context(context: &mut String, line: &str) {
    if line.len() > MAX_CONTEXT_BYTES {
        return;
    }
    while context.len() + line.len() > MAX_CONTEXT_BYTES {
        let end = context.find('\n').map_or(context.len(), |index| index + 1);
        context.drain(..end);
    }
    context.push_str(line);
}

/// Fold one explicitly selected page snapshot. Model calls never hold a database transaction.
pub async fn fold_page(
    book: &Book,
    config: &FolderConfig,
    room: &str,
    turn_no: i64,
) -> Result<Receipt> {
    if room.is_empty() || turn_no < 0 {
        return Err("Orihon folding requires a room and a nonnegative page ordinal".into());
    }
    let records = book.page(room, turn_no).await?;
    let source_through = records.last().ok_or("empty Orihon page")?.seq;
    let mut page = draft(room, 1, turn_no, &records, recipe(config, ""));

    if let Some(existing) = book.existing(&page).await? {
        return Ok(Receipt {
            authority: "derived-provenance",
            room: room.to_owned(),
            turn_no,
            source_through,
            source_records: records.len(),
            model_calls: 0,
            reused: 1,
            page: existing,
        });
    }

    let mut context = String::new();
    let mut page_input = String::new();
    let mut model_calls = 0;
    let mut reused = 0;
    for record in &records {
        let free = record.text.len() <= SUMMARY_BYTES && !record.text.is_empty();
        let input = if free {
            String::new()
        } else {
            line(record, &record.text)
        };
        let reference_context = if free {
            ""
        } else {
            model::fit_context(config, &context, &input)?
        };
        let mut leaf = draft(
            room,
            0,
            record.seq,
            std::slice::from_ref(record),
            recipe(config, reference_context),
        );
        let leaf = if let Some(existing) = book.existing(&leaf).await? {
            reused += 1;
            existing
        } else {
            if free {
                leaf.text = record.text.clone();
            } else {
                let summary = model::summarize(config, reference_context, &input).await?;
                model_calls += usize::from(summary.tries);
                leaf.text = summary.text;
                leaf.tries = summary.tries;
                leaf.model = Some(config.model.clone());
            }
            book.save(&leaf).await?
        };
        let line = line(record, &leaf.text);
        if page_input.len() + line.len() > MAX_INPUT_BYTES {
            return Err("Orihon page summaries exceed 64 KiB; message folds remain stored, no partial page fold was written".into());
        }
        page_input.push_str(&line);
        model::fit_context(config, "", &page_input)?;
        append_context(&mut context, &line);
    }
    let summary = model::summarize(config, "", &page_input).await?;
    model_calls += usize::from(summary.tries);
    page.text = summary.text;
    page.tries = summary.tries;
    page.model = Some(config.model.clone());
    let page = book.save(&page).await?;

    Ok(Receipt {
        authority: "derived-provenance",
        room: room.to_owned(),
        turn_no,
        source_through,
        source_records: records.len(),
        model_calls,
        reused,
        page,
    })
}
