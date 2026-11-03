//! `orihon_zoom` and `orihon_date`: exact reads of the live tree, its units and their stored
//! records. Nothing here is generated: every text is a stored node, a stored record slice
//! checked against its unit digest, or a raw record page.

use super::{invalid, storage_error};
use crate::HostConfig;
use hearth::orihon::{Kind, Record};
use origami::orihon::live::{
    NodeKey, Unit, kind_name, sha256_hex,
    store::{self, Store},
    unit_body,
};
use protocol::ProtocolErrorBody;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

/// One page of a record's exact audit text, cut back to a character boundary.
const RAW_PAGE_BYTES: usize = 30_000;
/// Units are stored and bound as PostgreSQL bigint.
const MAX_UNIT: u64 = i64::MAX as u64;

/// `{id, n}`, `{id, n: 1, rawOffset}` or `{name}`; any other combination is refused.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub id: Option<u64>,
    pub n: Option<u64>,
    pub name: Option<String>,
    pub raw_offset: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DateRequest {
    pub id: u64,
}

pub async fn zoom(
    host: &HostConfig,
    pool: Option<&PgPool>,
    request: Request,
) -> Result<Value, ProtocolErrorBody> {
    let store = open(host, pool)?;
    let room = host.room.as_str();

    match request {
        Request {
            id: None,
            n: None,
            name: Some(name),
            raw_offset: None,
        } => children(&store, room, &name).await,
        Request {
            id: Some(id),
            n: Some(1),
            name: None,
            raw_offset: Some(offset),
        } => raw_page(&store, room, id, offset).await,
        Request {
            id: Some(id),
            n: Some(1),
            name: None,
            raw_offset: None,
        } => message(&store, room, id).await,
        Request {
            id: Some(id),
            n: Some(n),
            name: None,
            raw_offset: None,
        } => node(&store, room, id, n).await,
        _ => Err(invalid(
            "zoom takes exactly {id, n}, {id, n: 1, rawOffset} or {name}",
        )),
    }
}

pub async fn date(
    host: &HostConfig,
    pool: Option<&PgPool>,
    request: DateRequest,
) -> Result<Value, ProtocolErrorBody> {
    span(request.id, 1)?;
    let store = open(host, pool)?;
    let (_, record) = read_unit(&store, &host.room, request.id).await?;

    Ok(json!({
        "ok": true,
        "id": request.id,
        "seq": record.seq,
        "saidAt": record.said_at,
        "timestampObserved": record.timestamp_observed,
        "writtenAt": record.written_at,
    }))
}

fn open(host: &HostConfig, pool: Option<&PgPool>) -> Result<Store, ProtocolErrorBody> {
    if host.akasha_enabled() {
        let pool = pool.ok_or_else(|| invalid("Orihon database is unavailable"))?;
        return Ok(Store::Postgres(pool.clone()));
    }
    Ok(Store::File(host.room_dir.join(hearth::orihon::DIRECTORY)))
}

/// The node `id+n`. `from_span` only divides and shifts right; the span end is checked here
/// before any key arithmetic adds to it.
fn span(id: u64, n: u64) -> Result<NodeKey, ProtocolErrorBody> {
    let key = NodeKey::from_span(id, n).ok_or_else(|| {
        invalid("zoom n must be a power of two up to 2^62 and id a multiple of n")
    })?;
    id.checked_add(n - 1)
        .filter(|last| *last <= MAX_UNIT)
        .ok_or_else(|| invalid("zoom id+n is outside the unit range"))?;
    Ok(key)
}

/// A node exists once every unit it covers is stored; units are numbered without gaps.
async fn exists(store: &Store, room: &str, key: NodeKey) -> Result<bool, ProtocolErrorBody> {
    let last = key.last();
    if last > MAX_UNIT {
        return Ok(false);
    }
    Ok(store::read_unit(store, room, last)
        .await
        .map_err(storage_error)?
        .is_some())
}

/// The parent line, or null while the units it would cover are not all stored.
async fn parent(store: &Store, room: &str, key: NodeKey) -> Result<Value, ProtocolErrorBody> {
    let Some(parent) = key.parent() else {
        return Ok(Value::Null);
    };
    if !exists(store, room, parent).await? {
        return Ok(Value::Null);
    }
    Ok(json!({"id": parent.first(), "n": parent.count()}))
}

async fn node_text(
    store: &Store,
    room: &str,
    key: NodeKey,
) -> Result<Option<String>, ProtocolErrorBody> {
    Ok(store::read_node(store, room, key)
        .await
        .map_err(storage_error)?
        .map(|node| node.text))
}

fn book_mismatch() -> ProtocolErrorBody {
    ProtocolErrorBody::application(
        "invalid_book",
        "the stored unit no longer matches its record",
    )
    .build()
}

/// The stored unit `id` and its record, checked against each other.
async fn read_unit(
    store: &Store,
    room: &str,
    id: u64,
) -> Result<(Unit, Record), ProtocolErrorBody> {
    let (unit, record) = store::read_unit(store, room, id)
        .await
        .map_err(storage_error)?
        .ok_or_else(|| invalid("zoom id is not a message of this room"))?;
    if unit.unit != id
        || unit.seq != record.seq
        || unit.record_digest != record.content_digest
        || unit.part >= unit.parts
        || unit.semantic != record.fold_text.is_some()
    {
        return Err(book_mismatch());
    }
    Ok((unit, record))
}

/// `{id, n > 1}`: the line and the two lines it was made from.
async fn node(store: &Store, room: &str, id: u64, n: u64) -> Result<Value, ProtocolErrorBody> {
    let key = span(id, n)?;
    let [left, right] = key
        .children()
        .ok_or_else(|| invalid("zoom n must be 1 or a larger power of two"))?;
    if !exists(store, room, key).await? {
        return Err(invalid("zoom id+n is not a line of this room's tree"));
    }
    let text = node_text(store, room, key).await?;
    let mut children = Vec::with_capacity(2);
    for child in [left, right] {
        let text = node_text(store, room, child).await?;
        let built = text.is_some();
        children.push(json!({
            "id": child.first(),
            "n": child.count(),
            "text": text,
            "built": built,
        }));
    }

    Ok(json!({
        "ok": true,
        "node": {"id": key.first(), "n": key.count(), "text": text},
        "parent": parent(store, room, key).await?,
        "children": children,
    }))
}

/// `{id, n: 1}`: the whole unit, its exact text and its record's provenance.
async fn message(store: &Store, room: &str, id: u64) -> Result<Value, ProtocolErrorBody> {
    let key = span(id, 1)?;
    let (unit, record) = read_unit(store, room, id).await?;
    let text = unit_body(&record)
        .get(unit.byte_start..unit.byte_end)
        .filter(|text| sha256_hex(text.as_bytes()) == unit.digest)
        .ok_or_else(book_mismatch)?;
    // The record's units tile it in order, so its range follows from this unit's part.
    let first = id
        .checked_sub(u64::from(unit.part))
        .ok_or_else(book_mismatch)?;
    let last = first
        .checked_add(u64::from(unit.parts) - 1)
        .filter(|last| *last <= MAX_UNIT)
        .ok_or_else(book_mismatch)?;

    Ok(json!({
        "ok": true,
        "unit": {
            "id": id,
            "part": unit.part + 1,
            "parts": unit.parts,
            "units": [first, last],
            "semantic": unit.semantic,
        },
        "text": text,
        "record": {
            "seq": record.seq,
            "kind": kind_name(record.kind),
            "speaker": record.speaker,
            "saidAt": record.said_at,
            "sourceSession": record.source_session,
            "sourceId": record.source_id,
            "contentDigest": record.content_digest,
            "truncated": record.truncated,
            "redacted": record.redacted,
            "children": record.children,
            "textBytes": record.text.len(),
        },
        "parent": parent(store, room, key).await?,
    }))
}

/// `{id, n: 1, rawOffset}`: one page of the record's exact audit text, which is what a
/// `foldText` unit was summarized in place of.
async fn raw_page(
    store: &Store,
    room: &str,
    id: u64,
    offset: u64,
) -> Result<Value, ProtocolErrorBody> {
    span(id, 1)?;
    let (_, record) = read_unit(store, room, id).await?;
    let text = record.text.as_str();
    let offset = usize::try_from(offset)
        .ok()
        .filter(|offset| (*offset == 0 || *offset < text.len()) && text.is_char_boundary(*offset))
        .ok_or_else(|| {
            invalid("rawOffset must be 0 or a character boundary inside the record text")
        })?;
    let mut end = text.len().min(offset + RAW_PAGE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }

    Ok(json!({
        "ok": true,
        "seq": record.seq,
        "offset": offset,
        "nextOffset": (end < text.len()).then_some(end),
        "text": &text[offset..end],
    }))
}

/// `{name}`: the exact child sessions stored work records name, each linked to the first unit
/// of its report. The harness resolves and verifies the sessions; no path is read here.
async fn children(store: &Store, room: &str, name: &str) -> Result<Value, ProtocolErrorBody> {
    let reports = store::child_reports(store, room, name)
        .await
        .map_err(storage_error)?;
    let mut children = Vec::with_capacity(reports.len());
    for (record, child) in reports {
        if record.kind != Kind::Work {
            continue;
        }
        let report_unit = store::first_unit(store, room, record.seq)
            .await
            .map_err(storage_error)?;
        children.push(json!({
            "session": child.session,
            "reportUnit": report_unit,
            "seq": record.seq,
            "saidAt": record.said_at,
            "sourceSession": record.source_session,
        }));
    }

    Ok(json!({"ok": true, "name": name, "children": children}))
}
