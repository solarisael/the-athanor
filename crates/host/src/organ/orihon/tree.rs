use super::{invalid, storage_error};
use crate::HostConfig;
use hearth::orihon::ReadRequest;
use origami::orihon::folds::{self, Fold, FoldRef};
use protocol::ProtocolErrorBody;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UnfoldRequest {
    pub reference: FoldRef,
    pub after_seq: Option<i64>,
    pub limit: Option<usize>,
}

async fn read_fold(
    host: &HostConfig,
    pool: Option<&PgPool>,
    reference: &FoldRef,
) -> Result<Fold, ProtocolErrorBody> {
    let fold = if host.akasha_enabled() {
        folds::read_pg(
            pool.ok_or_else(|| invalid("Orihon database is unavailable"))?,
            &host.room,
            reference.level,
            reference.index,
            &reference.source_digest,
            &reference.recipe_digest,
        )
        .await
        .map_err(storage_error)?
    } else {
        let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
        let room = host.room.clone();
        let reference = reference.clone();
        tokio::task::spawn_blocking(move || {
            folds::read_file(
                &directory,
                &room,
                reference.level,
                reference.index,
                &reference.source_digest,
                &reference.recipe_digest,
            )
        })
        .await
        .map_err(|_| invalid("Orihon fold read did not complete"))?
        .map_err(storage_error)?
    };

    fold.ok_or_else(|| invalid("The exact fold revision does not exist in this room"))
}

pub async fn unfold(
    host: &HostConfig,
    pool: Option<&PgPool>,
    request: UnfoldRequest,
) -> Result<Value, ProtocolErrorBody> {
    let fold = read_fold(host, pool, &request.reference).await?;

    if fold.level >= 2 {
        if request.after_seq.is_some() || request.limit.is_some() {
            return Err(invalid(
                "Source cursors apply only to a page or message fold",
            ));
        }
        let left = read_fold(host, pool, &fold.children[0]).await?;
        let right = read_fold(host, pool, &fold.children[1]).await?;

        return Ok(
            json!({"ok":true,"authority":"derived-provenance","reference":request.reference,
            "children":[{"reference":left.reference(),"text":left.text},{"reference":right.reference(),"text":right.text}]}),
        );
    }
    let first = fold
        .sources
        .first()
        .ok_or_else(|| invalid("Fold has no source references"))?
        .seq;
    let through = fold
        .sources
        .last()
        .ok_or_else(|| invalid("Fold has no source references"))?
        .seq;

    if request
        .after_seq
        .is_some_and(|seq| seq < first.saturating_sub(1) || seq > through)
    {
        return Err(invalid(
            "Source cursor is outside the selected fold revision",
        ));
    }
    let read = ReadRequest {
        after_seq: request
            .after_seq
            .or_else(|| first.checked_sub(1).filter(|seq| *seq >= 0)),
        until_seq: Some(through),
        turn_no: (fold.level == 1).then_some(fold.index),
        day: None,
        limit: request.limit.unwrap_or(50),
    };
    let receipt = if host.akasha_enabled() {
        origami::orihon::read_pg(
            pool.ok_or_else(|| invalid("Orihon database is unavailable"))?,
            &host.room,
            read,
        )
        .await
        .map_err(storage_error)?
    } else {
        let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
        let room = host.room.clone();
        tokio::task::spawn_blocking(move || origami::orihon::read_file(&directory, &room, read))
            .await
            .map_err(|_| invalid("Orihon source read did not complete"))?
            .map_err(storage_error)?
    };

    for row in &receipt.rows {
        if !fold
            .sources
            .iter()
            .any(|source| source.seq == row.seq && source.content_digest == row.content_digest)
        {
            return Err(invalid(
                "Orihon source no longer matches the selected fold revision",
            ));
        }
    }

    Ok(
        json!({"ok":true,"authority":"provenance","reference":request.reference,
        "rows":receipt.rows,"hasMore":receipt.has_more,"nextAfter":receipt.next_after}),
    )
}
