//! Vault-mode store for `worker_lifecycle`.
//!
//! When AKASHA is disabled the Host keeps worker lifecycle state in a runtime
//! journal under its own `state_dir`, outside room and vault content. The
//! rules are `hearth::worker_lifecycle`, exactly as in the PostgreSQL store:
//! lock one swarm aggregate, replay a committed request, apply, persist.
//!
//! The journal is append-only JSONL. Each committed write appends one line
//! holding only the changed swarm/book metadata, the new entries, retained
//! evidence, a disposal receipt, and the request receipt. The whole journal is
//! replayed only on cold load; hot calls work on the in-process index. A
//! whole-swarm disposal compacts the journal with an atomic rewrite so the
//! disposed transient entries leave the live file (backups are not erased).

use crate::HostConfig;
use akasha::AppError;
use atomicwrites::{AllowOverwrite, AtomicFile, Error as AtomicError};
use hearth::worker_lifecycle::{
    Applied, BookState, Call, DispositionRecord, EntryRecord, INSPECT_SWARM_LIMIT, LifecycleError,
    RequestRecord, RetainedRecord, SwarmRecord, SwarmState, Target, apply, changed_books, deliver,
    inspect_caller, page_retained, read_response, render_open, replay,
};
use protocol::ResponsePayload;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

const METHOD: &str = "worker_lifecycle";
const JOURNAL_VERSION: u32 = 1;

/// One committed write. Replay applies lines in order.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Commit {
    version: u32,
    #[serde(default)]
    swarm: Option<SwarmRecord>,
    #[serde(default)]
    books: Vec<BookState>,
    #[serde(default)]
    entries: Vec<EntryRecord>,
    #[serde(default)]
    retained: Vec<RetainedRecord>,
    #[serde(default)]
    disposition: Option<DispositionRecord>,
    #[serde(default)]
    requests: Vec<RequestRecord>,
}

type RequestKey = (String, String, String, String);

fn request_key(record: &RequestRecord) -> RequestKey {
    (
        record.house_id.clone(),
        record.room.clone(),
        record.session.clone(),
        record.request_id.clone(),
    )
}

#[derive(Debug)]
struct Journal {
    path: PathBuf,
    file: Option<File>,
    _ownership_lock: File,
    needs_compaction: bool,
    swarms: HashMap<String, SwarmRecord>,
    books: HashMap<String, BookState>,
    tokens: HashMap<String, String>,
    entries: HashMap<String, Vec<EntryRecord>>,
    retained: Vec<RetainedRecord>,
    dispositions: Vec<DispositionRecord>,
    requests: HashMap<RequestKey, RequestRecord>,
}

#[derive(Debug)]
enum Failure {
    Domain(LifecycleError),
    Io(String),
}

impl From<LifecycleError> for Failure {
    fn from(error: LifecycleError) -> Self {
        Self::Domain(error)
    }
}

impl Failure {
    fn into_app(self) -> AppError {
        match self {
            Self::Domain(LifecycleError::Invalid(message)) => AppError::Invalid(message),
            Self::Domain(LifecycleError::Refused { code, message }) => {
                AppError::Refusal { code, message }
            }
            Self::Io(message) => AppError::Io(io::Error::other(message)),
        }
    }
}

fn io_failure(path: &Path, what: &str, error: impl std::fmt::Display) -> Failure {
    Failure::Io(format!("{what} {}: {error}", path.display()))
}

/// One journal per Host state directory, shared by every connection of this
/// process. A failed write drops the loaded journal so the next call reloads
/// from disk instead of trusting memory that may differ from the file.
static JOURNALS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<Option<Journal>>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn journal_path(host: &HostConfig) -> PathBuf {
    host.state_dir
        .join("worker-lifecycle")
        .join("journal.jsonl")
}

pub(crate) fn call(host: &HostConfig, call: Call) -> ResponsePayload<Value> {
    match execute(&journal_path(host), &call) {
        Ok(result) => ResponsePayload::Result { result },
        Err(failure) => ResponsePayload::Error {
            error: failure.into_app().protocol_error_body(METHOD),
        },
    }
}

fn execute(path: &Path, call: &Call) -> Result<Value, Failure> {
    let slot = {
        let mut journals = JOURNALS
            .lock()
            .map_err(|_| Failure::Io("worker lifecycle journal registry is poisoned".into()))?;
        Arc::clone(journals.entry(path.to_path_buf()).or_default())
    };
    let mut guard = slot
        .lock()
        .map_err(|_| Failure::Io("worker lifecycle journal lock is poisoned".into()))?;
    if guard.is_none() {
        *guard = Some(Journal::load(path)?);
    }
    let journal = guard.as_mut().expect("journal loaded above");
    let result = journal.execute(call);
    if matches!(result, Err(Failure::Io(_))) {
        *guard = None;
    }
    result
}

impl Journal {
    fn load(path: &Path) -> Result<Self, Failure> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_failure(parent, "cannot create", error))?;
        }
        let lock_path = path.with_extension("lock");
        let ownership_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io_failure(&lock_path, "cannot open ownership lock", error))?;
        ownership_lock.try_lock().map_err(|error| {
            io_failure(
                &lock_path,
                "journal already owned or lock unavailable",
                error,
            )
        })?;
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)
            .map_err(|error| io_failure(path, "cannot open", error))?;
        let mut journal = Self {
            path: path.to_path_buf(),
            file: Some(
                file.try_clone()
                    .map_err(|error| io_failure(path, "cannot open", error))?,
            ),
            _ownership_lock: ownership_lock,
            needs_compaction: false,
            swarms: HashMap::new(),
            books: HashMap::new(),
            tokens: HashMap::new(),
            entries: HashMap::new(),
            retained: Vec::new(),
            dispositions: Vec::new(),
            requests: HashMap::new(),
        };
        file.seek(SeekFrom::Start(0))
            .map_err(|error| io_failure(path, "cannot read", error))?;
        let mut reader = BufReader::new(&file);
        let mut offset: u64 = 0;
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader
                .read_until(b'\n', &mut line)
                .map_err(|error| io_failure(path, "cannot read", error))?;
            if read == 0 {
                break;
            }
            if line.last() != Some(&b'\n') {
                // A torn final write was never acknowledged: its caller saw an
                // error and will retry by requestId. Drop it explicitly.
                tracing::warn!(
                    path = %path.display(),
                    offset,
                    "truncating an unterminated worker lifecycle journal tail"
                );
                OpenOptions::new()
                    .write(true)
                    .open(path)
                    .and_then(|writable| writable.set_len(offset))
                    .map_err(|error| io_failure(path, "cannot truncate torn tail of", error))?;
                break;
            }
            let commit: Commit = serde_json::from_slice(&line).map_err(|error| {
                io_failure(
                    path,
                    &format!("corrupt journal line at byte {offset} in"),
                    error,
                )
            })?;
            if commit.version != JOURNAL_VERSION {
                return Err(io_failure(
                    path,
                    "unsupported journal version in",
                    commit.version,
                ));
            }
            journal.absorb(commit);
            offset += read as u64;
        }
        drop(reader);
        drop(file);
        // Finish a disposal whose journal commit survived a crash before
        // the transient rows were removed from the live file.
        if journal.needs_compaction {
            journal.compact()?;
        }
        Ok(journal)
    }

    /// Fold one committed line into the in-process index.
    fn absorb(&mut self, commit: Commit) {
        if let Some(swarm) = commit.swarm {
            self.swarms.insert(swarm.swarm_id.clone(), swarm);
        }
        if let Some(disposition) = commit.disposition {
            let swarm_id = disposition.swarm_id.clone();
            let book_ids = self
                .books
                .values()
                .filter(|book| book.book.swarm_id == swarm_id)
                .map(|book| book.book.book_id.clone())
                .collect::<Vec<_>>();
            self.needs_compaction |= !book_ids.is_empty();
            for book_id in book_ids {
                self.books.remove(&book_id);
                self.entries.remove(&book_id);
            }
            self.tokens.retain(|_, book| self.books.contains_key(book));
            self.requests
                .retain(|_, record| record.swarm_id.as_deref() != Some(swarm_id.as_str()));
            self.dispositions.push(disposition);
        }
        for book in commit.books {
            for instance in &book.instances {
                self.tokens
                    .insert(instance.token_hash.clone(), book.book.book_id.clone());
            }
            self.books.insert(book.book.book_id.clone(), book);
        }
        for entry in commit.entries {
            self.entries
                .entry(entry.book_id.clone())
                .or_default()
                .push(entry);
        }
        self.retained.extend(commit.retained);
        for record in commit.requests {
            self.requests.insert(request_key(&record), record);
        }
    }

    fn aggregate(&self, swarm_id: &str) -> Option<SwarmState> {
        let swarm = self.swarms.get(swarm_id)?.clone();
        let mut books = self
            .books
            .values()
            .filter(|book| book.book.swarm_id == swarm_id)
            .cloned()
            .collect::<Vec<_>>();
        books.sort_by(|a, b| {
            (a.book.created_ms, &a.book.book_id).cmp(&(b.book.created_ms, &b.book.book_id))
        });
        Some(SwarmState { swarm, books })
    }

    fn locate(&self, call: &Call, target: &Target) -> Option<String> {
        let actor = &call.actor;
        match target {
            Target::NewSwarm { swarm_key } => self
                .swarms
                .values()
                .find(|swarm| {
                    swarm.house_id == actor.house_id
                        && swarm.room == actor.room
                        && swarm.owner_session == actor.session
                        && &swarm.swarm_key == swarm_key
                })
                .map(|swarm| swarm.swarm_id.clone()),
            Target::Book(book_id) => self
                .books
                .get(book_id)
                .map(|book| book.book.swarm_id.clone()),
            Target::Swarm(swarm_id) => self.swarms.contains_key(swarm_id).then(|| swarm_id.clone()),
            Target::TokenHash(hash) => self
                .tokens
                .get(hash)
                .and_then(|book_id| self.books.get(book_id))
                .map(|book| book.book.swarm_id.clone()),
            Target::Caller => None,
        }
    }

    fn inspect_all(&self, call: &Call) -> Value {
        let actor = &call.actor;
        let mut owned = self
            .swarms
            .values()
            .filter(|swarm| {
                swarm.house_id == actor.house_id
                    && swarm.room == actor.room
                    && swarm.owner_session == actor.session
                    && swarm.disposed_ms.is_none()
            })
            .collect::<Vec<_>>();
        owned.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        let owned = owned
            .into_iter()
            .take(usize::try_from(INSPECT_SWARM_LIMIT).unwrap_or(usize::MAX))
            .filter_map(|swarm| self.aggregate(&swarm.swarm_id))
            .collect::<Vec<_>>();
        let bound = self
            .books
            .values()
            .filter_map(|book| {
                book.instances
                    .iter()
                    .filter(|instance| instance.bound_session.as_deref() == Some(&actor.session))
                    .map(|instance| (instance.updated_ms, book.book.swarm_id.as_str()))
                    .max()
            })
            .filter(|(_, swarm_id)| {
                self.swarms.get(*swarm_id).is_some_and(|swarm| {
                    swarm.house_id == actor.house_id && swarm.room == actor.room
                })
            })
            .max()
            .and_then(|(_, swarm_id)| self.aggregate(swarm_id));
        inspect_caller(actor, &owned, bound.as_ref())
    }

    fn execute(&mut self, call: &Call) -> Result<Value, Failure> {
        let target = call.target();
        if target == Target::Caller {
            return Ok(self.inspect_all(call));
        }
        let actor = &call.actor;
        if call.action.is_write() {
            let key = (
                actor.house_id.clone(),
                actor.room.clone(),
                actor.session.clone(),
                call.request_id.clone(),
            );
            if let Some(record) = self.requests.get(&key) {
                return Ok(replay(call, record)?);
            }
        }
        let before = self
            .locate(call, &target)
            .and_then(|swarm_id| self.aggregate(&swarm_id));
        let mut aggregate = before.clone();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let step = apply(call, &mut aggregate, now_ms, &mut || {
            uuid::Uuid::new_v4().to_string()
        })?;
        let after = aggregate.expect("an applied lifecycle step keeps its aggregate");
        let response = match step.applied {
            Applied::Response(response) => response,
            Applied::Page {
                book_id,
                after: from,
                limit,
            } => {
                let entries = self.entries.get(&book_id).map_or(&[][..], Vec::as_slice);
                let start = entries.partition_point(|entry| entry.sequence <= from);
                let end = entries
                    .len()
                    .min(start + usize::try_from(limit).unwrap_or(usize::MAX));
                read_response(&entries[start..end], from)
            }
            Applied::Wake { plan, base } => {
                let wanted = plan.sequences();
                let loaded = self
                    .entries
                    .get(&plan.book_id)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter(|entry| wanted.contains(&entry.sequence))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                // Vault mode has no lesson registry: a named familiar's start
                // is refused here before anything is journaled.
                render_open(&plan, base, &loaded, None)?
            }
            Applied::Retained {
                swarm_id,
                after: cursor,
                limit,
            } => page_retained(&self.retained, &swarm_id, cursor.as_deref(), limit)?,
        };
        if !call.action.is_write() {
            return Ok(response);
        }
        let disposed = step.effects.disposition.is_some();
        let swarm_changed = before
            .as_ref()
            .is_none_or(|before| before.swarm != after.swarm);
        let commit = Commit {
            version: JOURNAL_VERSION,
            swarm: swarm_changed.then(|| after.swarm.clone()),
            books: changed_books(before.as_ref(), &after)
                .into_iter()
                .cloned()
                .collect(),
            entries: step.effects.entries,
            retained: step.effects.retained,
            disposition: step.effects.disposition,
            requests: vec![RequestRecord {
                house_id: actor.house_id.clone(),
                room: actor.room.clone(),
                session: actor.session.clone(),
                request_id: call.request_id.clone(),
                action: call.action.name().to_owned(),
                fingerprint: call.fingerprint(),
                response: response.clone(),
                // A disposal receipt outlives the swarm's transient receipts.
                swarm_id: (!disposed).then(|| after.swarm.swarm_id.clone()),
                created_ms: now_ms,
            }],
        };
        self.append(&commit)?;
        self.absorb(commit);
        if disposed {
            self.compact()?;
        }
        Ok(deliver(call, response))
    }

    fn append(&mut self, commit: &Commit) -> Result<(), Failure> {
        let mut line = serde_json::to_vec(commit)
            .map_err(|error| io_failure(&self.path, "cannot serialize a commit for", error))?;
        line.push(b'\n');
        let file = self.file.as_mut().ok_or_else(|| {
            io_failure(
                &self.path,
                "cannot append to closed journal",
                "reload required",
            )
        })?;
        file.write_all(&line)
            .and_then(|()| file.sync_data())
            .map_err(|error| io_failure(&self.path, "cannot append to", error))
    }

    /// Rewrite the journal from the live index after a disposal, so disposed
    /// entries leave the live file. One line per swarm keeps each line bounded
    /// by one swarm's metadata and entries.
    fn compact(&mut self) -> Result<(), Failure> {
        let mut bytes = Vec::new();
        let mut push = |commit: &Commit| -> Result<(), Failure> {
            serde_json::to_writer(&mut bytes, commit)
                .map_err(|error| io_failure(&self.path, "cannot serialize compaction of", error))?;
            bytes.push(b'\n');
            Ok(())
        };
        let mut swarm_ids = self.swarms.keys().cloned().collect::<Vec<_>>();
        swarm_ids.sort();
        for swarm_id in &swarm_ids {
            let state = self.aggregate(swarm_id).expect("indexed swarm");
            let entries = state
                .books
                .iter()
                .flat_map(|book| {
                    self.entries
                        .get(&book.book.book_id)
                        .into_iter()
                        .flatten()
                        .cloned()
                })
                .collect();
            push(&Commit {
                version: JOURNAL_VERSION,
                swarm: Some(state.swarm),
                books: state.books,
                entries,
                ..Commit::default()
            })?;
        }
        push(&Commit {
            version: JOURNAL_VERSION,
            retained: self.retained.clone(),
            requests: self.requests.values().cloned().collect(),
            ..Commit::default()
        })?;
        for disposition in &self.dispositions {
            // Receipts replay last; their swarms hold no books by now.
            push(&Commit {
                version: JOURNAL_VERSION,
                disposition: Some(disposition.clone()),
                ..Commit::default()
            })?;
        }
        // Windows refuses replacement while our append handle is open.
        // The separate ownership lock remains held across the replacement.
        drop(self.file.take());
        let file = AtomicFile::new(&self.path, AllowOverwrite);
        file.write(|target| -> io::Result<()> {
            target.write_all(&bytes)?;
            target.sync_all()
        })
        .map_err(|error| match error {
            AtomicError::Internal(error) | AtomicError::User(error) => {
                io_failure(&self.path, "cannot atomically compact", error)
            }
        })?;
        self.file = Some(
            OpenOptions::new()
                .append(true)
                .open(&self.path)
                .map_err(|error| io_failure(&self.path, "cannot reopen", error))?,
        );
        self.needs_compaction = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
