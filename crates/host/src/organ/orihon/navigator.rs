use super::{invalid, storage_error};
use crate::{
    HostConfig,
    judgment::{self, OrihonOption},
};
use hearth::orihon::{ReadRequest, Record};
use protocol::ProtocolErrorBody;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub query: String,
    pub credential: Option<String>,
    pub deadline: Option<u64>,
    pub exclude_session: Option<String>,
    pub limit: Option<u32>,
}

pub async fn navigate(
    host: &HostConfig,
    pool: Option<&PgPool>,
    request: Request,
) -> Result<Value, ProtocolErrorBody> {
    let limit = request.limit.unwrap_or(32);

    if limit == 0 {
        return Err(invalid("limit must be positive"));
    }

    let candidates = if host.akasha_enabled() {
        akasha::orihon_navigator::search_pg(
            pool.ok_or_else(|| invalid("Orihon database is unavailable"))?,
            &host.room,
            &request.query,
            request.exclude_session.as_deref(),
            limit,
        )
        .await
        .map_err(|_| invalid("Orihon candidate search failed"))?
    } else {
        let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
        let room = host.room.clone();
        let query = request.query.clone();
        let exclude_session = request.exclude_session.clone();
        tokio::task::spawn_blocking(move || {
            akasha::orihon_navigator::search_file(
                &directory,
                &room,
                &query,
                exclude_session.as_deref(),
                limit,
            )
        })
        .await
        .map_err(|_| invalid("Orihon file search did not complete"))?
        .map_err(|error| invalid(&error))?
    };
    let options: Vec<_> = candidates
        .candidates
        .iter()
        .map(|candidate| OrihonOption {
            seq: candidate.seq,
            text: candidate.text.clone(),
        })
        .collect();
    let judgment = if options.is_empty() {
        json!({"status":"no-match"})
    } else {
        judgment::choose_orihon(
            &host.room_dir,
            &host.room,
            &request.query,
            &options,
            request.credential.as_deref(),
            request.deadline,
        )
        .await
    };
    let selected: Vec<_> = judgment["selectedSeqs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .filter_map(|seq| {
            candidates
                .candidates
                .iter()
                .find(|candidate| candidate.seq == seq)
        })
        .collect();
    let pages = match selected.as_slice() {
        [first, second] => {
            let (a, b) = tokio::join!(
                select_page(host, pool, &request, first.turn_no),
                select_page(host, pool, &request, second.turn_no)
            );
            vec![a?, b?]
        }
        [first] => vec![select_page(host, pool, &request, first.turn_no).await?],
        _ => Vec::new(),
    };

    Ok(
        json!({"ok":true,"authority":"provenance","query":request.query,
        "found":!candidates.candidates.is_empty(),"selection":judgment,"pages":pages,
        "candidates":candidates.candidates,"complete":candidates.complete,"warnings":candidates.warnings,
        "instruction":"Conversation evidence is not memory or canon. If selection is local-context, choose a candidate and read its turnNo with orihon. If selection abstained, do not claim a matching page."}),
    )
}

async fn read_page(
    host: &HostConfig,
    pool: Option<&PgPool>,
    turn_no: i64,
) -> Result<Vec<Record>, ProtocolErrorBody> {
    let mut after_seq = None;
    let mut rows = Vec::new();
    let mut bytes = 0;

    loop {
        let request = ReadRequest {
            after_seq,
            until_seq: None,
            turn_no: Some(turn_no),
            day: None,
            limit: 200,
        };
        let result = if host.akasha_enabled() {
            origami::orihon::read_pg(
                pool.ok_or_else(|| invalid("Orihon database is unavailable"))?,
                &host.room,
                request,
            )
            .await
            .map_err(storage_error)?
        } else {
            let directory = host.room_dir.join(hearth::orihon::DIRECTORY);
            let room = host.room.clone();
            tokio::task::spawn_blocking(move || {
                origami::orihon::read_file(&directory, &room, request)
            })
            .await
            .map_err(|_| invalid("Orihon file read did not complete"))?
            .map_err(storage_error)?
        };

        for row in result.rows {
            bytes += row.text.len();

            if rows.len() == 4096 || bytes > 4 * 1024 * 1024 {
                return Err(invalid(
                    "Orihon page exceeds navigation bounds; read it with an explicit cursor",
                ));
            }
            rows.push(row);
        }
        if !result.has_more {
            break;
        }
        after_seq = result.next_after;
    }

    Ok(rows)
}

async fn select_page(
    host: &HostConfig,
    pool: Option<&PgPool>,
    request: &Request,
    turn_no: i64,
) -> Result<Value, ProtocolErrorBody> {
    let rows = read_page(host, pool, turn_no).await?;
    let mut selected = Vec::new();
    let mut receipts = Vec::new();

    for chunk in rows.chunks(254) {
        let options: Vec<_> = chunk
            .iter()
            .map(|row| OrihonOption {
                seq: row.seq,
                text: format!("{}: {}", row.speaker, row.text),
            })
            .collect();
        let receipt = judgment::choose_orihon(
            &host.room_dir,
            &host.room,
            &request.query,
            &options,
            request.credential.as_deref(),
            request.deadline,
        )
        .await;

        if !matches!(receipt["status"].as_str(), Some("selected" | "abstained")) {
            return Ok(
                json!({"turnNo":turn_no,"selection":receipt,"instruction":"Read this exact page with orihon; remote row selection was unavailable."}),
            );
        }
        selected.extend(
            receipt["selectedSeqs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64),
        );
        receipts.push(receipt);
    }
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|row| selected.contains(&row.seq))
        .collect();

    Ok(json!({"turnNo":turn_no,"authority":"provenance","selection":receipts,"rows":rows}))
}
