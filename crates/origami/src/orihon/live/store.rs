//! Live Orihon evidence: units, nodes, jobs, persisted views, the room compactor configuration
//! and harness prefixes. PostgreSQL is authoritative when the Host has a database; otherwise
//! the room book directory holds the same evidence under `live/`.

use super::{
    LINE_BYTES, MODEL_LINE_CEILING, Method, Node, NodeKey, Unit, View, branch_source_digest,
    node_digest, sha256_hex, unit_body, units_for,
};
use crate::orihon::error::{Error, invalid, io, malformed, refusal};
use chrono::{SecondsFormat, Utc};
use hearth::orihon::{ChildRef, FolderSettings, Kind, ReadRequest, Record};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

const LIVE_DIRECTORY: &str = "live";
const LOCK_FILE: &str = ".live.lock";
const INGEST_RECORDS: usize = hearth::orihon::MAX_BATCH_RECORDS;
const MAX_LINE_BYTES: usize = 2 * 1024 * 1024;

/// One harness tool definition, sent to the compactor and never executed by it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// The exact stable system prompt parts and tool definitions the harness sends for turns.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Prefix {
    pub system: Vec<String>,
    pub tools: Vec<Tool>,
}

impl Prefix {
    /// Adapter-visible prefix identity. Provider-added blocks are outside it.
    pub fn digest(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).expect("prefix serializes"))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LiveConfig {
    pub folder: Option<FolderSettings>,
    pub prefix_digest: Option<String>,
}

/// Token counts one response reported. `None` is unreported, never zero.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

impl Usage {
    /// Sums every attempt. A field stays unknown when any attempt did not report it.
    pub fn total<'a>(attempts: impl IntoIterator<Item = &'a Usage>) -> Self {
        let sum = |left: Option<u64>, right: Option<u64>| Some(left?.saturating_add(right?));
        attempts.into_iter().fold(
            Self {
                input_tokens: Some(0),
                output_tokens: Some(0),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
            },
            |total, usage| Self {
                input_tokens: sum(total.input_tokens, usage.input_tokens),
                output_tokens: sum(total.output_tokens, usage.output_tokens),
                cache_read_tokens: sum(total.cache_read_tokens, usage.cache_read_tokens),
                cache_write_tokens: sum(total.cache_write_tokens, usage.cache_write_tokens),
            },
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Attempt {
    pub attempt: u32,
    /// `accepted`, `too_long`, `failed`, or `cancelled`.
    pub outcome: String,
    pub bytes: Option<usize>,
    pub usage: Usage,
    /// Exactly one of `cost_usd` and `cost_reason` is present.
    pub cost_usd: Option<f64>,
    pub cost_reason: Option<String>,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Accepted,
    Failed,
    Cancelled,
}

impl Outcome {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One finished compaction job with every attempt it paid for.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Job {
    pub id: Option<i64>,
    pub key: NodeKey,
    pub outcome: Outcome,
    pub backend: String,
    pub provider: String,
    pub model: String,
    pub cache_scope: String,
    pub prefix_digest: String,
    pub context_digest: String,
    pub context_lines: u32,
    pub context_bytes: u32,
    pub input_bytes: u32,
    pub attempts: Vec<Attempt>,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
    pub cost_reason: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: String,
}

/// The exact text a leaf compresses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeafSource {
    pub kind: Kind,
    pub unit: Unit,
    pub text: String,
}

/// One accepted record of a harness session: enough to place a turn boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRecord {
    pub seq: i64,
    pub source_id: String,
    pub kind: Kind,
    pub parent_user_id: Option<String>,
}

impl SessionRecord {
    fn of(record: &Record) -> Self {
        Self {
            seq: record.seq,
            source_id: record.source_id.clone(),
            kind: record.kind,
            parent_user_id: record.parent_user_id.clone(),
        }
    }
}

pub enum Store {
    Postgres(PgPool),
    /// The room book directory, `<room>/orihon`.
    File(PathBuf),
}

impl Clone for Store {
    fn clone(&self) -> Self {
        match self {
            Self::Postgres(pool) => Self::Postgres(pool.clone()),
            Self::File(path) => Self::File(path.clone()),
        }
    }
}

/// What the owner loads once at start. The views are the persisted ones, never rebuilt.
pub struct Snapshot {
    pub units: u64,
    pub ingested_seq: i64,
    pub nodes: HashMap<NodeKey, usize>,
    pub chat: Option<View>,
    pub compactor: Option<View>,
    pub config: LiveConfig,
}

// Ownership: one process builds a room's tree. The PostgreSQL lock lives on a detached
// connection that closes on drop; the file lock closes with its handle.
struct OwnerLock {
    _file: Option<File>,
    connection: Option<sqlx::PgConnection>,
}

/// The owner's handle: storage plus the exclusive room lock.
pub struct Live {
    store: Store,
    room: String,
    _lock: OwnerLock,
    file: Option<FileState>,
}

// enough: file rooms keep node texts, unit ends, unbuilt leaf sources and each session's
// ingested record places in memory; move the room to PostgreSQL when the book outgrows the
// Host's memory.
struct FileState {
    directory: PathBuf,
    texts: HashMap<NodeKey, String>,
    digests: HashMap<NodeKey, String>,
    unit_digests: Vec<String>,
    unit_ends: BTreeMap<i64, u64>,
    leaves: HashMap<u64, LeafSource>,
    sessions: HashMap<(String, String), Vec<SessionRecord>>,
    next_job: i64,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> impl std::future::Future<Output = Result<T, Error>> {
    async move {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|_| invalid("Orihon live file operation did not complete"))?
    }
}

fn live_directory(book: &Path) -> PathBuf {
    book.join(LIVE_DIRECTORY)
}

fn append_line(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    append_batch(path, std::slice::from_ref(value))
}

fn append_batch(path: &Path, values: &[impl Serialize]) -> Result<(), Error> {
    if values.is_empty() {
        return Ok(());
    }

    let mut bytes = Vec::new();
    for value in values {
        serde_json::to_writer(&mut bytes, value)
            .map_err(|_| invalid("live line cannot be serialized"))?;
        bytes.push(b'\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| io(path, error))?;
    file.write_all(&bytes).map_err(|error| io(path, error))?;
    file.sync_all().map_err(|error| io(path, error))
}

fn write_atomic(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| invalid("live state cannot be serialized"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = File::create(&temporary).map_err(|error| io(&temporary, error))?;
    file.write_all(&bytes)
        .map_err(|error| io(&temporary, error))?;
    file.sync_all().map_err(|error| io(&temporary, error))?;
    drop(file);
    fs::rename(&temporary, path).map_err(|error| io(path, error))
}

fn read_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io(path, error)),
    };
    let mut values = Vec::new();
    for (index, line) in BufReader::new(file).split(b'\n').enumerate() {
        let line = line.map_err(|error| io(path, error))?;
        if line.len() > MAX_LINE_BYTES {
            return Err(malformed(path, index + 1));
        }
        values.push(serde_json::from_slice(&line).map_err(|_| malformed(path, index + 1))?);
    }
    Ok(values)
}

fn truncate_tail(path: &Path, length: u64) -> Result<(), Error> {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| io(path, error))?;
    let dropped = file.metadata().map_err(|error| io(path, error))?.len() - length;
    file.set_len(length).map_err(|error| io(path, error))?;
    file.sync_all().map_err(|error| io(path, error))?;
    eprintln!(
        "Orihon repaired derived log {}: dropped {dropped} trailing bytes",
        path.display()
    );
    Ok(())
}

// Only the exclusive owner may repair a derived log. Readers never truncate files.
fn owner_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<(T, u64)>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io(path, error)),
    };
    let mut reader = BufReader::new(file);
    let mut values = Vec::new();
    let mut offset = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        let count = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| io(path, error))?;

        if count == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            truncate_tail(path, offset)?;
            break;
        }
        if line.len() > MAX_LINE_BYTES {
            return Err(malformed(path, values.len() + 1));
        }

        let value = serde_json::from_slice(&line).map_err(|_| malformed(path, values.len() + 1))?;
        values.push((value, offset));
        offset += count as u64;
    }
    Ok(values)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, Error> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| malformed(path, 1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(path, error)),
    }
}

fn tree_paths(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let tree = directory.join("tree");
    let entries = match fs::read_dir(&tree) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io(&tree, error)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| io(&tree, error))?.path();
        if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Views {
    chat: View,
    compactor: View,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Checks one node against its geometry, exact sources, and text, as PostgreSQL does on insert.
fn check_node(
    node: &Node,
    unit_digest: impl Fn(u64) -> Option<String>,
    child_digest: impl Fn(NodeKey) -> Option<String>,
) -> bool {
    let expected = match node.key.children() {
        None => unit_digest(node.key.index),
        Some([left, right]) => match (child_digest(left), child_digest(right)) {
            (Some(left), Some(right)) => Some(branch_source_digest(&left, &right)),
            _ => None,
        },
    };
    let method = match node.method {
        Method::Verbatim => node.key.level == 0 && node.job.is_none(),
        Method::Joined => node.key.level > 0 && node.job.is_none(),
        Method::Model => node.job.is_some(),
    };
    method
        && !node.text.is_empty()
        && node.text.len()
            <= if node.method == Method::Model {
                MODEL_LINE_CEILING
            } else {
                LINE_BYTES
            }
        && expected.as_deref() == Some(node.source_digest.as_str())
        && node.digest == node_digest(node.key, &node.source_digest, &node.text)
}

fn record_from_book(directory: &Path, room: &str, seq: i64) -> Result<Option<Record>, Error> {
    let receipt = crate::orihon::read_file(
        directory,
        room,
        ReadRequest {
            after_seq: (seq > 0).then(|| seq - 1),
            until_seq: Some(seq),
            turn_no: None,
            day: None,
            limit: 1,
        },
    )?;
    Ok(receipt.rows.into_iter().find(|record| record.seq == seq))
}

fn book_records(directory: &Path, room: &str, after_seq: i64) -> Result<Vec<Record>, Error> {
    let mut records = Vec::new();
    let mut after = (after_seq >= 0).then_some(after_seq);
    loop {
        let receipt = crate::orihon::read_file(
            directory,
            room,
            ReadRequest {
                after_seq: after,
                until_seq: None,
                turn_no: None,
                day: None,
                limit: hearth::orihon::MAX_READ_RECORDS,
            },
        )?;
        records.extend(receipt.rows);
        if !receipt.has_more {
            return Ok(records);
        }
        after = receipt.next_after;
    }
}

fn leaf_sources(record: &Record, units: &[Unit]) -> Vec<LeafSource> {
    let body = unit_body(record);
    units
        .iter()
        .map(|unit| LeafSource {
            kind: record.kind,
            unit: unit.clone(),
            text: format!(
                "{}: {}",
                record.speaker,
                &body[unit.byte_start..unit.byte_end]
            ),
        })
        .collect()
}

impl Store {
    async fn lock_owner(&self, room: &str) -> Result<Option<OwnerLock>, Error> {
        match self {
            Self::Postgres(pool) => {
                let mut connection = pool.acquire().await?.detach();
                let acquired: bool = sqlx::query_scalar(
                    "SELECT pg_try_advisory_lock(hashtextextended('orihon-live:' || $1, 0))",
                )
                .bind(room)
                .fetch_one(&mut connection)
                .await?;
                Ok(acquired.then_some(OwnerLock {
                    _file: None,
                    connection: Some(connection),
                }))
            }
            Self::File(book) => {
                let directory = live_directory(book);
                blocking(move || {
                    fs::create_dir_all(&directory).map_err(|error| io(&directory, error))?;
                    let path = directory.join(LOCK_FILE);
                    let file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(&path)
                        .map_err(|error| io(&path, error))?;
                    match file.try_lock() {
                        Ok(()) => Ok(Some(OwnerLock {
                            _file: Some(file),
                            connection: None,
                        })),
                        Err(fs::TryLockError::WouldBlock) => Ok(None),
                        Err(fs::TryLockError::Error(error)) => Err(io(&path, error)),
                    }
                })
                .await
            }
        }
    }
}

impl Live {
    /// Takes the room's exclusive owner lock and loads the persisted state once.
    /// `None` means another process owns this room's chat.
    pub async fn open(store: Store, room: &str) -> Result<Option<(Self, Snapshot)>, Error> {
        crate::orihon::prepare::identity(room, "room")?;
        let Some(lock) = store.lock_owner(room).await? else {
            return Ok(None);
        };
        match &store {
            Store::Postgres(pool) => {
                let snapshot = load_pg(pool, room).await?;
                Ok(Some((
                    Self {
                        store,
                        room: room.to_owned(),
                        _lock: lock,
                        file: None,
                    },
                    snapshot,
                )))
            }
            Store::File(book) => {
                let book = book.clone();
                let room_name = room.to_owned();
                let (state, snapshot) = blocking(move || load_file(&book, &room_name)).await?;
                Ok(Some((
                    Self {
                        store,
                        room: room.to_owned(),
                        _lock: lock,
                        file: Some(state),
                    },
                    snapshot,
                )))
            }
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Proves the room lock still holds. A PostgreSQL owner runs one bounded query on the very
    /// connection that holds the session advisory lock; any failure means the lock may be gone,
    /// and the owner must stop instead of reconnecting. A file lock lives with its handle.
    pub async fn check_owner(&mut self) -> Result<(), Error> {
        let Some(connection) = self._lock.connection.as_mut() else {
            return Ok(());
        };
        let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&mut *connection);
        match tokio::time::timeout(std::time::Duration::from_secs(10), probe).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(error.into()),
            Err(_) => Err(refusal(
                "orihon_owner_lost",
                "the room lock connection did not answer within 10 seconds",
            )),
        }
    }

    /// Adds units for accepted records after `ingested_seq`, at most one capture batch per call.
    /// File rooms use `hint` when it continues the ingested prefix exactly.
    pub async fn ingest(
        &mut self,
        ingested_seq: i64,
        next_unit: u64,
        hint: Vec<Record>,
    ) -> Result<Vec<LeafSource>, Error> {
        let room = self.room.clone();
        match &self.store {
            Store::Postgres(pool) => {
                let cursor = sqlx::query(
                    "SELECT unit,seq FROM orihon_units WHERE room=$1 ORDER BY unit DESC LIMIT 1",
                )
                .bind(&room)
                .fetch_optional(pool)
                .await?;
                let (stored_next, stored_seq) = match cursor {
                    Some(row) => (
                        row.try_get::<i64, _>("unit")? as u64 + 1,
                        row.try_get("seq")?,
                    ),
                    None => (0, -1),
                };

                if next_unit < stored_next {
                    let mut sources = Vec::new();
                    for unit in next_unit..stored_next {
                        let (unit, record) =
                            read_unit_pg(pool, &room, unit).await?.ok_or_else(|| {
                                refusal("orihon_units_gap", "a stored unit is missing")
                            })?;
                        sources.extend(leaf_sources(&record, std::slice::from_ref(&unit)));
                    }
                    return Ok(sources);
                }
                let rows = sqlx::query(
                    "SELECT * FROM orihon WHERE room=$1 AND seq>$2 ORDER BY seq LIMIT $3",
                )
                .bind(&room)
                .bind(stored_seq)
                .bind(INGEST_RECORDS as i64)
                .fetch_all(pool)
                .await?;
                let records = rows
                    .into_iter()
                    .map(crate::orihon::pg::record_from_row)
                    .collect::<Result<Vec<_>, _>>()?;
                let (units, sources) = plan_units(&records, stored_seq, stored_next)?;
                if units.is_empty() {
                    return Ok(sources);
                }
                let rows: Vec<Value> = units
                    .iter()
                    .map(|unit| {
                        json!({"room":room,"unit":unit.unit,"seq":unit.seq,"part":unit.part,
                            "parts":unit.parts,"byte_start":unit.byte_start,"byte_end":unit.byte_end,
                            "semantic":unit.semantic,"record_digest":unit.record_digest,"digest":unit.digest})
                    })
                    .collect();
                sqlx::query(
                    "INSERT INTO orihon_units
                     SELECT * FROM jsonb_populate_recordset(NULL::orihon_units,$1) ORDER BY unit",
                )
                .bind(Value::Array(rows))
                .execute(pool)
                .await?;
                Ok(sources)
            }
            Store::File(book) => {
                let state = self.file.as_mut().expect("file rooms keep file state");
                let mut fresh: Vec<Record> = hint
                    .into_iter()
                    .filter(|record| record.seq > ingested_seq)
                    .collect();
                fresh.sort_by_key(|record| record.seq);
                fresh.dedup_by_key(|record| record.seq);
                let contiguous = fresh
                    .first()
                    .is_some_and(|record| record.seq == ingested_seq + 1)
                    && fresh.windows(2).all(|pair| pair[1].seq == pair[0].seq + 1);
                let records = if contiguous {
                    fresh
                } else {
                    let book = book.clone();
                    let room = room.clone();
                    blocking(move || book_records(&book, &room, ingested_seq)).await?
                };
                let records: Vec<Record> = records.into_iter().take(INGEST_RECORDS).collect();
                let (units, sources) = plan_units(&records, ingested_seq, next_unit)?;
                let directory = state.directory.clone();
                let stored_next = state.unit_digests.len() as u64;
                let stored_seq = state.unit_ends.last_key_value().map_or(-1, |(seq, _)| *seq);
                let written: Vec<Unit> = units
                    .iter()
                    .filter(|unit| unit.unit >= stored_next)
                    .cloned()
                    .collect();
                let result =
                    blocking(move || append_batch(&directory.join("units.jsonl"), &written)).await;

                if let Err(error) = result {
                    let book = book.clone();
                    let room = room.clone();
                    self.file = Some(
                        blocking(move || load_file(&book, &room).map(|(state, _)| state)).await?,
                    );
                    return Err(error);
                }
                for unit in units.iter().filter(|unit| unit.unit >= stored_next) {
                    state.unit_digests.push(unit.digest.clone());
                    state.unit_ends.insert(unit.seq, unit.unit + 1);
                }
                for source in sources
                    .iter()
                    .filter(|source| source.unit.unit >= stored_next)
                {
                    state.leaves.insert(source.unit.unit, source.clone());
                }
                for record in records.iter().filter(|record| record.seq > stored_seq) {
                    state
                        .sessions
                        .entry((record.harness.clone(), record.source_session.clone()))
                        .or_default()
                        .push(SessionRecord::of(record));
                }
                Ok(sources)
            }
        }
    }

    /// The unit end of every record at or before `seq`.
    pub async fn units_through(&self, seq: i64) -> Result<u64, Error> {
        match (&self.store, &self.file) {
            (_, Some(state)) => Ok(state
                .unit_ends
                .range(..=seq)
                .next_back()
                .map_or(0, |(_, end)| *end)),
            (Store::Postgres(pool), None) => {
                let unit: Option<i64> = sqlx::query_scalar(
                    "SELECT unit FROM orihon_units WHERE room=$1 AND seq<=$2
                     ORDER BY seq DESC, part DESC LIMIT 1",
                )
                .bind(&self.room)
                .bind(seq)
                .fetch_optional(pool)
                .await?;
                Ok(unit.map_or(0, |unit| unit as u64 + 1))
            }
            (Store::File(_), None) => unreachable!("file rooms keep file state"),
        }
    }

    /// The last record whose units all end at or before unit `end`.
    pub async fn seq_through_units(&self, end: u64) -> Result<Option<i64>, Error> {
        match (&self.store, &self.file) {
            (_, Some(state)) => Ok(state
                .unit_ends
                .iter()
                .rev()
                .find(|(_, unit_end)| **unit_end <= end)
                .map(|(seq, _)| *seq)),
            (Store::Postgres(pool), None) => Ok(sqlx::query_scalar(
                "SELECT seq FROM orihon_units WHERE room=$1 AND unit<$2 AND part=parts-1
                 ORDER BY unit DESC LIMIT 1",
            )
            .bind(&self.room)
            .bind(i64::try_from(end).unwrap_or(i64::MAX))
            .fetch_optional(pool)
            .await?),
            (Store::File(_), None) => unreachable!("file rooms keep file state"),
        }
    }

    /// The room's last accepted record. File rooms answer through the ingested prefix.
    pub async fn last_seq(&self) -> Result<Option<i64>, Error> {
        match (&self.store, &self.file) {
            (_, Some(state)) => Ok(state.unit_ends.keys().next_back().copied()),
            (Store::Postgres(pool), None) => Ok(sqlx::query_scalar(
                "SELECT max(seq) FROM orihon WHERE room=$1",
            )
            .bind(&self.room)
            .fetch_one(pool)
            .await?),
            (Store::File(_), None) => unreachable!("file rooms keep file state"),
        }
    }

    /// The seq of one accepted source record of a harness session.
    pub async fn source_seq(
        &self,
        harness: &str,
        session: &str,
        source_id: &str,
    ) -> Result<Option<i64>, Error> {
        match (&self.store, &self.file) {
            (_, Some(state)) => Ok(state
                .sessions
                .get(&(harness.to_owned(), session.to_owned()))
                .and_then(|records| records.iter().find(|record| record.source_id == source_id))
                .map(|record| record.seq)),
            (Store::Postgres(pool), None) => Ok(sqlx::query_scalar(
                "SELECT seq FROM orihon
                 WHERE room=$1 AND harness=$2 AND source_session=$3 AND source_id=$4",
            )
            .bind(&self.room)
            .bind(harness)
            .bind(session)
            .bind(source_id)
            .fetch_optional(pool)
            .await?),
            (Store::File(_), None) => unreachable!("file rooms keep file state"),
        }
    }

    /// The first accepted seq of a harness session.
    pub async fn session_first(&self, harness: &str, session: &str) -> Result<Option<i64>, Error> {
        match (&self.store, &self.file) {
            (_, Some(state)) => Ok(state
                .sessions
                .get(&(harness.to_owned(), session.to_owned()))
                .and_then(|records| records.first())
                .map(|record| record.seq)),
            (Store::Postgres(pool), None) => Ok(sqlx::query_scalar(
                "SELECT min(seq) FROM orihon WHERE room=$1 AND harness=$2 AND source_session=$3",
            )
            .bind(&self.room)
            .bind(harness)
            .bind(session)
            .fetch_one(pool)
            .await?),
            (Store::File(_), None) => unreachable!("file rooms keep file state"),
        }
    }

    /// At most `limit` accepted records of a harness session after `after`, oldest first.
    pub async fn session_after(
        &self,
        harness: &str,
        session: &str,
        after: i64,
        limit: usize,
    ) -> Result<Vec<SessionRecord>, Error> {
        match (&self.store, &self.file) {
            (_, Some(state)) => Ok(state
                .sessions
                .get(&(harness.to_owned(), session.to_owned()))
                .map(|records| {
                    records
                        .iter()
                        .filter(|record| record.seq > after)
                        .take(limit)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()),
            (Store::Postgres(pool), None) => {
                let rows = sqlx::query(
                    "SELECT seq,source_id,kind,parent_user_id FROM orihon
                     WHERE room=$1 AND harness=$2 AND source_session=$3 AND seq>$4
                     ORDER BY seq LIMIT $5",
                )
                .bind(&self.room)
                .bind(harness)
                .bind(session)
                .bind(after)
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .fetch_all(pool)
                .await?;
                rows.into_iter()
                    .map(|row| -> Result<SessionRecord, Error> {
                        Ok(SessionRecord {
                            seq: row.try_get("seq")?,
                            source_id: row.try_get("source_id")?,
                            kind: serde_json::from_value(Value::String(row.try_get("kind")?))
                                .map_err(|_| {
                                    refusal("invalid_book", "a stored record kind is unknown")
                                })?,
                            parent_user_id: row.try_get("parent_user_id")?,
                        })
                    })
                    .collect()
            }
            (Store::File(_), None) => unreachable!("file rooms keep file state"),
        }
    }

    pub async fn leaf_source(&mut self, unit: u64) -> Result<LeafSource, Error> {
        if let Some(state) = &self.file {
            return state
                .leaves
                .get(&unit)
                .cloned()
                .ok_or_else(|| refusal("orihon_unit_missing", "the unit source is not loaded"));
        }
        let Store::Postgres(pool) = &self.store else {
            unreachable!("database rooms have a pool")
        };
        let (unit, record) = read_unit_pg(pool, &self.room, unit)
            .await?
            .ok_or_else(|| refusal("orihon_unit_missing", "the unit has no stored source"))?;
        Ok(leaf_sources(&record, std::slice::from_ref(&unit)).remove(0))
    }

    /// Texts of built nodes. File rooms answer from memory.
    pub async fn texts(&mut self, keys: &[NodeKey]) -> Result<HashMap<NodeKey, String>, Error> {
        if let Some(state) = &self.file {
            return Ok(keys
                .iter()
                .filter_map(|key| state.texts.get(key).map(|text| (*key, text.clone())))
                .collect());
        }
        let Store::Postgres(pool) = &self.store else {
            unreachable!("database rooms have a pool")
        };
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let levels: Vec<i16> = keys.iter().map(|key| i16::from(key.level)).collect();
        let indexes: Vec<i64> = keys.iter().map(|key| key.index as i64).collect();
        let rows = sqlx::query(
            "SELECT node.level,node.idx,node.text FROM orihon_nodes node
             JOIN unnest($2::smallint[],$3::bigint[]) AS wanted(level,idx)
               ON node.level=wanted.level AND node.idx=wanted.idx
             WHERE node.room=$1",
        )
        .bind(&self.room)
        .bind(&levels)
        .bind(&indexes)
        .fetch_all(pool)
        .await?;
        let mut texts = HashMap::with_capacity(rows.len());
        for row in rows {
            let key = NodeKey {
                level: row.try_get::<i16, _>("level")? as u8,
                index: row.try_get::<i64, _>("idx")? as u64,
            };
            texts.insert(key, row.try_get("text")?);
        }
        Ok(texts)
    }

    /// Stores one built node, after its accepted job when a model built it. Returns the node.
    pub async fn put_node(
        &mut self,
        key: NodeKey,
        text: String,
        method: Method,
        source_digest: String,
        job: Option<&Job>,
    ) -> Result<Node, Error> {
        let ceiling = if method == Method::Model {
            MODEL_LINE_CEILING
        } else {
            LINE_BYTES
        };
        if text.is_empty() || text.len() > ceiling {
            return Err(invalid(
                "a node line must be nonempty and within its byte ceiling",
            ));
        }
        let digest = node_digest(key, &source_digest, &text);
        match &self.store {
            Store::Postgres(pool) => {
                let mut transaction = pool.begin().await?;
                let job_id = match job {
                    Some(job) => Some(insert_job(&mut transaction, &self.room, job).await?),
                    None => None,
                };
                let inserted: Result<chrono::DateTime<Utc>, sqlx::Error> = sqlx::query_scalar(
                    "INSERT INTO orihon_nodes(room,level,idx,text,method,source_digest,digest,job)
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING built_at",
                )
                .bind(&self.room)
                .bind(i16::from(key.level))
                .bind(key.index as i64)
                .bind(&text)
                .bind(method.name())
                .bind(&source_digest)
                .bind(&digest)
                .bind(job_id)
                .fetch_one(&mut *transaction)
                .await;
                let built_at = match inserted {
                    Ok(built_at) => built_at,
                    Err(error)
                        if error.as_database_error().is_some_and(|error| {
                            error.is_unique_violation()
                                && error.constraint() == Some("orihon_nodes_pkey")
                        }) =>
                    {
                        transaction.rollback().await?;
                        let stored = read_node(&self.store, &self.room, key).await?;
                        let job_matches = match (job, stored.as_ref().and_then(|node| node.job)) {
                            (None, None) => true,
                            (Some(job), Some(id)) => {
                                sqlx::query_scalar::<_, bool>(
                                    "SELECT level=$3 AND idx=$4 FROM orihon_jobs WHERE room=$1 AND id=$2",
                                )
                                .bind(&self.room)
                                .bind(id)
                                .bind(i16::from(job.key.level))
                                .bind(job.key.index as i64)
                                .fetch_optional(pool)
                                .await?
                                .unwrap_or(false)
                            }
                            _ => false,
                        };

                        if let Some(stored) = stored.filter(|node| {
                            node.digest == digest && node.method == method && job_matches
                        }) {
                            return Ok(stored);
                        }
                        return Err(error.into());
                    }
                    Err(error) => return Err(error.into()),
                };
                transaction.commit().await?;
                Ok(Node {
                    key,
                    text,
                    method,
                    source_digest,
                    digest,
                    job: job_id,
                    built_at: built_at.to_rfc3339_opts(SecondsFormat::AutoSi, true),
                })
            }
            Store::File(book) => {
                let state = self.file.as_mut().expect("file rooms keep file state");
                let job_id = job.map(|_| state.next_job);
                let node = Node {
                    key,
                    text,
                    method,
                    source_digest,
                    digest,
                    job: job_id,
                    built_at: now(),
                };
                let directory = state.directory.clone();
                let job = job.map(|job| Job {
                    id: job_id,
                    ..job.clone()
                });
                let line = node.clone();
                let result = blocking(move || {
                    if let Some(job) = &job {
                        append_line(&directory.join("jobs.jsonl"), job)?;
                    }
                    let day = &line.built_at[..10];
                    append_line(&directory.join("tree").join(format!("{day}.jsonl")), &line)
                })
                .await;

                if let Err(error) = result {
                    let book = book.clone();
                    let room = self.room.clone();
                    self.file = Some(
                        blocking(move || load_file(&book, &room).map(|(state, _)| state)).await?,
                    );
                    return Err(error);
                }
                if job_id.is_some() {
                    state.next_job += 1;
                }
                state.texts.insert(key, node.text.clone());
                state.digests.insert(key, node.digest.clone());
                if key.level == 0 {
                    state.leaves.remove(&key.index);
                }
                Ok(node)
            }
        }
    }

    /// The exact digest of a built node, used to bind its parent.
    pub async fn node_digest(&mut self, key: NodeKey) -> Result<Option<String>, Error> {
        if let Some(state) = &self.file {
            return Ok(state.digests.get(&key).cloned());
        }
        let Store::Postgres(pool) = &self.store else {
            unreachable!("database rooms have a pool")
        };
        Ok(sqlx::query_scalar(
            "SELECT digest FROM orihon_nodes WHERE room=$1 AND level=$2 AND idx=$3",
        )
        .bind(&self.room)
        .bind(i16::from(key.level))
        .bind(key.index as i64)
        .fetch_optional(pool)
        .await?)
    }

    /// Stores a failed or cancelled job with every attempt it paid for.
    pub async fn put_job(&mut self, job: &Job) -> Result<i64, Error> {
        match &self.store {
            Store::Postgres(pool) => {
                let mut transaction = pool.begin().await?;
                let id = insert_job(&mut transaction, &self.room, job).await?;
                transaction.commit().await?;
                Ok(id)
            }
            Store::File(_) => {
                let state = self.file.as_mut().expect("file rooms keep file state");
                let id = state.next_job;
                let path = state.directory.join("jobs.jsonl");
                let job = Job {
                    id: Some(id),
                    ..job.clone()
                };
                blocking(move || append_line(&path, &job)).await?;
                state.next_job += 1;
                Ok(id)
            }
        }
    }

    pub async fn save_views(&mut self, chat: &View, compactor: &View) -> Result<(), Error> {
        chat.validate().map_err(invalid)?;
        compactor.validate().map_err(invalid)?;
        match &self.store {
            Store::Postgres(pool) => {
                let mut transaction = pool.begin().await?;
                for (name, view) in [("chat", chat), ("compactor", compactor)] {
                    sqlx::query(
                        "INSERT INTO orihon_views(room,name,revision,units,merging,lines)
                         VALUES($1,$2,$3,$4,$5,$6)
                         ON CONFLICT(room,name) DO UPDATE SET revision=EXCLUDED.revision,
                           units=EXCLUDED.units, merging=EXCLUDED.merging, lines=EXCLUDED.lines,
                           updated_at=now()",
                    )
                    .bind(&self.room)
                    .bind(name)
                    .bind(view.revision as i64)
                    .bind(view.units as i64)
                    .bind(view.merging)
                    .bind(json!(
                        view.lines
                            .iter()
                            .map(|key| (key.level, key.index))
                            .collect::<Vec<_>>()
                    ))
                    .execute(&mut *transaction)
                    .await?;
                }
                transaction.commit().await?;
                Ok(())
            }
            Store::File(_) => {
                let state = self.file.as_ref().expect("file rooms keep file state");
                let path = state.directory.join("views.json");
                let views = Views {
                    chat: chat.clone(),
                    compactor: compactor.clone(),
                };
                blocking(move || write_atomic(&path, &views)).await
            }
        }
    }

    pub async fn save_config(&mut self, config: &LiveConfig) -> Result<(), Error> {
        match &self.store {
            Store::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO orihon_live(room,folder,prefix_digest) VALUES($1,$2,$3)
                     ON CONFLICT(room) DO UPDATE SET folder=EXCLUDED.folder,
                       prefix_digest=EXCLUDED.prefix_digest, updated_at=now()",
                )
                .bind(&self.room)
                .bind(config.folder.as_ref().map(|folder| json!(folder)))
                .bind(&config.prefix_digest)
                .execute(pool)
                .await?;
                Ok(())
            }
            Store::File(_) => {
                let state = self.file.as_ref().expect("file rooms keep file state");
                let path = state.directory.join("live.json");
                let config = config.clone();
                blocking(move || write_atomic(&path, &config)).await
            }
        }
    }

    pub async fn put_prefix(&mut self, prefix: &Prefix) -> Result<String, Error> {
        let digest = prefix.digest();
        match &self.store {
            Store::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO orihon_prefixes(room,digest,body) VALUES($1,$2,$3)
                     ON CONFLICT(room,digest) DO NOTHING",
                )
                .bind(&self.room)
                .bind(&digest)
                .bind(
                    serde_json::to_string(prefix)
                        .map_err(|_| invalid("prefix cannot be serialized"))?,
                )
                .execute(pool)
                .await?;
            }
            Store::File(_) => {
                let state = self.file.as_ref().expect("file rooms keep file state");
                let path = state
                    .directory
                    .join("prefixes")
                    .join(format!("{digest}.json"));
                let prefix = prefix.clone();
                blocking(move || {
                    if !path.exists() {
                        write_atomic(&path, &prefix)?;
                    }
                    Ok(())
                })
                .await?;
            }
        }
        Ok(digest)
    }

    pub async fn prefix(&self, digest: &str) -> Result<Option<Prefix>, Error> {
        read_prefix(&self.store, &self.room, digest).await
    }
}

/// Splits records into the next units, refusing a gap in the ingested seq order.
fn plan_units(
    records: &[Record],
    ingested_seq: i64,
    next_unit: u64,
) -> Result<(Vec<Unit>, Vec<LeafSource>), Error> {
    let mut units = Vec::new();
    let mut sources = Vec::new();
    let mut expected = ingested_seq + 1;
    let mut next = next_unit;
    for record in records {
        if record.seq != expected {
            return Err(refusal(
                "orihon_units_gap",
                "records must enter the live tree in dense seq order",
            ));
        }
        let record_units = units_for(record, next);
        next += record_units.len() as u64;
        sources.extend(leaf_sources(record, &record_units));
        units.extend(record_units);
        expected += 1;
    }
    Ok((units, sources))
}

async fn insert_job(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    room: &str,
    job: &Job,
) -> Result<i64, Error> {
    let cost_status = if job.cost_usd.is_some() {
        "reported"
    } else {
        "unavailable"
    };
    Ok(sqlx::query_scalar(
        "INSERT INTO orihon_jobs(room,level,idx,outcome,backend,provider,model,cache_scope,prefix_digest,
            context_digest,context_lines,context_bytes,input_bytes,attempts,input_tokens,output_tokens,
            cache_read_tokens,cache_write_tokens,cost_usd,cost_status,cost_reason,error,started_at,finished_at)
         SELECT room,level,idx,outcome,backend,provider,model,cache_scope,prefix_digest,
            context_digest,context_lines,context_bytes,input_bytes,attempts,input_tokens,output_tokens,
            cache_read_tokens,cache_write_tokens,cost_usd,cost_status,cost_reason,error,started_at,finished_at
         FROM jsonb_populate_record(NULL::orihon_jobs,$1)
         RETURNING id",
    )
    .bind(json!({
        "room":room, "level":job.key.level, "idx":job.key.index, "outcome":job.outcome.name(),
        "backend":job.backend, "provider":job.provider, "model":job.model,
        "cache_scope":job.cache_scope, "prefix_digest":job.prefix_digest,
        "context_digest":job.context_digest, "context_lines":job.context_lines,
        "context_bytes":job.context_bytes, "input_bytes":job.input_bytes,
        "attempts":job.attempts, "input_tokens":job.usage.input_tokens,
        "output_tokens":job.usage.output_tokens, "cache_read_tokens":job.usage.cache_read_tokens,
        "cache_write_tokens":job.usage.cache_write_tokens, "cost_usd":job.cost_usd,
        "cost_status":cost_status, "cost_reason":job.cost_reason, "error":job.error,
        "started_at":job.started_at, "finished_at":job.finished_at,
    }))
    .fetch_one(&mut **transaction)
    .await?)
}

async fn load_pg(pool: &PgPool, room: &str) -> Result<Snapshot, Error> {
    let cursor =
        sqlx::query("SELECT unit,seq FROM orihon_units WHERE room=$1 ORDER BY unit DESC LIMIT 1")
            .bind(room)
            .fetch_optional(pool)
            .await?;
    let (units, ingested_seq) = match cursor {
        Some(row) => (
            row.try_get::<i64, _>("unit")? as u64 + 1,
            row.try_get::<i64, _>("seq")?,
        ),
        None => (0, -1),
    };
    // A record whose units were cut short by a crash would leave a partial tiling.
    if let Some(row) =
        sqlx::query("SELECT part,parts FROM orihon_units WHERE room=$1 ORDER BY unit DESC LIMIT 1")
            .bind(room)
            .fetch_optional(pool)
            .await?
    {
        let (part, parts): (i32, i32) = (row.try_get("part")?, row.try_get("parts")?);
        if part + 1 != parts {
            return Err(refusal(
                "orihon_units_partial",
                "the last record's units are incomplete; repair the unit tiling before live use",
            ));
        }
    }
    let mut nodes = HashMap::new();
    for row in sqlx::query("SELECT level,idx,bytes FROM orihon_nodes WHERE room=$1")
        .bind(room)
        .fetch_all(pool)
        .await?
    {
        nodes.insert(
            NodeKey {
                level: row.try_get::<i16, _>("level")? as u8,
                index: row.try_get::<i64, _>("idx")? as u64,
            },
            row.try_get::<i32, _>("bytes")? as usize,
        );
    }
    let (chat, compactor) = read_views(&Store::Postgres(pool.clone()), room).await?;
    let config = read_config(&Store::Postgres(pool.clone()), room).await?;
    Ok(Snapshot {
        units,
        ingested_seq,
        nodes,
        chat,
        compactor,
        config,
    })
}

fn load_file(book: &Path, room: &str) -> Result<(FileState, Snapshot), Error> {
    let directory = live_directory(book);
    let units: Vec<(Unit, u64)> = owner_lines(&directory.join("units.jsonl"))?;
    let records = book_records(book, room, -1)?;
    let mut unit_digests = Vec::with_capacity(units.len());
    let mut unit_ends = BTreeMap::new();
    let mut by_unit = Vec::with_capacity(units.len());
    let mut next = 0;
    let mut seq = -1;
    let mut record_offset = 0;
    for (index, (unit, offset)) in units.iter().enumerate() {
        let record = records
            .get(unit.seq as usize)
            .filter(|record| record.seq == unit.seq)
            .ok_or_else(|| malformed(&directory.join("units.jsonl"), index + 1))?;
        let first = unit
            .unit
            .checked_sub(u64::from(unit.part))
            .ok_or_else(|| malformed(&directory.join("units.jsonl"), index + 1))?;
        let expected = units_for(record, first);

        if expected.get(unit.part as usize) != Some(unit) {
            return Err(malformed(&directory.join("units.jsonl"), index + 1));
        }
        if unit.unit < next {
            if unit_digests.get(unit.unit as usize) == Some(&unit.digest) {
                continue;
            }
            return Err(malformed(&directory.join("units.jsonl"), index + 1));
        }
        if unit.unit != next {
            return Err(malformed(&directory.join("units.jsonl"), index + 1));
        }
        if unit.part == 0 {
            record_offset = *offset;
        }
        next += 1;
        seq = unit.seq;
        unit_digests.push(unit.digest.clone());
        unit_ends.insert(unit.seq, unit.unit + 1);
        by_unit.push((record, unit));
    }
    if let Some((_, unit)) = by_unit
        .last()
        .filter(|(_, unit)| unit.part + 1 != unit.parts)
    {
        let first = unit.unit - u64::from(unit.part);
        unit_ends.remove(&unit.seq);
        truncate_tail(&directory.join("units.jsonl"), record_offset)?;
        unit_digests.truncate(first as usize);
        by_unit.truncate(first as usize);
        next = first;
        seq = by_unit.last().map_or(-1, |(_, unit)| unit.seq);
    }

    let mut stored_nodes = Vec::new();
    for path in tree_paths(&directory)? {
        for (index, (node, _)) in owner_lines::<Node>(&path)?.into_iter().enumerate() {
            stored_nodes.push((node, path.clone(), index + 1));
        }
    }
    stored_nodes.sort_by_key(|(node, _, _)| (node.key.level, node.key.index));
    let mut nodes: HashMap<NodeKey, Node> = HashMap::new();
    for (node, path, line) in stored_nodes {
        if let Some(stored) = nodes.get(&node.key) {
            if stored.digest == node.digest {
                continue;
            }
            return Err(malformed(&path, line));
        }

        if !check_node(
            &node,
            |unit| unit_digests.get(unit as usize).cloned(),
            |key| nodes.get(&key).map(|node| node.digest.clone()),
        ) {
            return Err(malformed(&path, line));
        }
        nodes.insert(node.key, node);
    }
    let jobs: Vec<(Job, u64)> = owner_lines(&directory.join("jobs.jsonl"))?;
    let leaves = by_unit
        .into_iter()
        .filter(|(_, unit)| !nodes.contains_key(&NodeKey::leaf(unit.unit)))
        .flat_map(|(record, unit)| leaf_sources(record, std::slice::from_ref(unit)))
        .map(|source| (source.unit.unit, source))
        .collect();
    let mut sessions: HashMap<(String, String), Vec<SessionRecord>> = HashMap::new();
    for record in records.iter().take_while(|record| record.seq <= seq) {
        sessions
            .entry((record.harness.clone(), record.source_session.clone()))
            .or_default()
            .push(SessionRecord::of(record));
    }
    let views: Option<Views> = read_json(&directory.join("views.json"))?;
    if let Some(views) = &views {
        views.chat.validate().map_err(invalid)?;
        views.compactor.validate().map_err(invalid)?;
    }
    let config: LiveConfig = read_json(&directory.join("live.json"))?.unwrap_or_default();
    let snapshot = Snapshot {
        units: next,
        ingested_seq: seq,
        nodes: nodes
            .iter()
            .map(|(key, node)| (*key, node.text.len()))
            .collect(),
        chat: views.as_ref().map(|views| views.chat.clone()),
        compactor: views.map(|views| views.compactor),
        config,
    };
    let state = FileState {
        directory,
        digests: nodes
            .iter()
            .map(|(key, node)| (*key, node.digest.clone()))
            .collect(),
        texts: nodes
            .into_iter()
            .map(|(key, node)| (key, node.text))
            .collect(),
        unit_digests,
        unit_ends,
        leaves,
        sessions,
        next_job: jobs.len() as i64,
    };
    Ok((state, snapshot))
}

async fn read_unit_pg(
    pool: &PgPool,
    room: &str,
    unit: u64,
) -> Result<Option<(Unit, Record)>, Error> {
    let Some(row) = sqlx::query(
        "SELECT unit,seq,part,parts,byte_start,byte_end,semantic,record_digest,digest
         FROM orihon_units WHERE room=$1 AND unit=$2",
    )
    .bind(room)
    .bind(unit as i64)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let unit = Unit {
        unit: row.try_get::<i64, _>("unit")? as u64,
        seq: row.try_get("seq")?,
        part: row.try_get::<i32, _>("part")? as u32,
        parts: row.try_get::<i32, _>("parts")? as u32,
        byte_start: row.try_get::<i32, _>("byte_start")? as usize,
        byte_end: row.try_get::<i32, _>("byte_end")? as usize,
        semantic: row.try_get("semantic")?,
        record_digest: row.try_get("record_digest")?,
        digest: row.try_get("digest")?,
    };
    let record = sqlx::query("SELECT * FROM orihon WHERE room=$1 AND seq=$2")
        .bind(room)
        .bind(unit.seq)
        .fetch_one(pool)
        .await
        .map_err(Error::from)
        .and_then(crate::orihon::pg::record_from_row)?;
    Ok(Some((unit, record)))
}

/// The unit and its record, for navigation.
pub async fn read_unit(
    store: &Store,
    room: &str,
    unit: u64,
) -> Result<Option<(Unit, Record)>, Error> {
    match store {
        Store::Postgres(pool) => read_unit_pg(pool, room, unit).await,
        Store::File(book) => {
            let book = book.clone();
            let room = room.to_owned();
            blocking(move || {
                let directory = live_directory(&book);
                let Some(found) = read_lines::<Unit>(&directory.join("units.jsonl"))?
                    .into_iter()
                    .find(|candidate| candidate.unit == unit)
                else {
                    return Ok(None);
                };
                let record = record_from_book(&book, &room, found.seq)?.ok_or_else(|| {
                    refusal("orihon_unit_missing", "the unit has no stored source")
                })?;
                Ok(Some((found, record)))
            })
            .await
        }
    }
}

/// Every unit of one record, for page navigation.
pub async fn record_units(store: &Store, room: &str, seq: i64) -> Result<Vec<Unit>, Error> {
    match store {
        Store::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT unit,seq,part,parts,byte_start,byte_end,semantic,record_digest,digest
                 FROM orihon_units WHERE room=$1 AND seq=$2 ORDER BY part",
            )
            .bind(room)
            .bind(seq)
            .fetch_all(pool)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Unit {
                        unit: row.try_get::<i64, _>("unit")? as u64,
                        seq: row.try_get("seq")?,
                        part: row.try_get::<i32, _>("part")? as u32,
                        parts: row.try_get::<i32, _>("parts")? as u32,
                        byte_start: row.try_get::<i32, _>("byte_start")? as usize,
                        byte_end: row.try_get::<i32, _>("byte_end")? as usize,
                        semantic: row.try_get("semantic")?,
                        record_digest: row.try_get("record_digest")?,
                        digest: row.try_get("digest")?,
                    })
                })
                .collect()
        }
        Store::File(book) => {
            let path = live_directory(book).join("units.jsonl");
            blocking(move || {
                Ok(read_lines::<Unit>(&path)?
                    .into_iter()
                    .filter(|unit| unit.seq == seq)
                    .collect())
            })
            .await
        }
    }
}

pub async fn read_node(store: &Store, room: &str, key: NodeKey) -> Result<Option<Node>, Error> {
    match store {
        Store::Postgres(pool) => {
            let Some(row) = sqlx::query(
                "SELECT text,method,source_digest,digest,job,built_at FROM orihon_nodes
                 WHERE room=$1 AND level=$2 AND idx=$3",
            )
            .bind(room)
            .bind(i16::from(key.level))
            .bind(key.index as i64)
            .fetch_optional(pool)
            .await?
            else {
                return Ok(None);
            };
            Ok(Some(Node {
                key,
                text: row.try_get("text")?,
                method: Method::parse(row.try_get("method")?)
                    .ok_or_else(|| refusal("invalid_book", "stored node method is unknown"))?,
                source_digest: row.try_get("source_digest")?,
                digest: row.try_get("digest")?,
                job: row.try_get("job")?,
                built_at: row
                    .try_get::<chrono::DateTime<Utc>, _>("built_at")?
                    .to_rfc3339_opts(SecondsFormat::AutoSi, true),
            }))
        }
        Store::File(book) => {
            let directory = live_directory(book);
            blocking(move || {
                for path in tree_paths(&directory)? {
                    if let Some(node) = read_lines::<Node>(&path)?
                        .into_iter()
                        .find(|node| node.key == key)
                    {
                        return Ok(Some(node));
                    }
                }
                Ok(None)
            })
            .await
        }
    }
}

pub async fn read_views(store: &Store, room: &str) -> Result<(Option<View>, Option<View>), Error> {
    match store {
        Store::Postgres(pool) => {
            let mut chat = None;
            let mut compactor = None;
            for row in sqlx::query(
                "SELECT name,revision,units,merging,lines FROM orihon_views WHERE room=$1",
            )
            .bind(room)
            .fetch_all(pool)
            .await?
            {
                let view: View = serde_json::from_value(json!({
                    "revision": row.try_get::<i64, _>("revision")?,
                    "units": row.try_get::<i64, _>("units")?,
                    "merging": row.try_get::<bool, _>("merging")?,
                    "lines": row.try_get::<Value, _>("lines")?,
                }))
                .map_err(|_| refusal("invalid_book", "a persisted Orihon view is malformed"))?;
                view.validate()
                    .map_err(|message| refusal("invalid_book", message))?;
                match row.try_get::<&str, _>("name")? {
                    "chat" => chat = Some(view),
                    _ => compactor = Some(view),
                }
            }
            Ok((chat, compactor))
        }
        Store::File(book) => {
            let path = live_directory(book).join("views.json");
            blocking(move || {
                let views: Option<Views> = read_json(&path)?;
                if let Some(views) = &views {
                    views
                        .chat
                        .validate()
                        .map_err(|message| refusal("invalid_book", message))?;
                    views
                        .compactor
                        .validate()
                        .map_err(|message| refusal("invalid_book", message))?;
                }
                Ok(views.map_or((None, None), |views| {
                    (Some(views.chat), Some(views.compactor))
                }))
            })
            .await
        }
    }
}

pub async fn read_config(store: &Store, room: &str) -> Result<LiveConfig, Error> {
    match store {
        Store::Postgres(pool) => {
            let Some(row) =
                sqlx::query("SELECT folder,prefix_digest FROM orihon_live WHERE room=$1")
                    .bind(room)
                    .fetch_optional(pool)
                    .await?
            else {
                return Ok(LiveConfig::default());
            };
            Ok(LiveConfig {
                folder: row
                    .try_get::<Option<Value>, _>("folder")?
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|_| {
                        refusal("invalid_book", "the live folder configuration is malformed")
                    })?,
                prefix_digest: row.try_get("prefix_digest")?,
            })
        }
        Store::File(book) => {
            let path = live_directory(book).join("live.json");
            blocking(move || Ok(read_json(&path)?.unwrap_or_default())).await
        }
    }
}

pub async fn read_prefix(store: &Store, room: &str, digest: &str) -> Result<Option<Prefix>, Error> {
    if !valid_digest(digest) {
        return Err(invalid("prefix digest must be 64 lowercase hex characters"));
    }
    let prefix = match store {
        Store::Postgres(pool) => {
            let Some(body) = sqlx::query_scalar::<_, String>(
                "SELECT body FROM orihon_prefixes WHERE room=$1 AND digest=$2",
            )
            .bind(room)
            .bind(digest)
            .fetch_optional(pool)
            .await?
            else {
                return Ok(None);
            };
            serde_json::from_str::<Prefix>(&body)
                .map_err(|_| refusal("invalid_book", "a stored prefix is malformed"))?
        }
        Store::File(book) => {
            let path = live_directory(book)
                .join("prefixes")
                .join(format!("{digest}.json"));
            match blocking(move || read_json::<Prefix>(&path)).await? {
                Some(prefix) => prefix,
                None => return Ok(None),
            }
        }
    };
    if prefix.digest() != digest {
        return Err(refusal(
            "invalid_book",
            "a stored prefix does not match its digest",
        ));
    }
    Ok(Some(prefix))
}

/// Stored work records that name `name` among their exact child identities, oldest first.
pub async fn child_reports(
    store: &Store,
    room: &str,
    name: &str,
) -> Result<Vec<(Record, ChildRef)>, Error> {
    crate::orihon::prepare::identity(name, "child name")?;
    let records = match store {
        Store::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM orihon WHERE room=$1 AND children @> $2 ORDER BY seq LIMIT 50",
            )
            .bind(room)
            .bind(json!([{ "name": name }]))
            .fetch_all(pool)
            .await?;
            rows.into_iter()
                .map(crate::orihon::pg::record_from_row)
                .collect::<Result<Vec<_>, _>>()?
        }
        Store::File(book) => {
            let book = book.clone();
            let room = room.to_owned();
            let wanted = name.to_owned();
            blocking(move || {
                Ok(book_records(&book, &room, -1)?
                    .into_iter()
                    .filter(|record| {
                        record.children.as_ref().is_some_and(|children| {
                            children.iter().any(|child| child.name == wanted)
                        })
                    })
                    .take(50)
                    .collect())
            })
            .await?
        }
    };
    Ok(records
        .into_iter()
        .flat_map(|record| {
            let children: Vec<ChildRef> = record
                .children
                .iter()
                .flatten()
                .filter(|child| child.name == name)
                .cloned()
                .collect();
            children
                .into_iter()
                .map(move |child| (record.clone(), child))
        })
        .collect())
}

/// The first unit of a record, for linking a work report back into the tree.
pub async fn first_unit(store: &Store, room: &str, seq: i64) -> Result<Option<u64>, Error> {
    Ok(record_units(store, room, seq)
        .await?
        .first()
        .map(|unit| unit.unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(input: Option<u64>, outcome: &str) -> Attempt {
        Attempt {
            attempt: 1,
            outcome: outcome.into(),
            bytes: Some(600),
            usage: Usage {
                input_tokens: input,
                output_tokens: Some(10),
                cache_read_tokens: Some(5),
                cache_write_tokens: Some(1),
            },
            cost_usd: None,
            cost_reason: Some("the provider reported no price".into()),
            stop_reason: Some("end_turn".into()),
            error: None,
            elapsed_ms: 1,
        }
    }

    #[test]
    fn job_usage_counts_every_paid_attempt_and_never_invents_zero() {
        let attempts = [
            attempt(Some(100), "too_long"),
            attempt(Some(120), "failed"),
            attempt(Some(90), "accepted"),
        ];
        let total = Usage::total(attempts.iter().map(|attempt| &attempt.usage));
        assert_eq!(total.input_tokens, Some(310));
        assert_eq!(total.output_tokens, Some(30));
        assert_eq!(total.cache_read_tokens, Some(15));
        let unknown = [attempt(Some(100), "too_long"), attempt(None, "accepted")];
        assert_eq!(
            Usage::total(unknown.iter().map(|attempt| &attempt.usage)).input_tokens,
            None
        );
    }

    #[test]
    fn prefix_digest_covers_system_parts_and_tools() {
        let prefix = Prefix {
            system: vec!["a".into(), "b".into()],
            tools: vec![Tool {
                name: "zoom".into(),
                description: "Open a line".into(),
                input_schema: json!({"type":"object"}),
            }],
        };
        let mut joined = prefix.clone();
        joined.system = vec!["ab".into()];
        assert_ne!(prefix.digest(), joined.digest());
        assert_eq!(prefix.digest(), prefix.clone().digest());
    }

    struct TempRoom(PathBuf);

    impl TempRoom {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("orihon-live-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn store(&self) -> Store {
            Store::File(self.0.clone())
        }
    }

    impl Drop for TempRoom {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove the owned temporary room");
            assert!(!self.0.exists());
        }
    }

    fn capture_batch(long: bool) -> hearth::orihon::CaptureBatch {
        use hearth::orihon::CaptureRecord;
        hearth::orihon::CaptureBatch {
            harness: "omp".into(),
            source_session: "storage-seal".into(),
            records: (0..2)
                .map(|index| CaptureRecord {
                    source_id: format!("user-{index}"),
                    parent_user_id: None,
                    kind: Kind::User,
                    speaker: "Operator".into(),
                    text: if long && index == 1 {
                        "x".repeat(40000)
                    } else {
                        format!("record {index}")
                    },
                    said_at: "2026-10-23T00:00:00Z".into(),
                    timestamp_observed: false,
                    redacted: false,
                    fold_text: None,
                    children: None,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn file_ingest_recovers_torn_records_and_retries_without_duplicate_lines() {
        let room = TempRoom::new();
        let records = crate::orihon::append_file(&room.0, "seal", "writer", capture_batch(true))
            .unwrap()
            .accepted_records;
        let (mut owner, _) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        let sources = owner.ingest(-1, 0, records.clone()).await.unwrap();
        assert!(sources.len() > 2);
        drop(owner);

        let path = room.0.join("live/units.jsonl");
        let length = fs::metadata(&path).unwrap().len();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(length - 13)
            .unwrap();
        let (mut owner, snapshot) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        assert_eq!(snapshot.units, 1);
        owner
            .ingest(snapshot.ingested_seq, snapshot.units, records.clone())
            .await
            .unwrap();
        let complete = fs::read(&path).unwrap();
        assert_eq!(
            complete.iter().filter(|byte| **byte == b'\n').count(),
            sources.len()
        );
        owner.ingest(-1, 0, records).await.unwrap();
        assert_eq!(fs::read(&path).unwrap(), complete);
        drop(owner);

        let (_owner, snapshot) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        assert_eq!(snapshot.units, sources.len() as u64);
    }

    #[tokio::test]
    async fn file_nodes_load_by_level_across_days_and_ignore_identical_retries() {
        let room = TempRoom::new();
        let records = crate::orihon::append_file(&room.0, "seal", "writer", capture_batch(false))
            .unwrap()
            .accepted_records;
        let (mut owner, _) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        let sources = owner.ingest(-1, 0, records).await.unwrap();
        let mut nodes = Vec::new();
        for source in sources {
            nodes.push(
                owner
                    .put_node(
                        NodeKey::leaf(source.unit.unit),
                        source.text,
                        Method::Verbatim,
                        source.unit.digest,
                        None,
                    )
                    .await
                    .unwrap(),
            );
        }
        let parent = owner
            .put_node(
                NodeKey { level: 1, index: 0 },
                "joined".into(),
                Method::Joined,
                branch_source_digest(&nodes[0].digest, &nodes[1].digest),
                None,
            )
            .await
            .unwrap();
        drop(owner);

        let tree = room.0.join("live/tree");
        for entry in fs::read_dir(&tree).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        append_batch(&tree.join("2020-01-01.jsonl"), &[parent.clone(), parent]).unwrap();
        append_batch(&tree.join("2030-01-01.jsonl"), &nodes).unwrap();
        OpenOptions::new()
            .append(true)
            .open(tree.join("2030-01-01.jsonl"))
            .unwrap()
            .write_all(b"{torn")
            .unwrap();
        let (_owner, snapshot) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        assert_eq!(snapshot.nodes.len(), 3);
    }

    #[tokio::test]
    async fn file_open_refuses_persisted_view_gaps() {
        let room = TempRoom::new();
        let (mut owner, _) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        owner
            .save_views(&View::default(), &View::default())
            .await
            .unwrap();
        drop(owner);
        let invalid = json!({"revision":1,"units":2,"merging":false,"lines":[[0,1],[0,0]]});
        fs::write(
            room.0.join("live/views.json"),
            serde_json::to_vec(&json!({"chat":invalid,"compactor":invalid})).unwrap(),
        )
        .unwrap();
        assert!(Live::open(room.store(), "seal").await.is_err());
    }

    async fn pg_seal_room() -> (PgPool, String) {
        let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
            .expect("set the isolated database binding");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .unwrap();
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            database,
            "orihon_live_test_973137750b62482e8c884c62f6b54891"
        );
        let room = format!("storage-seal-{}", uuid::Uuid::new_v4());
        crate::orihon::append_pg(&pool, &room, "writer", capture_batch(false))
            .await
            .unwrap();
        (pool, room)
    }

    #[tokio::test]
    #[ignore = "requires the migrated isolated Orihon database"]
    async fn pg_ingest_reconciles_a_lost_acknowledgment() {
        let (pool, room) = pg_seal_room().await;
        let (mut owner, _) = Live::open(Store::Postgres(pool.clone()), &room)
            .await
            .unwrap()
            .unwrap();
        let sources = owner.ingest(-1, 0, Vec::new()).await.unwrap();
        assert_eq!(sources.len(), 2);
        assert_eq!(owner.ingest(-1, 0, Vec::new()).await.unwrap(), sources);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orihon_units WHERE room=$1")
            .bind(&room)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let view = View::default();
        owner.save_views(&view, &view).await.unwrap();
        drop(owner);
        let (_owner, snapshot) = Live::open(Store::Postgres(pool), &room)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.chat.unwrap().revision, 1);
    }

    #[tokio::test]
    #[ignore = "requires the migrated isolated Orihon database"]
    async fn pg_node_retry_rolls_back_the_duplicate_job() {
        let (pool, room) = pg_seal_room().await;
        let (mut owner, _) = Live::open(Store::Postgres(pool.clone()), &room)
            .await
            .unwrap()
            .unwrap();
        let sources = owner.ingest(-1, 0, Vec::new()).await.unwrap();
        let key = NodeKey::leaf(0);
        let job = Job {
            id: None,
            key,
            outcome: Outcome::Accepted,
            backend: "local".into(),
            provider: "seal".into(),
            model: "seal".into(),
            cache_scope: "a".repeat(64),
            prefix_digest: "b".repeat(64),
            context_digest: "c".repeat(64),
            context_lines: 0,
            context_bytes: 0,
            input_bytes: 8,
            attempts: Vec::new(),
            usage: Usage::default(),
            cost_usd: None,
            cost_reason: Some("no model call".into()),
            error: None,
            started_at: "2026-10-23T00:00:00Z".into(),
            finished_at: "2026-10-23T00:00:01Z".into(),
        };
        let first = owner
            .put_node(
                key,
                "summary".into(),
                Method::Model,
                sources[0].unit.digest.clone(),
                Some(&job),
            )
            .await
            .unwrap();
        let retry = owner
            .put_node(
                key,
                "summary".into(),
                Method::Model,
                sources[0].unit.digest.clone(),
                Some(&job),
            )
            .await
            .unwrap();
        assert_eq!(retry, first);
        assert!(
            owner
                .put_node(
                    key,
                    "changed".into(),
                    Method::Model,
                    sources[0].unit.digest.clone(),
                    Some(&job)
                )
                .await
                .is_err()
        );
        let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM orihon_jobs WHERE room=$1")
            .bind(&room)
            .fetch_one(&pool)
            .await
            .unwrap();
        let nodes: i64 = sqlx::query_scalar("SELECT count(*) FROM orihon_nodes WHERE room=$1")
            .bind(&room)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((jobs, nodes), (1, 1));
    }

    #[tokio::test]
    async fn file_view_revision_starts_at_one_and_survives_open() {
        let room = TempRoom::new();
        let view = View::default();
        assert_eq!(view.revision, 1);
        let (mut owner, _) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        owner.save_views(&view, &view).await.unwrap();
        drop(owner);

        let (_owner, snapshot) = Live::open(room.store(), "seal").await.unwrap().unwrap();
        assert_eq!(snapshot.chat.unwrap().revision, 1);
        assert_eq!(snapshot.compactor.unwrap().revision, 1);
    }
}
