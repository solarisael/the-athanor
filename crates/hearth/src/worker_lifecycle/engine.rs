use super::familiar::{Familiar, TaggedLesson, role_lessons};
use super::model::{
    BookRecord, BookState, DispositionRecord, EntryAuthor, EntryRecord, HandoffRecord,
    InstanceRecord, InstanceState, OperationRecord, RequestRecord, RetainedRecord, SwarmRecord,
    SwarmState,
};
use super::request::{
    Accept, Action, Actor, Append, Call, Checkpoint, Dispose, Observe, Open, Prepare, Read, Repair,
    Settle, SettleOutcome, token_hash,
};
use super::wake::{WakeKind, WakePlan, render};
use super::{
    BODY_LIMIT_BYTES, HANDOFF_PERCENT, HANDOFF_REQUEST_LIMIT, INSPECT_BOOK_LIMIT,
    INSPECT_INSTANCE_LIMIT, PENDING_LIMIT, READ_LIMIT_DEFAULT, REPORT_LIMIT_BYTES,
    TASK_EXCERPT_CHARS, WARN_PERCENT,
};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq)]
pub enum LifecycleError {
    Invalid(String),
    /// Static code and message: refusals name the rule, never echo state.
    Refused {
        code: &'static str,
        message: &'static str,
    },
}

pub const fn refused(code: &'static str, message: &'static str) -> LifecycleError {
    LifecycleError::Refused { code, message }
}

const UNKNOWN_BOOK: LifecycleError = refused("unknown_book", "the requested book does not exist");
const NOT_AUTHORIZED: LifecycleError = refused(
    "not_authorized",
    "only the owning parent or this book's bound worker may do this",
);
const PARENT_ONLY: LifecycleError = refused(
    "parent_only",
    "only the parent session that owns this swarm may do this",
);
const WORKER_ONLY: LifecycleError = refused(
    "worker_only",
    "only the worker session bound to this book may do this",
);
const RETIRED: LifecycleError = refused(
    "instance_retired",
    "this worker instance is retired and cannot work or resume",
);
const DISPOSED: LifecycleError = refused("swarm_disposed", "this swarm was disposed");
const UNKNOWN_INSTANCE: LifecycleError = refused(
    "unknown_instance",
    "the requested instance does not exist in this book",
);

/// Rows a store must persist alongside the mutated aggregate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    pub entries: Vec<EntryRecord>,
    pub retained: Vec<RetainedRecord>,
    pub disposition: Option<DispositionRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Applied {
    /// Final response body (without any echoed token; see [`replay`]).
    Response(Value),
    /// Authorized read: the store pages entries, then calls [`read_response`].
    Page {
        book_id: String,
        after: i64,
        limit: i64,
    },
    /// Authorized open: the store loads [`WakePlan::sequences`] from the book
    /// and, for [`WakePlan::familiar`], the familiar's tagged coding lessons,
    /// then calls [`render_open`] before it commits.
    Wake { plan: WakePlan, base: Value },
    /// Authorized retained read: the store pages this swarm's accepted
    /// evidence in (createdMs, retainedId) order, starting after the cursor
    /// record, which must belong to the same swarm ([`unknown_cursor`]).
    /// It loads up to `limit + 1` records, then calls [`retained_response`].
    Retained {
        swarm_id: String,
        after: Option<String>,
        limit: i64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub effects: Effects,
    pub applied: Applied,
}

/// Replay a committed request. The same requestId with a different payload
/// is refused rather than silently answered with the earlier result.
pub fn replay(call: &Call, record: &RequestRecord) -> Result<Value, LifecycleError> {
    if record.fingerprint != call.fingerprint() || record.action != call.action.name() {
        return Err(refused(
            "request_id_conflict",
            "this requestId was already used for a different request",
        ));
    }
    Ok(deliver(call, record.response.clone()))
}

/// Attach the caller's own token to a prepare response. Stored responses never
/// contain it; it is echoed from the request, never read from storage.
pub fn deliver(call: &Call, mut response: Value) -> Value {
    if let (Some(token), Some(object)) = (call.echoed_token(), response.as_object_mut()) {
        object.insert("token".into(), Value::String(token.to_owned()));
    }
    response
}

pub fn read_response(entries: &[EntryRecord], after: i64) -> Value {
    let next = entries.last().map_or(after, |entry| entry.sequence);
    json!({
        "ok": true,
        "entries": entries.iter().map(|entry| json!({
            "sequence": entry.sequence,
            "instanceId": entry.instance_id,
            "kind": entry.kind,
            "body": entry.body,
        })).collect::<Vec<_>>(),
        "nextSequence": next,
    })
}

/// Render an authorized open. Every open answers `familiarLessons`: `null`
/// for an ordinary agent, or a named familiar's role lessons beside the
/// clipped wake text, on every wake. `lessons` is what the store's lesson
/// registry returned for [`Familiar::tags`], or `None` from a store without
/// one. A refusal here must leave the open uncommitted.
pub fn render_open(
    plan: &WakePlan,
    mut base: Value,
    entries: &[EntryRecord],
    lessons: Option<Vec<TaggedLesson>>,
) -> Result<Value, LifecycleError> {
    let delivered = match plan.familiar {
        Some(familiar) => role_lessons(familiar, lessons)?,
        None => Value::Null,
    };
    let object = base.as_object_mut().expect("open base is an object");
    object.insert("familiarLessons".into(), delivered);
    let text = render(plan, entries);
    object.insert(
        "wake".into(),
        json!({"kind": plan.kind.as_str(), "text": text}),
    );
    Ok(base)
}

pub const fn unknown_cursor() -> LifecycleError {
    refused(
        "unknown_cursor",
        "the cursor is not a retained record of this swarm",
    )
}

/// Build the retained page from up to `limit + 1` ordered records.
pub fn retained_response(mut records: Vec<RetainedRecord>, limit: i64) -> Value {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = records.len() > limit;
    records.truncate(limit);
    let next = more
        .then(|| records.last().map(|record| record.retained_id.clone()))
        .flatten();
    json!({
        "ok": true,
        "retained": records,
        "nextCursor": next,
    })
}

/// [`Applied::Retained`] over an in-memory collection (Vault journal).
pub fn page_retained(
    all: &[RetainedRecord],
    swarm_id: &str,
    after: Option<&str>,
    limit: i64,
) -> Result<Value, LifecycleError> {
    let mut records = all
        .iter()
        .filter(|record| record.swarm_id == swarm_id)
        .collect::<Vec<_>>();
    records.sort_by(|a, b| (a.created_ms, &a.retained_id).cmp(&(b.created_ms, &b.retained_id)));
    let start = match after {
        None => 0,
        Some(cursor) => {
            records
                .iter()
                .position(|record| record.retained_id == cursor)
                .ok_or_else(unknown_cursor)?
                + 1
        }
    };
    let take = usize::try_from(limit)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let page = records
        .into_iter()
        .skip(start)
        .take(take)
        .cloned()
        .collect();
    Ok(retained_response(page, limit))
}

fn same_room(swarm: &SwarmRecord, actor: &Actor) -> bool {
    swarm.house_id == actor.house_id && swarm.room == actor.room
}

fn is_owner(swarm: &SwarmRecord, actor: &Actor) -> bool {
    same_room(swarm, actor) && swarm.owner_session == actor.session
}

fn bound_index(book: &BookState, actor: &Actor) -> Option<usize> {
    book.instances
        .iter()
        .position(|instance| instance.bound_session.as_deref() == Some(actor.session.as_str()))
}

fn json_size(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

fn require_report(report: &Value) -> Result<(), LifecycleError> {
    if !nonempty(report) {
        return Err(refused("empty_report", "a nonempty report is required"));
    }
    if json_size(report) > REPORT_LIMIT_BYTES {
        return Err(refused(
            "report_too_large",
            "the report exceeds the stored size bound; excerpt it and cite the transcript",
        ));
    }
    Ok(())
}

fn excerpt(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let mut clipped: String = text.chars().take(limit).collect();
        clipped.push_str(" [excerpt]");
        clipped
    }
}

struct Writer<'a> {
    state: &'a mut SwarmState,
    effects: Effects,
    now_ms: i64,
}

impl Writer<'_> {
    fn push(
        &mut self,
        book: usize,
        instance: Option<usize>,
        author: EntryAuthor,
        kind: &str,
        body: Value,
    ) -> i64 {
        let swarm_id = self.state.swarm.swarm_id.clone();
        let now_ms = self.now_ms;
        let book_state = &mut self.state.books[book];
        let sequence = book_state.book.next_sequence;
        book_state.book.next_sequence += 1;
        let instance_id = instance.map(|index| {
            let record = &mut book_state.instances[index];
            if author == EntryAuthor::Worker {
                record.own_entries += 1;
                record.last_own_sequence = Some(sequence);
            }
            record.updated_ms = now_ms;
            record.instance_id.clone()
        });
        self.effects.entries.push(EntryRecord {
            swarm_id,
            book_id: book_state.book.book_id.clone(),
            sequence,
            instance_id,
            author,
            kind: kind.to_owned(),
            body,
            created_ms: now_ms,
        });
        sequence
    }

    fn lifecycle(&mut self, book: usize, instance: usize, body: Value) -> i64 {
        self.push(
            book,
            Some(instance),
            EntryAuthor::Lifecycle,
            "lifecycle",
            body,
        )
    }

    fn done(self, response: Value) -> Step {
        Step {
            effects: self.effects,
            applied: Applied::Response(response),
        }
    }
}

fn pending_json(operations: &[OperationRecord]) -> Value {
    Value::Array(
        operations
            .iter()
            .take(PENDING_LIMIT)
            .map(|operation| serde_json::to_value(operation).expect("operation serializes"))
            .collect(),
    )
}

fn instance_view(instance: &InstanceRecord) -> Value {
    let handoff = match &instance.handoff {
        None => "none",
        Some(handoff) if handoff.complete => "complete",
        Some(handoff) if handoff.repair_sequence.is_some() => "repaired",
        Some(_) => "incomplete",
    };
    json!({
        "instanceId": instance.instance_id,
        "ordinal": instance.ordinal,
        "agentId": instance.agent_id,
        "agentName": instance.agent_name,
        "workerSession": instance.bound_session,
        "state": instance.state.as_str(),
        "handoffComplete": instance.handoff_usable(),
        "handoff": handoff,
        "settled": instance.settled_outcome,
        "predecessorId": instance.predecessor_instance_id,
        "handoffRequests": instance.handoff_requests,
        "warned": instance.warned,
        "transcriptPath": instance.transcript_path,
    })
}

fn book_view(book: &BookState) -> Value {
    let skip = book.instances.len().saturating_sub(INSPECT_INSTANCE_LIMIT);
    json!({
        "swarmId": book.book.swarm_id,
        "bookId": book.book.book_id,
        "task": book.book.task_excerpt,
        "slotKey": book.book.slot_key,
        "agentName": book.book.agent_name,
        "lastSequence": book.book.next_sequence - 1,
        "acceptedCount": book.book.accepted_count,
        "pendingOperations": pending_json(&book.operations),
        "instances": book.instances[skip..].iter().map(instance_view).collect::<Vec<_>>(),
    })
}

/// inspect without bookId: books of the caller's owned swarms (store-bounded)
/// plus the one book the caller's session is bound to, never anyone else's.
pub fn inspect_caller(actor: &Actor, owned: &[SwarmState], bound: Option<&SwarmState>) -> Value {
    let mut books = owned
        .iter()
        .filter(|swarm| is_owner(&swarm.swarm, actor))
        .flat_map(|swarm| swarm.books.iter())
        .take(INSPECT_BOOK_LIMIT)
        .map(book_view)
        .collect::<Vec<_>>();
    let own = bound
        .filter(|swarm| same_room(&swarm.swarm, actor))
        .and_then(|swarm| {
            swarm
                .books
                .iter()
                .find(|book| bound_index(book, actor).is_some())
        });
    if let Some(book) = own {
        let listed = books
            .iter()
            .any(|view| view["bookId"].as_str() == Some(book.book.book_id.as_str()));
        if !listed {
            books.push(book_view(book));
        }
    }
    json!({"ok": true, "books": books})
}

/// Book metadata rows a store must rewrite after [`apply`]: only books whose
/// record, instances, or open operations differ from the locked snapshot.
/// Entry bodies are never part of this; they are appended separately.
pub fn changed_books<'a>(before: Option<&SwarmState>, after: &'a SwarmState) -> Vec<&'a BookState> {
    after
        .books
        .iter()
        .filter(|book| {
            before
                .and_then(|before| {
                    before
                        .books
                        .iter()
                        .find(|old| old.book.book_id == book.book.book_id)
                })
                .is_none_or(|old| old != *book)
        })
        .collect()
}

/// Apply one call to its locked swarm aggregate. `aggregate` is `None` when
/// the store found nothing for [`Call::target`]; prepare then creates it.
pub fn apply(
    call: &Call,
    aggregate: &mut Option<SwarmState>,
    now_ms: i64,
    next_id: &mut dyn FnMut() -> String,
) -> Result<Step, LifecycleError> {
    let actor = &call.actor;
    if let Action::Prepare(prepare) = &call.action {
        if prepare.predecessor.is_none() {
            return prepare_new(actor, prepare, aggregate, now_ms, next_id);
        }
    }
    if let Action::Open(open) = &call.action {
        return open_reservation(actor, open, aggregate, now_ms);
    }
    if let Action::Dispose(dispose) = &call.action {
        return dispose_swarm(actor, dispose, aggregate, now_ms);
    }
    if let Action::Retained(retained) = &call.action {
        // Works before and after disposal: the swarm row and its accepted
        // evidence outlive the transient book.
        const UNKNOWN: LifecycleError =
            refused("unknown_swarm", "the requested swarm does not exist");
        let state = aggregate.as_ref().ok_or(UNKNOWN)?;
        if !same_room(&state.swarm, actor) {
            return Err(UNKNOWN);
        }
        if !is_owner(&state.swarm, actor) {
            return Err(PARENT_ONLY);
        }
        return Ok(Step {
            effects: Effects::default(),
            applied: Applied::Retained {
                swarm_id: state.swarm.swarm_id.clone(),
                after: retained.after.clone(),
                limit: retained.limit.unwrap_or(READ_LIMIT_DEFAULT),
            },
        });
    }
    let book_id = match call.target() {
        super::request::Target::Book(book_id) => book_id,
        _ => {
            return Err(LifecycleError::Invalid(
                "this action needs a bookId target".into(),
            ));
        }
    };
    let state = aggregate.as_mut().ok_or(UNKNOWN_BOOK)?;
    if !same_room(&state.swarm, actor) {
        return Err(UNKNOWN_BOOK);
    }
    if state.swarm.disposed_ms.is_some() {
        return Err(DISPOSED);
    }
    let book = state
        .books
        .iter()
        .position(|book| book.book.book_id == book_id)
        .ok_or(UNKNOWN_BOOK)?;
    let owner = is_owner(&state.swarm, actor);
    let worker = bound_index(&state.books[book], actor);
    let mut writer = Writer {
        state,
        effects: Effects::default(),
        now_ms,
    };
    match &call.action {
        Action::Prepare(prepare) => prepare_successor(&mut writer, prepare, book, owner, next_id),
        Action::Read(read) => read_page(&writer, read, book, owner, worker),
        Action::Append(append) => append_entry(writer, append, book, owner, worker),
        Action::Observe(observe) => observe_usage(writer, observe, book, worker),
        Action::Checkpoint(checkpoint) => checkpoint_report(writer, checkpoint, book, worker),
        Action::Settle(settle) => settle_instance(writer, settle, book, owner, worker),
        Action::Inspect(_) => {
            if worker.is_none() && !owner {
                return Err(NOT_AUTHORIZED);
            }
            let response = json!({"ok": true, "books": [book_view(&writer.state.books[book])]});
            Ok(writer.done(response))
        }
        Action::Accept(accept) => accept_evidence(writer, actor, accept, book, owner, next_id),
        Action::Repair(repair) => repair_handoff(writer, repair, book, owner),
        Action::Open(_) | Action::Dispose(_) | Action::Retained(_) => {
            unreachable!("handled before book lookup")
        }
    }
}

fn reservation_key(prepare: &Prepare) -> String {
    // Length-prefix the first component: PostgreSQL text cannot contain NUL.
    format!(
        "{}:{}{}",
        prepare.swarm_key.len(),
        prepare.swarm_key,
        prepare.slot_key
    )
}

fn new_instance(
    instance_id: String,
    book_id: &str,
    ordinal: i64,
    predecessor: Option<String>,
    prepare: &Prepare,
    now_ms: i64,
) -> InstanceRecord {
    InstanceRecord {
        instance_id,
        book_id: book_id.to_owned(),
        ordinal,
        predecessor_instance_id: predecessor,
        reservation_key: reservation_key(prepare),
        token_hash: token_hash(&prepare.token),
        agent_name: prepare.agent_name.clone(),
        bound_session: None,
        agent_id: None,
        transcript_path: None,
        state: InstanceState::Working,
        warned: false,
        handoff_requests: 0,
        checkpoint_sequence: None,
        handoff: None,
        settled_outcome: None,
        own_entries: 0,
        last_own_sequence: None,
        opened_count: 0,
        created_ms: now_ms,
        updated_ms: now_ms,
    }
}

fn prepared(state: &SwarmState, book: usize) -> Value {
    let book_state = &state.books[book];
    let instance = book_state
        .instances
        .last()
        .expect("prepared book has an instance");
    json!({
        "ok": true,
        "swarmId": state.swarm.swarm_id,
        "bookId": book_state.book.book_id,
        "instanceId": instance.instance_id,
        "state": instance.state.as_str(),
    })
}

fn prepare_new(
    actor: &Actor,
    prepare: &Prepare,
    aggregate: &mut Option<SwarmState>,
    now_ms: i64,
    next_id: &mut dyn FnMut() -> String,
) -> Result<Step, LifecycleError> {
    let state = aggregate.get_or_insert_with(|| SwarmState {
        swarm: SwarmRecord {
            swarm_id: next_id(),
            house_id: actor.house_id.clone(),
            room: actor.room.clone(),
            owner_spirit: actor.spirit.clone(),
            owner_session: actor.session.clone(),
            swarm_key: prepare.swarm_key.clone(),
            created_ms: now_ms,
            disposed_ms: None,
        },
        books: Vec::new(),
    });
    if !is_owner(&state.swarm, actor) {
        return Err(PARENT_ONLY);
    }
    if state.swarm.disposed_ms.is_some() {
        return Err(DISPOSED);
    }
    if state
        .books
        .iter()
        .any(|book| book.book.slot_key == prepare.slot_key)
    {
        return Err(refused(
            "slot_already_prepared",
            "this task slot already has a book; retry with the original requestId",
        ));
    }
    let book_id = next_id();
    let instance = new_instance(next_id(), &book_id, 1, None, prepare, now_ms);
    let instance_id = instance.instance_id.clone();
    state.books.push(BookState {
        book: BookRecord {
            book_id,
            swarm_id: state.swarm.swarm_id.clone(),
            slot_key: prepare.slot_key.clone(),
            agent_name: prepare.agent_name.clone(),
            task_excerpt: excerpt(&prepare.task, TASK_EXCERPT_CHARS),
            next_sequence: 1,
            accepted_count: 0,
            created_ms: now_ms,
        },
        instances: vec![instance],
        operations: Vec::new(),
    });
    let book = state.books.len() - 1;
    let mut writer = Writer {
        state,
        effects: Effects::default(),
        now_ms,
    };
    writer.lifecycle(
        book,
        0,
        json!({"event": "reserved", "instanceId": instance_id, "agentName": prepare.agent_name}),
    );
    let response = prepared(writer.state, book);
    Ok(writer.done(response))
}

fn prepare_successor(
    writer: &mut Writer<'_>,
    prepare: &Prepare,
    book: usize,
    owner: bool,
    next_id: &mut dyn FnMut() -> String,
) -> Result<Step, LifecycleError> {
    let predecessor = prepare
        .predecessor
        .as_ref()
        .expect("successor has predecessor");
    if !owner {
        return Err(PARENT_ONLY);
    }
    let book_state = &writer.state.books[book];
    let index = book_state
        .instances
        .iter()
        .position(|instance| instance.instance_id == predecessor.instance_id)
        .ok_or(UNKNOWN_INSTANCE)?;
    if index + 1 != book_state.instances.len() {
        return Err(refused(
            "predecessor_superseded",
            "this predecessor already has a successor",
        ));
    }
    let previous = &book_state.instances[index];
    if previous.state != InstanceState::Retired {
        return Err(refused(
            "predecessor_active",
            "the predecessor has not retired; settle it before creating a successor",
        ));
    }
    if !previous.handoff_usable() {
        return Err(refused(
            "handoff_incomplete",
            "the predecessor handoff is incomplete; inspect and repair it before creating a successor",
        ));
    }
    let reservation_key = reservation_key(prepare);
    if writer.state.books.iter().any(|book| {
        book.instances
            .iter()
            .any(|instance| instance.reservation_key == reservation_key)
    }) {
        return Err(refused(
            "reservation_key_taken",
            "this task call slot already reserved an instance; retry with the original requestId",
        ));
    }
    let ordinal = previous.ordinal + 1;
    let previous_id = previous.instance_id.clone();
    let book_id = book_state.book.book_id.clone();
    let instance = new_instance(
        next_id(),
        &book_id,
        ordinal,
        Some(previous_id.clone()),
        prepare,
        writer.now_ms,
    );
    let instance_id = instance.instance_id.clone();
    writer.state.books[book].instances.push(instance);
    let index = writer.state.books[book].instances.len() - 1;
    writer.lifecycle(
        book,
        index,
        json!({
            "event": "successor_reserved",
            "instanceId": instance_id,
            "predecessorInstanceId": previous_id,
            "agentName": prepare.agent_name,
        }),
    );
    let response = prepared(writer.state, book);
    let effects = std::mem::take(&mut writer.effects);
    Ok(Step {
        effects,
        applied: Applied::Response(response),
    })
}

fn open_reservation(
    actor: &Actor,
    open: &Open,
    aggregate: &mut Option<SwarmState>,
    now_ms: i64,
) -> Result<Step, LifecycleError> {
    const UNKNOWN: LifecycleError = refused(
        "unknown_reservation",
        "no reservation matches this enrollment token",
    );
    let state = aggregate.as_mut().ok_or(UNKNOWN)?;
    if !same_room(&state.swarm, actor) {
        return Err(UNKNOWN);
    }
    if state.swarm.disposed_ms.is_some() {
        return Err(DISPOSED);
    }
    let hash = token_hash(&open.token);
    let (book, index) = state
        .books
        .iter()
        .enumerate()
        .find_map(|(book, state)| {
            state
                .instances
                .iter()
                .position(|instance| instance.token_hash == hash)
                .map(|index| (book, index))
        })
        .ok_or(UNKNOWN)?;
    let book_state = &state.books[book];
    let instance = &book_state.instances[index];
    if instance.state == InstanceState::Retired {
        return Err(RETIRED);
    }
    let kind = match instance.bound_session.as_deref() {
        Some(session) if session != actor.session => {
            return Err(refused(
                "reservation_claimed",
                "this reservation is bound to another session",
            ));
        }
        Some(_) => WakeKind::Resume,
        None => {
            if bound_index(book_state, actor).is_some() {
                return Err(refused(
                    "session_already_bound",
                    "this session is already bound to another instance of this book",
                ));
            }
            if instance.predecessor_instance_id.is_some() {
                WakeKind::Successor
            } else {
                WakeKind::Opening
            }
        }
    };
    if instance.agent_name != open.agent_name {
        return Err(refused(
            "agent_mismatch",
            "the opening agent does not match the reservation",
        ));
    }
    if !open.accepts_familiar_lessons && Familiar::named(&instance.agent_name).is_some() {
        return Err(refused(
            "familiar_lessons_unsupported",
            "this opener cannot render familiar role lessons; update the adapter",
        ));
    }
    let mut writer = Writer {
        state,
        effects: Effects::default(),
        now_ms,
    };
    {
        let record = &mut writer.state.books[book].instances[index];
        record.bound_session = Some(actor.session.clone());
        record.agent_id = Some(open.agent_id.clone());
        if open.transcript_path.is_some() {
            record.transcript_path = open.transcript_path.clone();
        }
        record.opened_count += 1;
        record.settled_outcome = None;
        record.updated_ms = now_ms;
    }
    let event = if kind == WakeKind::Resume {
        "resumed"
    } else {
        "opened"
    };
    writer.lifecycle(
        book,
        index,
        json!({"event": event, "agentId": open.agent_id, "transcriptPath": open.transcript_path}),
    );
    let state = &*writer.state;
    let book_state = &state.books[book];
    let instance = &book_state.instances[index];
    let predecessor = (kind == WakeKind::Successor)
        .then(|| {
            index
                .checked_sub(1)
                .map(|previous| &book_state.instances[previous])
        })
        .flatten();
    let handoff = predecessor.and_then(|previous| previous.handoff.as_ref());
    let plan = WakePlan {
        kind,
        swarm_id: state.swarm.swarm_id.clone(),
        book_id: book_state.book.book_id.clone(),
        instance_id: instance.instance_id.clone(),
        ordinal: instance.ordinal,
        agent_name: instance.agent_name.clone(),
        state: instance.state,
        handoff_requests: instance.handoff_requests,
        last_sequence: book_state.book.next_sequence - 1,
        own_entries: instance.own_entries,
        last_own_sequence: instance.last_own_sequence,
        predecessor_instance_id: instance.predecessor_instance_id.clone(),
        checkpoint_sequence: (kind == WakeKind::Resume)
            .then_some(instance.checkpoint_sequence)
            .flatten(),
        handoff_sequence: handoff.map(|handoff| handoff.sequence),
        repair_sequence: handoff.and_then(|handoff| handoff.repair_sequence),
        pending: book_state
            .operations
            .iter()
            .take(PENDING_LIMIT)
            .cloned()
            .collect(),
        familiar: Familiar::named(&instance.agent_name),
    };
    let base = json!({
        "ok": true,
        "swarmId": plan.swarm_id,
        "bookId": plan.book_id,
        "instanceId": plan.instance_id,
        "state": instance.state.as_str(),
        "lastSequence": plan.last_sequence,
        "handoffRequests": instance.handoff_requests,
        "pendingOperations": pending_json(&book_state.operations),
    });
    Ok(Step {
        effects: writer.effects,
        applied: Applied::Wake { plan, base },
    })
}

fn read_page(
    writer: &Writer<'_>,
    read: &Read,
    book: usize,
    owner: bool,
    worker: Option<usize>,
) -> Result<Step, LifecycleError> {
    match worker {
        Some(index)
            if writer.state.books[book].instances[index].state == InstanceState::Retired =>
        {
            return Err(RETIRED);
        }
        Some(_) => {}
        None if owner => {}
        None => return Err(NOT_AUTHORIZED),
    }
    Ok(Step {
        effects: Effects::default(),
        applied: Applied::Page {
            book_id: read.book_id.clone(),
            after: read.after.unwrap_or(0),
            limit: read.limit.unwrap_or(READ_LIMIT_DEFAULT),
        },
    })
}

fn append_entry(
    mut writer: Writer<'_>,
    append: &Append,
    book: usize,
    owner: bool,
    worker: Option<usize>,
) -> Result<Step, LifecycleError> {
    let tool = matches!(append.kind.as_str(), "tool-start" | "tool-end");
    let author = match worker {
        Some(index) => {
            if writer.state.books[book].instances[index].state == InstanceState::Retired {
                return Err(RETIRED);
            }
            if !matches!(
                append.kind.as_str(),
                "note" | "message" | "tool-start" | "tool-end"
            ) {
                return Err(refused(
                    "kind_not_allowed",
                    "workers append only note, message, tool-start, or tool-end",
                ));
            }
            EntryAuthor::Worker
        }
        None if owner => {
            if !matches!(append.kind.as_str(), "evidence" | "briefing") {
                return Err(refused(
                    "kind_not_allowed",
                    "parents append only evidence or briefing",
                ));
            }
            EntryAuthor::Parent
        }
        None => return Err(NOT_AUTHORIZED),
    };
    match (tool, append.operation_id.as_deref()) {
        (true, None) => {
            return Err(LifecycleError::Invalid(
                "tool-start and tool-end require operationId".into(),
            ));
        }
        (false, Some(_)) => {
            return Err(LifecycleError::Invalid(
                "operationId belongs only to tool-start and tool-end".into(),
            ));
        }
        _ => {}
    }
    if append.body.is_null() {
        return Err(LifecycleError::Invalid("body must not be null".into()));
    }
    if json_size(&append.body) > BODY_LIMIT_BYTES {
        return Err(refused(
            "body_too_large",
            "the entry body exceeds the stored size bound; excerpt it and cite the transcript",
        ));
    }
    let instance_id = worker.map(|index| {
        writer.state.books[book].instances[index]
            .instance_id
            .clone()
    });
    if let (Some(index), Some(operation_id)) = (worker, append.operation_id.as_deref()) {
        let book_state = &writer.state.books[book];
        let open = book_state
            .operations
            .iter()
            .position(|operation| operation.operation_id == operation_id);
        if append.kind == "tool-start" {
            if book_state.instances[index].state == InstanceState::HandoffOnly {
                return Err(refused(
                    "handoff_only",
                    "handoff-only mode permits no new tool operations",
                ));
            }
            if open.is_some() {
                return Err(refused(
                    "operation_already_open",
                    "this operation already has an unresolved start",
                ));
            }
        } else {
            match open {
                Some(position)
                    if Some(&book_state.operations[position].instance_id)
                        == instance_id.as_ref() => {}
                _ => {
                    return Err(refused(
                        "unknown_operation",
                        "this instance has no unresolved start for the operation",
                    ));
                }
            }
        }
    }
    let sequence = writer.push(book, worker, author, &append.kind, append.body.clone());
    if let (Some(operation_id), Some(instance_id)) = (append.operation_id.as_deref(), instance_id) {
        let operations = &mut writer.state.books[book].operations;
        if append.kind == "tool-start" {
            operations.push(OperationRecord {
                operation_id: operation_id.to_owned(),
                instance_id,
                started_sequence: sequence,
            });
        } else {
            operations.retain(|operation| operation.operation_id != operation_id);
        }
    }
    Ok(writer.done(json!({"ok": true, "sequence": sequence})))
}

fn observe_usage(
    mut writer: Writer<'_>,
    observe: &Observe,
    book: usize,
    worker: Option<usize>,
) -> Result<Step, LifecycleError> {
    let index = worker.ok_or(WORKER_ONLY)?;
    if writer.state.books[book].instances[index].state == InstanceState::Retired {
        return Err(RETIRED);
    }
    if observe.tokens < 0 || observe.context_window <= 0 {
        return Err(refused(
            "usage_invalid",
            "context usage needs nonnegative tokens and a positive context window",
        ));
    }
    let used = i128::from(observe.tokens) * 100;
    let window = i128::from(observe.context_window);
    let now_ms = writer.now_ms;
    let record = &mut writer.state.books[book].instances[index];
    let before = record.state;
    let target = if used >= window * HANDOFF_PERCENT {
        InstanceState::HandoffOnly
    } else if used >= window * WARN_PERCENT {
        InstanceState::Wrapping
    } else {
        InstanceState::Working
    };
    // Monotonic: usage dropping (compaction) never lowers the latch.
    record.state = record.state.max(target);
    let warn = !record.warned && record.state >= InstanceState::Wrapping;
    if warn {
        record.warned = true;
    }
    let mut retire_required = false;
    if record.state == InstanceState::HandoffOnly && observe.request_started == Some(true) {
        if record.handoff_requests < HANDOFF_REQUEST_LIMIT {
            record.handoff_requests += 1;
        } else {
            retire_required = true;
        }
    }
    record.updated_ms = now_ms;
    let after = record.state;
    let handoff_requests = record.handoff_requests;
    let remaining = if after == InstanceState::HandoffOnly {
        HANDOFF_REQUEST_LIMIT - handoff_requests
    } else {
        HANDOFF_REQUEST_LIMIT
    };
    if after != before {
        writer.lifecycle(
            book,
            index,
            json!({
                "event": "state",
                "from": before.as_str(),
                "to": after.as_str(),
                "tokens": observe.tokens,
                "contextWindow": observe.context_window,
            }),
        );
    }
    if retire_required {
        writer.lifecycle(
            book,
            index,
            json!({"event": "handoff_requests_exhausted", "handoffRequests": handoff_requests}),
        );
    }
    Ok(writer.done(json!({
        "ok": true,
        "state": after.as_str(),
        "warn": warn,
        "handoffRequests": handoff_requests,
        "remainingRequests": remaining,
        "retireRequired": retire_required,
    })))
}

fn retire(
    writer: &mut Writer<'_>,
    book: usize,
    index: usize,
    complete: bool,
    outcome: &str,
    report: Option<&Value>,
    transcript_path: Option<&String>,
    author: EntryAuthor,
) {
    let unresolved = writer.state.books[book].operations.clone();
    let transcript_path = transcript_path.cloned().or_else(|| {
        writer.state.books[book].instances[index]
            .transcript_path
            .clone()
    });
    let sequence = writer.push(
        book,
        Some(index),
        author,
        "handoff",
        json!({
            "complete": complete,
            "outcome": outcome,
            "report": report.cloned().unwrap_or(Value::Null),
            "transcriptPath": transcript_path,
            "unresolvedOperations": pending_json(&unresolved),
        }),
    );
    let record = &mut writer.state.books[book].instances[index];
    record.handoff = Some(HandoffRecord {
        complete,
        outcome: outcome.to_owned(),
        sequence,
        unresolved,
        transcript_path: transcript_path.clone(),
        repair_sequence: None,
    });
    record.transcript_path = transcript_path;
    record.state = InstanceState::Retired;
}

fn lifecycle_result(writer: Writer<'_>, book: usize, index: usize) -> Step {
    let book_state = &writer.state.books[book];
    let instance = &book_state.instances[index];
    let response = json!({
        "ok": true,
        "state": instance.state.as_str(),
        "bookId": book_state.book.book_id,
        "instanceId": instance.instance_id,
        "handoffComplete": instance.handoff_usable(),
    });
    writer.done(response)
}

fn checkpoint_report(
    mut writer: Writer<'_>,
    checkpoint: &Checkpoint,
    book: usize,
    worker: Option<usize>,
) -> Result<Step, LifecycleError> {
    let index = worker.ok_or(WORKER_ONLY)?;
    let state = writer.state.books[book].instances[index].state;
    if state == InstanceState::Retired {
        return Err(RETIRED);
    }
    require_report(&checkpoint.report)?;
    if checkpoint.terminal == Some(true) || state == InstanceState::HandoffOnly {
        // Persist the complete handoff and retire in one commit.
        retire(
            &mut writer,
            book,
            index,
            true,
            "handoff",
            Some(&checkpoint.report),
            None,
            EntryAuthor::Worker,
        );
    } else {
        let sequence = writer.push(
            book,
            Some(index),
            EntryAuthor::Worker,
            "checkpoint",
            json!({"report": checkpoint.report}),
        );
        writer.state.books[book].instances[index].checkpoint_sequence = Some(sequence);
    }
    Ok(lifecycle_result(writer, book, index))
}

fn settle_instance(
    mut writer: Writer<'_>,
    settle: &Settle,
    book: usize,
    owner: bool,
    worker: Option<usize>,
) -> Result<Step, LifecycleError> {
    let instances = &writer.state.books[book].instances;
    let index = match (settle.instance_id.as_deref(), worker) {
        (Some(id), Some(own)) => {
            if instances[own].instance_id != id {
                return Err(NOT_AUTHORIZED);
            }
            own
        }
        (None, Some(own)) => own,
        (Some(id), None) if owner => instances
            .iter()
            .position(|instance| instance.instance_id == id)
            .ok_or(UNKNOWN_INSTANCE)?,
        (None, None) if owner => instances.len() - 1,
        (_, None) => return Err(NOT_AUTHORIZED),
    };
    if let Some(report) = &settle.report {
        if json_size(report) > REPORT_LIMIT_BYTES {
            return Err(refused(
                "report_too_large",
                "the report exceeds the stored size bound; excerpt it and cite the transcript",
            ));
        }
    }
    let author = if worker == Some(index) {
        EntryAuthor::Worker
    } else {
        EntryAuthor::Parent
    };
    let record = &writer.state.books[book].instances[index];
    if record.state != InstanceState::Retired {
        match settle.outcome {
            SettleOutcome::Failed | SettleOutcome::Aborted => {
                retire(
                    &mut writer,
                    book,
                    index,
                    false,
                    settle.outcome.as_str(),
                    settle.report.as_ref(),
                    settle.transcript_path.as_ref(),
                    author,
                );
                writer.state.books[book].instances[index].settled_outcome =
                    Some(settle.outcome.as_str().to_owned());
            }
            SettleOutcome::Completed if record.settled_outcome.is_none() => {
                writer.push(
                    book,
                    Some(index),
                    author,
                    "settlement",
                    json!({
                        "outcome": "completed",
                        "report": settle.report,
                        "transcriptPath": settle.transcript_path,
                    }),
                );
                let record = &mut writer.state.books[book].instances[index];
                record.settled_outcome = Some("completed".into());
                if settle.transcript_path.is_some() {
                    record.transcript_path = settle.transcript_path.clone();
                }
            }
            SettleOutcome::Completed => {}
        }
    }
    Ok(lifecycle_result(writer, book, index))
}

fn accept_evidence(
    mut writer: Writer<'_>,
    actor: &Actor,
    accept: &Accept,
    book: usize,
    owner: bool,
    next_id: &mut dyn FnMut() -> String,
) -> Result<Step, LifecycleError> {
    if !owner {
        return Err(PARENT_ONLY);
    }
    require_report(&accept.body)?;
    let retained_id = next_id();
    let book_state = &writer.state.books[book];
    writer.effects.retained.push(RetainedRecord {
        retained_id: retained_id.clone(),
        swarm_id: book_state.book.swarm_id.clone(),
        book_id: book_state.book.book_id.clone(),
        slot_key: book_state.book.slot_key.clone(),
        agent_name: book_state.book.agent_name.clone(),
        instance_ids: book_state
            .instances
            .iter()
            .map(|instance| instance.instance_id.clone())
            .collect(),
        accepted_by_room: actor.room.clone(),
        accepted_by_spirit: actor.spirit.clone(),
        accepted_by_session: actor.session.clone(),
        body: accept.body.clone(),
        created_ms: writer.now_ms,
    });
    writer.state.books[book].book.accepted_count += 1;
    writer.push(
        book,
        None,
        EntryAuthor::Parent,
        "acceptance",
        json!({"retainedId": retained_id}),
    );
    let book_state = &writer.state.books[book];
    let response = json!({
        "ok": true,
        "retainedId": retained_id,
        "swarmId": book_state.book.swarm_id,
        "bookId": book_state.book.book_id,
    });
    Ok(writer.done(response))
}

fn repair_handoff(
    mut writer: Writer<'_>,
    repair: &Repair,
    book: usize,
    owner: bool,
) -> Result<Step, LifecycleError> {
    if !owner {
        return Err(PARENT_ONLY);
    }
    let instances = &writer.state.books[book].instances;
    let index = instances
        .iter()
        .position(|instance| instance.instance_id == repair.instance_id)
        .ok_or(UNKNOWN_INSTANCE)?;
    if index + 1 != instances.len() {
        return Err(refused(
            "predecessor_superseded",
            "this instance already has a successor",
        ));
    }
    let instance = &instances[index];
    let handoff = match (&instance.handoff, instance.state) {
        (Some(handoff), InstanceState::Retired) => handoff,
        _ => {
            return Err(refused(
                "repair_not_needed",
                "only a retired instance with an incomplete handoff can be repaired",
            ));
        }
    };
    if handoff.usable() {
        return Err(refused(
            "handoff_already_complete",
            "this handoff is already complete",
        ));
    }
    require_report(&repair.report)?;
    let repairs = handoff.sequence;
    let transcript_path = handoff.transcript_path.clone();
    let sequence = writer.push(
        book,
        Some(index),
        EntryAuthor::Parent,
        "repair",
        json!({
            "suppliedBy": "parent",
            "report": repair.report,
            "repairsSequence": repairs,
            "transcriptPath": transcript_path,
        }),
    );
    writer.state.books[book].instances[index]
        .handoff
        .as_mut()
        .expect("repaired handoff exists")
        .repair_sequence = Some(sequence);
    Ok(lifecycle_result(writer, book, index))
}

fn dispose_swarm(
    actor: &Actor,
    dispose: &Dispose,
    aggregate: &mut Option<SwarmState>,
    now_ms: i64,
) -> Result<Step, LifecycleError> {
    const UNKNOWN: LifecycleError = refused("unknown_swarm", "the requested swarm does not exist");
    let state = aggregate.as_mut().ok_or(UNKNOWN)?;
    if !same_room(&state.swarm, actor) {
        return Err(UNKNOWN);
    }
    if !is_owner(&state.swarm, actor) {
        return Err(PARENT_ONLY);
    }
    if state.swarm.disposed_ms.is_some() {
        return Err(DISPOSED);
    }
    if state
        .books
        .iter()
        .flat_map(|book| &book.instances)
        .any(|instance| !instance.inactive())
    {
        return Err(refused(
            "swarm_active",
            "the swarm still has active or unsettled workers",
        ));
    }
    if dispose.discard_reason.is_none()
        && state.books.iter().any(|book| book.book.accepted_count == 0)
    {
        return Err(refused(
            "unpreserved_book",
            "every book needs accepted evidence or an explicit discardReason",
        ));
    }
    let books = state
        .books
        .iter()
        .map(|book| {
            let retained = book.book.accepted_count > 0;
            let mut row = Map::new();
            row.insert("bookId".into(), json!(book.book.book_id));
            row.insert("slotKey".into(), json!(book.book.slot_key));
            row.insert("agentName".into(), json!(book.book.agent_name));
            row.insert("acceptedCount".into(), json!(book.book.accepted_count));
            row.insert("entryCount".into(), json!(book.book.next_sequence - 1));
            row.insert(
                "instanceIds".into(),
                json!(
                    book.instances
                        .iter()
                        .map(|i| &i.instance_id)
                        .collect::<Vec<_>>()
                ),
            );
            row.insert(
                "disposition".into(),
                json!(if retained { "retained" } else { "discarded" }),
            );
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    let disposed_books = books.len();
    let retained_books = state
        .books
        .iter()
        .filter(|book| book.book.accepted_count > 0)
        .count();
    let disposition = DispositionRecord {
        swarm_id: state.swarm.swarm_id.clone(),
        disposed_by_room: actor.room.clone(),
        disposed_by_spirit: actor.spirit.clone(),
        disposed_by_session: actor.session.clone(),
        discard_reason: dispose.discard_reason.clone(),
        books: Value::Array(books),
        disposed_ms: now_ms,
    };
    state.swarm.disposed_ms = Some(now_ms);
    state.books.clear();
    Ok(Step {
        effects: Effects {
            disposition: Some(disposition),
            ..Effects::default()
        },
        applied: Applied::Response(json!({
            "ok": true,
            "swarmId": state.swarm.swarm_id,
            "disposedBooks": disposed_books,
            "retainedBooks": retained_books,
            "backupsErased": false,
            "note": "transient book entries were removed from the live store; accepted evidence and this receipt are retained; existing backups are not erased",
        })),
    })
}
