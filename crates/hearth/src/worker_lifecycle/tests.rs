//! Contract tests over an in-memory store that follows the same lock, replay,
//! apply, persist sequence as the PostgreSQL and Vault stores.

use super::*;
use serde_json::{Value, json};

struct Fixture {
    swarms: Vec<SwarmState>,
    entries: Vec<EntryRecord>,
    retained: Vec<RetainedRecord>,
    requests: Vec<RequestRecord>,
    ids: u64,
    now: i64,
}

fn actor(room: &str, session: &str) -> Actor {
    Actor {
        house_id: "house".into(),
        room: room.into(),
        spirit: "Spirit".into(),
        session: session.into(),
    }
}

fn token(seed: char) -> String {
    std::iter::repeat_n(seed, 40).collect()
}

fn code(result: Result<Value, LifecycleError>) -> &'static str {
    match result {
        Err(LifecycleError::Refused { code, .. }) => code,
        Err(LifecycleError::Invalid(message)) => panic!("expected refusal, got invalid: {message}"),
        Ok(value) => panic!("expected refusal, got {value}"),
    }
}

impl Fixture {
    fn new() -> Self {
        Self {
            swarms: Vec::new(),
            entries: Vec::new(),
            retained: Vec::new(),
            requests: Vec::new(),
            ids: 0,
            now: 1_000,
        }
    }

    fn call(&mut self, actor: &Actor, request_id: &str, params: Value) -> Result<Value, LifecycleError> {
        let Value::Object(mut map) = params else {
            panic!("params object")
        };
        map.insert("houseId".into(), json!(actor.house_id));
        map.insert("room".into(), json!(actor.room));
        map.insert("spirit".into(), json!(actor.spirit));
        map.insert("session".into(), json!(actor.session));
        map.insert("requestId".into(), json!(request_id));
        let call = Call::from_params(Value::Object(map)).map_err(LifecycleError::Invalid)?;
        self.now += 1;
        if call.action.is_write() {
            if let Some(record) = self.requests.iter().find(|record| {
                record.house_id == actor.house_id
                    && record.room == actor.room
                    && record.session == actor.session
                    && record.request_id == request_id
            }) {
                return replay(&call, record);
            }
        }
        let index = match call.target() {
            Target::Caller => {
                let owned = self
                    .swarms
                    .iter()
                    .filter(|swarm| swarm.swarm.owner_session == actor.session)
                    .cloned()
                    .collect::<Vec<_>>();
                let bound = self.swarms.iter().find(|swarm| {
                    swarm.books.iter().any(|book| {
                        book.instances
                            .iter()
                            .any(|instance| instance.bound_session.as_deref() == Some(&actor.session))
                    })
                });
                return Ok(inspect_caller(actor, &owned, bound));
            }
            Target::NewSwarm { swarm_key } => self.swarms.iter().position(|swarm| {
                swarm.swarm.swarm_key == swarm_key
                    && swarm.swarm.owner_session == actor.session
                    && swarm.swarm.room == actor.room
            }),
            Target::Book(book_id) => self
                .swarms
                .iter()
                .position(|swarm| swarm.books.iter().any(|book| book.book.book_id == book_id)),
            Target::Swarm(swarm_id) => self
                .swarms
                .iter()
                .position(|swarm| swarm.swarm.swarm_id == swarm_id),
            Target::TokenHash(hash) => self.swarms.iter().position(|swarm| {
                swarm
                    .books
                    .iter()
                    .any(|book| book.instances.iter().any(|instance| instance.token_hash == hash))
            }),
        };
        let mut aggregate = index.map(|index| self.swarms[index].clone());
        let mut counter = self.ids;
        let step = apply(&call, &mut aggregate, self.now, &mut || {
            counter += 1;
            format!("id-{counter}")
        })?;
        self.ids = counter;
        let state = aggregate.expect("applied aggregate");
        let swarm_id = state.swarm.swarm_id.clone();
        match index {
            Some(index) => self.swarms[index] = state,
            None => self.swarms.push(state),
        }
        if step.effects.disposition.is_some() {
            self.entries.retain(|entry| entry.swarm_id != swarm_id);
        }
        self.entries.extend(step.effects.entries);
        self.retained.extend(step.effects.retained);
        let response = match step.applied {
            Applied::Response(response) => response,
            Applied::Page {
                book_id,
                after,
                limit,
            } => {
                let page = self
                    .entries
                    .iter()
                    .filter(|entry| entry.book_id == book_id && entry.sequence > after)
                    .take(usize::try_from(limit).unwrap())
                    .cloned()
                    .collect::<Vec<_>>();
                read_response(&page, after)
            }
            Applied::Wake { plan, base } => {
                let wanted = plan.sequences();
                let loaded = self
                    .entries
                    .iter()
                    .filter(|entry| entry.book_id == plan.book_id && wanted.contains(&entry.sequence))
                    .cloned()
                    .collect::<Vec<_>>();
                render_open(&plan, base, &loaded)
            }
            Applied::Retained {
                swarm_id,
                after,
                limit,
            } => page_retained(&self.retained, &swarm_id, after.as_deref(), limit)?,
        };
        if call.action.is_write() {
            self.requests.push(RequestRecord {
                house_id: actor.house_id.clone(),
                room: actor.room.clone(),
                session: actor.session.clone(),
                request_id: request_id.into(),
                action: call.action.name().into(),
                fingerprint: call.fingerprint(),
                response: response.clone(),
                swarm_id: Some(swarm_id),
                created_ms: self.now,
            });
        }
        Ok(deliver(&call, response))
    }

    fn prepare(&mut self, parent: &Actor, slot: &str, seed: char) -> Value {
        self.call(
            parent,
            &format!("prepare-{slot}"),
            json!({"action": "prepare", "swarmKey": "call-1", "slotKey": slot, "agentName": "lavra", "task": "build the wall", "token": token(seed)}),
        )
        .unwrap()
    }

    fn open(&mut self, worker: &Actor, seed: char, request: &str) -> Result<Value, LifecycleError> {
        self.call(
            worker,
            request,
            json!({"action": "open", "token": token(seed), "agentId": "0-Lavra", "agentName": "lavra"}),
        )
    }
}

fn id(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap().to_owned()
}

#[test]
fn prepare_echoes_input_token_but_never_stores_it() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let prepared = fixture.prepare(&parent, "0", 'a');
    assert_eq!(prepared["token"], json!(token('a')));
    let stored = serde_json::to_string(&(&fixture.swarms, &fixture.requests, &fixture.entries)).unwrap();
    assert!(!stored.contains(&token('a')));
    // A retry with the same requestId and token replays the same identity.
    let again = fixture.prepare(&parent, "0", 'a');
    assert_eq!(again, prepared);
    // The same requestId with another token is a conflict, not a new book.
    let conflict = fixture.call(
        &parent,
        "prepare-0",
        json!({"action": "prepare", "swarmKey": "call-1", "slotKey": "0", "agentName": "lavra", "task": "build the wall", "token": token('z')}),
    );
    assert_eq!(code(conflict), "request_id_conflict");
    let duplicate = fixture.call(
        &parent,
        "prepare-other",
        json!({"action": "prepare", "swarmKey": "call-1", "slotKey": "0", "agentName": "lavra", "task": "build the wall", "token": token('y')}),
    );
    assert_eq!(code(duplicate), "slot_already_prepared");
}

#[test]
fn token_theft_and_foreign_rooms_are_refused_and_same_session_resumes() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    fixture.prepare(&parent, "0", 'a');
    let worker = actor("kodo", "child-1");
    let opened = fixture.open(&worker, 'a', "open-1").unwrap();
    assert_eq!(opened["wake"]["kind"], "opening");
    let thief = actor("kodo", "child-2");
    assert_eq!(code(fixture.open(&thief, 'a', "steal")), "reservation_claimed");
    let foreign = actor("other-room", "child-1");
    assert_eq!(code(fixture.open(&foreign, 'a', "steal")), "unknown_reservation");

    let book = id(&opened, "bookId");
    fixture
        .call(&worker, "cp", json!({"action": "checkpoint", "bookId": book, "report": "halfway: laid the base"}))
        .unwrap();
    let resumed = fixture.open(&worker, 'a', "open-2").unwrap();
    assert_eq!(resumed["wake"]["kind"], "resume");
    let text = resumed["wake"]["text"].as_str().unwrap();
    assert!(text.contains("halfway: laid the base"));
    // Replay of the original open returns the original wake, not a new one.
    assert_eq!(fixture.open(&worker, 'a', "open-1").unwrap(), opened);
}

#[test]
fn siblings_and_foreign_parents_cannot_reach_another_book() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let first = fixture.prepare(&parent, "0", 'a');
    let second = fixture.prepare(&parent, "1", 'b');
    let sibling = actor("kodo", "child-b");
    fixture.open(&sibling, 'b', "open").unwrap();
    let first_book = id(&first, "bookId");
    let read = fixture.call(&sibling, "r", json!({"action": "read", "bookId": first_book}));
    assert_eq!(code(read), "not_authorized");
    let append = fixture.call(&sibling, "a", json!({"action": "append", "bookId": first_book, "kind": "note", "body": "x"}));
    assert_eq!(code(append), "not_authorized");
    let stranger = actor("kodo", "another-parent");
    let accept = fixture.call(&stranger, "acc", json!({"action": "accept", "bookId": first_book, "body": "mine"}));
    assert_eq!(code(accept), "parent_only");
    let foreign = actor("elsewhere", "parent");
    let read = fixture.call(&foreign, "r", json!({"action": "read", "bookId": first_book}));
    assert_eq!(code(read), "unknown_book");
    // A bound worker's inspect sees only its own book.
    let view = fixture.call(&sibling, "i", json!({"action": "inspect"})).unwrap();
    let books = view["books"].as_array().unwrap();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0]["bookId"], second["bookId"]);
    assert_eq!(books[0]["instances"][0]["workerSession"], "child-b");
}

#[test]
fn thresholds_warn_once_latch_and_cap_final_requests() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let prepared = fixture.prepare(&parent, "0", 'a');
    let worker = actor("kodo", "child");
    fixture.open(&worker, 'a', "open").unwrap();
    let book = id(&prepared, "bookId");
    let observe = |fixture: &mut Fixture, request: &str, tokens: i64| {
        fixture
            .call(&worker, request, json!({"action": "observe", "bookId": book, "tokens": tokens, "contextWindow": 1000, "requestStarted": true}))
            .unwrap()
    };
    let below = observe(&mut fixture, "o1", 399);
    assert_eq!((below["state"].as_str(), below["warn"].as_bool()), (Some("working"), Some(false)));
    assert_eq!(below["remainingRequests"], 2);
    let warned = observe(&mut fixture, "o2", 400);
    assert_eq!((warned["state"].as_str(), warned["warn"].as_bool()), (Some("wrapping"), Some(true)));
    let again = observe(&mut fixture, "o3", 450);
    assert_eq!(again["warn"], false);
    assert_eq!(again["handoffRequests"], 0);
    let latched = observe(&mut fixture, "o4", 600);
    assert_eq!(latched["state"], "handoff_only");
    assert_eq!((latched["handoffRequests"].as_i64(), latched["remainingRequests"].as_i64()), (Some(1), Some(1)));
    // Compaction lowers usage; the latch holds.
    let compacted = observe(&mut fixture, "o5", 100);
    assert_eq!(compacted["state"], "handoff_only");
    assert_eq!((compacted["handoffRequests"].as_i64(), compacted["remainingRequests"].as_i64()), (Some(2), Some(0)));
    assert_eq!(compacted["retireRequired"], false);
    let exhausted = observe(&mut fixture, "o6", 100);
    assert_eq!(exhausted["retireRequired"], true);
    assert_eq!((exhausted["handoffRequests"].as_i64(), exhausted["remainingRequests"].as_i64()), (Some(2), Some(0)));
    // A retried observe replays instead of consuming another request.
    assert_eq!(observe(&mut fixture, "o6", 100), exhausted);
    let invalid = fixture.call(&worker, "bad", json!({"action": "observe", "bookId": book, "tokens": 10, "contextWindow": 0}));
    assert_eq!(code(invalid), "usage_invalid");
    // Handoff-only blocks starting new tool operations.
    let tool = fixture.call(&worker, "t", json!({"action": "append", "bookId": book, "kind": "tool-start", "operationId": "call-9", "body": {"tool": "edit"}}));
    assert_eq!(code(tool), "handoff_only");
}

#[test]
fn handoff_retires_and_retired_worker_cannot_resurrect() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let prepared = fixture.prepare(&parent, "0", 'a');
    let worker = actor("kodo", "child");
    fixture.open(&worker, 'a', "open").unwrap();
    let book = id(&prepared, "bookId");
    fixture
        .call(&worker, "o", json!({"action": "observe", "bookId": book, "tokens": 700, "contextWindow": 1000}))
        .unwrap();
    let empty = fixture.call(&worker, "h0", json!({"action": "checkpoint", "bookId": book, "report": ""}));
    assert_eq!(code(empty), "empty_report");
    let handoff = fixture
        .call(&worker, "h", json!({"action": "checkpoint", "bookId": book, "report": {"done": "base", "next": "walls"}}))
        .unwrap();
    assert_eq!((handoff["state"].as_str(), handoff["handoffComplete"].as_bool()), (Some("retired"), Some(true)));
    for (request, params) in [
        ("a", json!({"action": "append", "bookId": book, "kind": "note", "body": "late"})),
        ("o2", json!({"action": "observe", "bookId": book, "tokens": 1, "contextWindow": 10})),
        ("c", json!({"action": "checkpoint", "bookId": book, "report": "more"})),
        ("r", json!({"action": "read", "bookId": book})),
    ] {
        assert_eq!(code(fixture.call(&worker, request, params)), "instance_retired");
    }
    assert_eq!(code(fixture.open(&worker, 'a', "reopen")), "instance_retired");
    // Settling a retired instance is an idempotent no-op that keeps the handoff.
    let settled = fixture
        .call(&parent, "s", json!({"action": "settle", "bookId": book, "outcome": "failed"}))
        .unwrap();
    assert_eq!(settled["handoffComplete"], true);
}

#[test]
fn incomplete_handoff_blocks_successor_until_repaired_and_pending_effects_survive() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let prepared = fixture.prepare(&parent, "0", 'a');
    let worker = actor("kodo", "child");
    fixture.open(&worker, 'a', "open").unwrap();
    let book = id(&prepared, "bookId");
    let first = id(&prepared, "instanceId");
    fixture
        .call(&worker, "ts", json!({"action": "append", "bookId": book, "kind": "tool-start", "operationId": "call-7", "body": {"tool": "bash", "command": "migrate"}}))
        .unwrap();
    let foreign_end = fixture.call(&parent, "te", json!({"action": "append", "bookId": book, "kind": "tool-end", "operationId": "call-7", "body": {}}));
    assert_eq!(code(foreign_end), "kind_not_allowed");
    let settled = fixture
        .call(&parent, "s", json!({"action": "settle", "bookId": book, "outcome": "aborted", "transcriptPath": "C:/t/child.jsonl"}))
        .unwrap();
    assert_eq!((settled["state"].as_str(), settled["handoffComplete"].as_bool()), (Some("retired"), Some(false)));

    let successor = |fixture: &mut Fixture, request: &str, seed: char, slot: &str| {
        fixture.call(
            &parent,
            request,
            json!({"action": "prepare", "swarmKey": "call-2", "slotKey": slot, "agentName": "lavra", "task": "continue", "token": token(seed), "predecessor": {"bookId": book, "instanceId": first}}),
        )
    };
    assert_eq!(code(successor(&mut fixture, "succ-1", 'b', "0")), "handoff_incomplete");
    let stranger = actor("kodo", "other-parent");
    let theft = fixture.call(&stranger, "rep", json!({"action": "repair", "bookId": book, "instanceId": first, "report": "fake"}));
    assert_eq!(code(theft), "parent_only");
    fixture
        .call(&parent, "rep", json!({"action": "repair", "bookId": book, "instanceId": first, "report": "migration may have run; verify schema first"}))
        .unwrap();
    let next = successor(&mut fixture, "succ-2", 'b', "0").unwrap();
    // The old predecessor is now superseded: a second successor is stale.
    assert_eq!(code(successor(&mut fixture, "succ-3", 'c', "1")), "predecessor_superseded");

    let heir = actor("kodo", "child-2");
    let opened = fixture.open(&heir, 'b', "open-heir").unwrap();
    assert_eq!(opened["wake"]["kind"], "successor");
    assert_eq!(opened["bookId"], json!(book));
    assert_eq!(opened["instanceId"], next["instanceId"]);
    assert_eq!(opened["pendingOperations"][0]["operationId"], "call-7");
    let text = opened["wake"]["text"].as_str().unwrap();
    assert!(text.contains("verify schema first"));
    assert!(text.contains("call-7"));
    // The successor reads the shared book; the predecessor's entries are there.
    let page = fixture
        .call(&heir, "read", json!({"action": "read", "bookId": book, "limit": 50}))
        .unwrap();
    assert!(page["entries"].as_array().unwrap().iter().any(|entry| entry["kind"] == "tool-start"));
    // The successor cannot end its predecessor's operation as its own.
    let end = fixture.call(&heir, "end", json!({"action": "append", "bookId": book, "kind": "tool-end", "operationId": "call-7", "body": {}}));
    assert_eq!(code(end), "unknown_operation");
}

#[test]
fn completed_settlement_is_idle_and_dispose_requires_preservation() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let first = fixture.prepare(&parent, "0", 'a');
    let second = fixture.prepare(&parent, "1", 'b');
    let worker = actor("kodo", "child");
    fixture.open(&worker, 'a', "open").unwrap();
    let book = id(&first, "bookId");
    let swarm = id(&first, "swarmId");
    fixture
        .call(&worker, "cp", json!({"action": "checkpoint", "bookId": book, "report": "finished"}))
        .unwrap();
    let dispose = |fixture: &mut Fixture, request: &str, reason: Option<&str>| {
        fixture.call(&parent, request, json!({"action": "dispose", "swarmId": swarm, "discardReason": reason}))
    };
    assert_eq!(code(dispose(&mut fixture, "d1", Some("done"))), "swarm_active");
    let idle = fixture
        .call(&parent, "s1", json!({"action": "settle", "bookId": book, "outcome": "completed"}))
        .unwrap();
    assert_eq!((idle["state"].as_str(), idle["handoffComplete"].as_bool()), (Some("working"), Some(false)));
    fixture
        .call(&parent, "s2", json!({"action": "settle", "bookId": id(&second, "bookId"), "outcome": "failed"}))
        .unwrap();
    // Reopening clears the idle mark: the worker is active again.
    fixture.open(&worker, 'a', "reopen").unwrap();
    assert_eq!(code(dispose(&mut fixture, "d2", Some("done"))), "swarm_active");
    fixture
        .call(&worker, "s3", json!({"action": "settle", "bookId": book, "outcome": "completed"}))
        .unwrap();
    let worker_dispose = fixture.call(&worker, "d", json!({"action": "dispose", "swarmId": swarm, "discardReason": "x"}));
    assert_eq!(code(worker_dispose), "parent_only");
    assert_eq!(code(dispose(&mut fixture, "d3", None)), "unpreserved_book");
    fixture
        .call(&parent, "acc", json!({"action": "accept", "bookId": book, "body": {"report": "finished", "evidence": "tests passed"}}))
        .unwrap();
    let receipt = dispose(&mut fixture, "d4", Some("second slot failed and was not useful")).unwrap();
    assert_eq!((receipt["disposedBooks"].as_i64(), receipt["retainedBooks"].as_i64()), (Some(2), Some(1)));
    assert_eq!(receipt["backupsErased"], false);
    assert!(fixture.entries.iter().all(|entry| entry.swarm_id != swarm));
    assert_eq!(fixture.retained.len(), 1);
    assert_eq!(fixture.retained[0].accepted_by_session, "parent");
    let read = fixture.call(&parent, "r", json!({"action": "read", "bookId": book}));
    assert_eq!(code(read), "unknown_book");
    assert_eq!(dispose(&mut fixture, "d4", Some("second slot failed and was not useful")).unwrap(), receipt);
}

#[test]
fn wake_text_is_bounded_and_marks_clipping() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let prepared = fixture.prepare(&parent, "0", 'a');
    let worker = actor("kodo", "child");
    fixture.open(&worker, 'a', "open").unwrap();
    let book = id(&prepared, "bookId");
    let long = "x".repeat(40_000);
    fixture
        .call(&worker, "h", json!({"action": "checkpoint", "bookId": book, "report": long, "terminal": true}))
        .unwrap();
    let next = fixture
        .call(&parent, "succ", json!({"action": "prepare", "swarmKey": "call-2", "slotKey": "0", "agentName": "lavra", "task": "continue", "token": token('b'), "predecessor": {"bookId": book, "instanceId": id(&prepared, "instanceId")}}))
        .unwrap();
    assert_eq!(next["bookId"], json!(book));
    let opened = fixture.open(&actor("kodo", "child-2"), 'b', "open-2").unwrap();
    let text = opened["wake"]["text"].as_str().unwrap();
    assert_eq!(text.chars().count(), WAKE_LIMIT_CHARS);
    assert!(text.ends_with("for the rest]"));
}

#[test]
fn unknown_fields_and_short_tokens_are_invalid() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let extra = fixture.call(
        &parent,
        "p",
        json!({"action": "prepare", "swarmKey": "k", "slotKey": "0", "agentName": "a", "task": "t", "token": token('a'), "bogus": true}),
    );
    assert!(matches!(extra, Err(LifecycleError::Invalid(_))));
    let short = fixture.call(
        &parent,
        "p",
        json!({"action": "prepare", "swarmKey": "k", "slotKey": "0", "agentName": "a", "task": "t", "token": "short"}),
    );
    assert!(matches!(short, Err(LifecycleError::Invalid(_))));
}

#[test]
fn retained_pages_survive_disposal_and_refuse_non_owners() {
    let mut fixture = Fixture::new();
    let parent = actor("kodo", "parent");
    let prepared = fixture.prepare(&parent, "0", 'a');
    let book = id(&prepared, "bookId");
    let swarm = id(&prepared, "swarmId");
    let worker = actor("kodo", "child");
    fixture.open(&worker, 'a', "open").unwrap();
    fixture
        .call(&worker, "done", json!({"action": "checkpoint", "bookId": book, "report": "finished", "terminal": true}))
        .unwrap();
    for (request, body) in [("acc-1", "first evidence"), ("acc-2", "second evidence"), ("acc-3", "third evidence")] {
        fixture
            .call(&parent, request, json!({"action": "accept", "bookId": book, "body": body}))
            .unwrap();
    }
    fixture
        .call(&parent, "dispose", json!({"action": "dispose", "swarmId": swarm}))
        .unwrap();

    let first = fixture
        .call(&parent, "r1", json!({"action": "retained", "swarmId": swarm, "limit": 2}))
        .unwrap();
    let page = first["retained"].as_array().unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0]["body"], "first evidence");
    assert_eq!(page[0]["acceptedBySession"], "parent");
    assert_eq!(page[0]["bookId"], json!(book));
    let cursor = first["nextCursor"].as_str().unwrap().to_owned();
    assert_eq!(cursor, page[1]["retainedId"].as_str().unwrap());
    let rest = fixture
        .call(&parent, "r2", json!({"action": "retained", "swarmId": swarm, "after": cursor}))
        .unwrap();
    assert_eq!(rest["retained"].as_array().unwrap().len(), 1);
    assert_eq!(rest["retained"][0]["body"], "third evidence");
    assert_eq!(rest["nextCursor"], Value::Null);

    let child = fixture.call(&worker, "r3", json!({"action": "retained", "swarmId": swarm}));
    assert_eq!(code(child), "parent_only");
    let foreign = fixture.call(&actor("elsewhere", "parent"), "r4", json!({"action": "retained", "swarmId": swarm}));
    assert_eq!(code(foreign), "unknown_swarm");
    let bogus = fixture.call(&parent, "r5", json!({"action": "retained", "swarmId": swarm, "after": "not-a-retained-id"}));
    assert_eq!(code(bogus), "unknown_cursor");

    // A cursor from another swarm of the same parent is refused, not followed.
    let other = fixture
        .call(&parent, "prep-other", json!({"action": "prepare", "swarmKey": "call-9", "slotKey": "0", "agentName": "lavra", "task": "other", "token": token('q')}))
        .unwrap();
    let foreign_cursor = fixture.call(
        &parent,
        "r6",
        json!({"action": "retained", "swarmId": id(&other, "swarmId"), "after": page[0]["retainedId"]}),
    );
    assert_eq!(code(foreign_cursor), "unknown_cursor");
}
