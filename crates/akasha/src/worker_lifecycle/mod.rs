//! PostgreSQL store for `worker_lifecycle`. The rules live in
//! `hearth::worker_lifecycle`; this module only locks one swarm aggregate,
//! replays committed requests, applies the domain step, and persists the
//! changed metadata rows plus append-only entries in the same transaction.
//! A named familiar's tagged coding lessons are read in that transaction on
//! every wake, so a refused wake commits nothing.

use crate::config::AppError;
use chrono::{DateTime, Utc};
use hearth::worker_lifecycle::{
    Applied, BookRecord, BookState, Call, EntryAuthor, EntryRecord, Familiar, HandoffRecord,
    INSPECT_SWARM_LIMIT, InstanceRecord, InstanceState, LifecycleError, OperationRecord,
    RequestRecord, RetainedRecord, RoleTag, SwarmRecord, SwarmState, TaggedLesson, Target, apply,
    changed_books, deliver, inspect_caller, read_response, render_open, replay, retained_response,
    unknown_cursor,
};
use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

fn domain_error(error: LifecycleError) -> AppError {
    match error {
        LifecycleError::Invalid(message) => AppError::Invalid(message),
        LifecycleError::Refused { code, message } => AppError::Refusal { code, message },
    }
}

fn corrupt(what: &'static str) -> AppError {
    AppError::Config(format!("worker_lifecycle row holds an invalid {what}"))
}

fn instant(ms: i64) -> Result<DateTime<Utc>, AppError> {
    DateTime::from_timestamp_millis(ms).ok_or_else(|| corrupt("timestamp"))
}

fn millis(row: &PgRow, column: &str) -> Result<i64, AppError> {
    Ok(row.try_get::<DateTime<Utc>, _>(column)?.timestamp_millis())
}

/// Execute one `worker_lifecycle` call against PostgreSQL.
pub async fn worker_lifecycle(pool: &PgPool, call: Call) -> Result<Value, AppError> {
    let mut tx = pool.begin().await?;
    let target = call.target();
    if target == Target::Caller {
        let response = inspect_all(&mut tx, &call).await?;
        tx.commit().await?;
        return Ok(response);
    }
    let swarm_id = lock_target(&mut tx, &call, &target).await?;
    if call.action.is_write() {
        if let Some(record) = request_record(&mut tx, &call).await? {
            let response = replay(&call, &record).map_err(domain_error)?;
            tx.commit().await?;
            return Ok(response);
        }
    }
    let before = match &swarm_id {
        Some(swarm_id) => load_swarm(&mut tx, swarm_id).await?,
        None => None,
    };
    let mut aggregate = before.clone();
    let now_ms = Utc::now().timestamp_millis();
    let step = apply(&call, &mut aggregate, now_ms, &mut || {
        uuid::Uuid::new_v4().to_string()
    })
    .map_err(domain_error)?;
    let after = aggregate.expect("an applied lifecycle step keeps its aggregate");
    let disposed = step.effects.disposition.is_some();
    if call.action.is_write() {
        persist(&mut tx, before.as_ref(), &after, &step.effects).await?;
    }
    let response = match step.applied {
        Applied::Response(response) => response,
        Applied::Page {
            book_id,
            after: from,
            limit,
        } => {
            let entries = page(&mut tx, &book_id, from, limit).await?;
            read_response(&entries, from)
        }
        Applied::Wake { plan, base } => {
            let entries = entries_at(&mut tx, &plan.book_id, &plan.sequences()).await?;
            let lessons = match plan.familiar {
                Some(familiar) => {
                    Some(familiar_lessons(&mut tx, &call.actor.room, familiar).await?)
                }
                None => None,
            };
            render_open(&plan, base, &entries, lessons).map_err(domain_error)?
        }
        Applied::Retained {
            swarm_id,
            after: cursor,
            limit,
        } => {
            let records = retained_page(&mut tx, &swarm_id, cursor.as_deref(), limit).await?;
            retained_response(records, limit)
        }
    };
    if call.action.is_write() {
        // A disposal receipt outlives the swarm's transient request records.
        let swarm = (!disposed).then(|| after.swarm.swarm_id.clone());
        insert_request(&mut tx, &call, &response, swarm, now_ms).await?;
    }
    tx.commit().await?;
    Ok(deliver(&call, response))
}

/// Resolve the target to a swarm id and take its row lock. Every write on a
/// swarm serializes here, so concurrent retries see each other's commits.
async fn lock_target(
    tx: &mut Tx<'_>,
    call: &Call,
    target: &Target,
) -> Result<Option<String>, AppError> {
    let actor = &call.actor;
    let swarm_id: Option<String> = match target {
        Target::NewSwarm { swarm_key } => {
            // Two first prepares of one native call must not both create it.
            sqlx::query(
                "SELECT pg_advisory_xact_lock(hashtextextended(jsonb_build_array('worker_lifecycle',$1::text,$2::text,$3::text,$4::text)::text,0))",
            )
            .bind(&actor.house_id)
            .bind(&actor.room)
            .bind(&actor.session)
            .bind(swarm_key)
            .execute(&mut **tx)
            .await?;
            sqlx::query_scalar(
                "SELECT swarm_id FROM worker_lifecycle.swarms WHERE house_id=$1 AND room=$2 AND owner_session=$3 AND swarm_key=$4",
            )
            .bind(&actor.house_id)
            .bind(&actor.room)
            .bind(&actor.session)
            .bind(swarm_key)
            .fetch_optional(&mut **tx)
            .await?
        }
        Target::Book(book_id) => {
            sqlx::query_scalar("SELECT swarm_id FROM worker_lifecycle.books WHERE book_id=$1")
                .bind(book_id)
                .fetch_optional(&mut **tx)
                .await?
        }
        Target::Swarm(swarm_id) => Some(swarm_id.clone()),
        Target::TokenHash(hash) => {
            sqlx::query_scalar(
                "SELECT b.swarm_id FROM worker_lifecycle.instances i JOIN worker_lifecycle.books b ON b.book_id=i.book_id WHERE i.token_hash=$1",
            )
            .bind(hash)
            .fetch_optional(&mut **tx)
            .await?
        }
        Target::Caller => None,
    };
    let Some(swarm_id) = swarm_id else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar(
        "SELECT swarm_id FROM worker_lifecycle.swarms WHERE swarm_id=$1 FOR UPDATE",
    )
    .bind(&swarm_id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn request_record(tx: &mut Tx<'_>, call: &Call) -> Result<Option<RequestRecord>, AppError> {
    let actor = &call.actor;
    let row = sqlx::query(
        "SELECT action,fingerprint,response,swarm_id,created_at FROM worker_lifecycle.requests WHERE house_id=$1 AND room=$2 AND session=$3 AND request_id=$4",
    )
    .bind(&actor.house_id)
    .bind(&actor.room)
    .bind(&actor.session)
    .bind(&call.request_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| -> Result<RequestRecord, AppError> {
        Ok(RequestRecord {
            house_id: actor.house_id.clone(),
            room: actor.room.clone(),
            session: actor.session.clone(),
            request_id: call.request_id.clone(),
            action: row.try_get("action")?,
            fingerprint: row.try_get("fingerprint")?,
            response: row.try_get("response")?,
            swarm_id: row.try_get("swarm_id")?,
            created_ms: millis(&row, "created_at")?,
        })
    })
    .transpose()
}

async fn insert_request(
    tx: &mut Tx<'_>,
    call: &Call,
    response: &Value,
    swarm_id: Option<String>,
    now_ms: i64,
) -> Result<(), AppError> {
    let actor = &call.actor;
    sqlx::query(
        "INSERT INTO worker_lifecycle.requests (house_id,room,session,request_id,action,fingerprint,response,swarm_id,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(&actor.house_id)
    .bind(&actor.room)
    .bind(&actor.session)
    .bind(&call.request_id)
    .bind(call.action.name())
    .bind(call.fingerprint())
    .bind(response)
    .bind(swarm_id)
    .bind(instant(now_ms)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn swarm_record(row: &PgRow) -> Result<SwarmRecord, AppError> {
    Ok(SwarmRecord {
        swarm_id: row.try_get("swarm_id")?,
        house_id: row.try_get("house_id")?,
        room: row.try_get("room")?,
        owner_spirit: row.try_get("owner_spirit")?,
        owner_session: row.try_get("owner_session")?,
        swarm_key: row.try_get("swarm_key")?,
        created_ms: millis(row, "created_at")?,
        disposed_ms: row
            .try_get::<Option<DateTime<Utc>>, _>("disposed_at")?
            .map(|at| at.timestamp_millis()),
    })
}

fn instance_record(row: &PgRow) -> Result<InstanceRecord, AppError> {
    let state: String = row.try_get("state")?;
    let handoff: Option<Value> = row.try_get("handoff")?;
    Ok(InstanceRecord {
        instance_id: row.try_get("instance_id")?,
        book_id: row.try_get("book_id")?,
        ordinal: row.try_get("ordinal")?,
        predecessor_instance_id: row.try_get("predecessor_instance_id")?,
        reservation_key: row.try_get("reservation_key")?,
        token_hash: row.try_get("token_hash")?,
        agent_name: row.try_get("agent_name")?,
        bound_session: row.try_get("bound_session")?,
        agent_id: row.try_get("agent_id")?,
        transcript_path: row.try_get("transcript_path")?,
        state: InstanceState::parse(&state).ok_or_else(|| corrupt("instance state"))?,
        warned: row.try_get("warned")?,
        handoff_requests: row.try_get("handoff_requests")?,
        checkpoint_sequence: row.try_get("checkpoint_sequence")?,
        handoff: handoff
            .map(serde_json::from_value::<HandoffRecord>)
            .transpose()
            .map_err(|_| corrupt("handoff record"))?,
        settled_outcome: row.try_get("settled_outcome")?,
        own_entries: row.try_get("own_entries")?,
        last_own_sequence: row.try_get("last_own_sequence")?,
        opened_count: row.try_get("opened_count")?,
        created_ms: millis(row, "created_at")?,
        updated_ms: millis(row, "updated_at")?,
    })
}

/// Load one swarm's metadata aggregate. Entry bodies are never loaded here.
async fn load_swarm(tx: &mut Tx<'_>, swarm_id: &str) -> Result<Option<SwarmState>, AppError> {
    let Some(row) = sqlx::query("SELECT * FROM worker_lifecycle.swarms WHERE swarm_id=$1")
        .bind(swarm_id)
        .fetch_optional(&mut **tx)
        .await?
    else {
        return Ok(None);
    };
    let swarm = swarm_record(&row)?;
    let book_rows = sqlx::query(
        "SELECT * FROM worker_lifecycle.books WHERE swarm_id=$1 ORDER BY created_at, book_id",
    )
    .bind(swarm_id)
    .fetch_all(&mut **tx)
    .await?;
    let instance_rows = sqlx::query(
        "SELECT i.* FROM worker_lifecycle.instances i JOIN worker_lifecycle.books b ON b.book_id=i.book_id WHERE b.swarm_id=$1 ORDER BY i.book_id, i.ordinal",
    )
    .bind(swarm_id)
    .fetch_all(&mut **tx)
    .await?;
    let operation_rows = sqlx::query(
        "SELECT o.* FROM worker_lifecycle.operations o JOIN worker_lifecycle.books b ON b.book_id=o.book_id WHERE b.swarm_id=$1 ORDER BY o.book_id, o.started_sequence",
    )
    .bind(swarm_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut books = book_rows
        .iter()
        .map(|row| -> Result<BookState, AppError> {
            Ok(BookState {
                book: BookRecord {
                    book_id: row.try_get("book_id")?,
                    swarm_id: row.try_get("swarm_id")?,
                    slot_key: row.try_get("slot_key")?,
                    agent_name: row.try_get("agent_name")?,
                    task_excerpt: row.try_get("task_excerpt")?,
                    next_sequence: row.try_get("next_sequence")?,
                    accepted_count: row.try_get("accepted_count")?,
                    created_ms: millis(row, "created_at")?,
                },
                instances: Vec::new(),
                operations: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    for row in &instance_rows {
        let instance = instance_record(row)?;
        let book = books
            .iter_mut()
            .find(|book| book.book.book_id == instance.book_id)
            .ok_or_else(|| corrupt("instance book"))?;
        book.instances.push(instance);
    }
    for row in &operation_rows {
        let book_id: String = row.try_get("book_id")?;
        let book = books
            .iter_mut()
            .find(|book| book.book.book_id == book_id)
            .ok_or_else(|| corrupt("operation book"))?;
        book.operations.push(OperationRecord {
            operation_id: row.try_get("operation_id")?,
            instance_id: row.try_get("instance_id")?,
            started_sequence: row.try_get("started_sequence")?,
        });
    }
    Ok(Some(SwarmState { swarm, books }))
}

async fn persist(
    tx: &mut Tx<'_>,
    before: Option<&SwarmState>,
    after: &SwarmState,
    effects: &hearth::worker_lifecycle::Effects,
) -> Result<(), AppError> {
    let swarm = &after.swarm;
    match before {
        None => {
            sqlx::query(
                "INSERT INTO worker_lifecycle.swarms (swarm_id,house_id,room,owner_spirit,owner_session,swarm_key,created_at,disposed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&swarm.swarm_id)
            .bind(&swarm.house_id)
            .bind(&swarm.room)
            .bind(&swarm.owner_spirit)
            .bind(&swarm.owner_session)
            .bind(&swarm.swarm_key)
            .bind(instant(swarm.created_ms)?)
            .bind(swarm.disposed_ms.map(instant).transpose()?)
            .execute(&mut **tx)
            .await?;
        }
        Some(before) if before.swarm != *swarm => {
            sqlx::query("UPDATE worker_lifecycle.swarms SET disposed_at=$2 WHERE swarm_id=$1")
                .bind(&swarm.swarm_id)
                .bind(swarm.disposed_ms.map(instant).transpose()?)
                .execute(&mut **tx)
                .await?;
        }
        Some(_) => {}
    }
    if let Some(disposition) = &effects.disposition {
        sqlx::query(
            "INSERT INTO worker_lifecycle.dispositions (swarm_id,disposed_by_room,disposed_by_spirit,disposed_by_session,discard_reason,books,disposed_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(&disposition.swarm_id)
        .bind(&disposition.disposed_by_room)
        .bind(&disposition.disposed_by_spirit)
        .bind(&disposition.disposed_by_session)
        .bind(&disposition.discard_reason)
        .bind(&disposition.books)
        .bind(instant(disposition.disposed_ms)?)
        .execute(&mut **tx)
        .await?;
        // disposed_at is already set in this transaction, which is the only
        // state in which the entry guard trigger permits deletion.
        for statement in [
            "DELETE FROM worker_lifecycle.operations WHERE book_id IN (SELECT book_id FROM worker_lifecycle.books WHERE swarm_id=$1)",
            "DELETE FROM worker_lifecycle.entries WHERE swarm_id=$1",
            "DELETE FROM worker_lifecycle.instances WHERE book_id IN (SELECT book_id FROM worker_lifecycle.books WHERE swarm_id=$1)",
            "DELETE FROM worker_lifecycle.books WHERE swarm_id=$1",
            "DELETE FROM worker_lifecycle.requests WHERE swarm_id=$1",
        ] {
            sqlx::query(statement)
                .bind(&swarm.swarm_id)
                .execute(&mut **tx)
                .await?;
        }
        return Ok(());
    }
    for book in changed_books(before, after) {
        upsert_book(tx, book).await?;
    }
    for entry in &effects.entries {
        insert_entry(tx, entry).await?;
    }
    for retained in &effects.retained {
        sqlx::query(
            "INSERT INTO worker_lifecycle.retained (retained_id,swarm_id,book_id,slot_key,agent_name,instance_ids,accepted_by_room,accepted_by_spirit,accepted_by_session,body,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(&retained.retained_id)
        .bind(&retained.swarm_id)
        .bind(&retained.book_id)
        .bind(&retained.slot_key)
        .bind(&retained.agent_name)
        .bind(serde_json::to_value(&retained.instance_ids).expect("ids serialize"))
        .bind(&retained.accepted_by_room)
        .bind(&retained.accepted_by_spirit)
        .bind(&retained.accepted_by_session)
        .bind(&retained.body)
        .bind(instant(retained.created_ms)?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn upsert_book(tx: &mut Tx<'_>, book: &BookState) -> Result<(), AppError> {
    let record = &book.book;
    sqlx::query(
        "INSERT INTO worker_lifecycle.books (book_id,swarm_id,slot_key,agent_name,task_excerpt,next_sequence,accepted_count,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (book_id) DO UPDATE SET next_sequence=EXCLUDED.next_sequence,accepted_count=EXCLUDED.accepted_count",
    )
    .bind(&record.book_id)
    .bind(&record.swarm_id)
    .bind(&record.slot_key)
    .bind(&record.agent_name)
    .bind(&record.task_excerpt)
    .bind(record.next_sequence)
    .bind(record.accepted_count)
    .bind(instant(record.created_ms)?)
    .execute(&mut **tx)
    .await?;
    for instance in &book.instances {
        let handoff = instance
            .handoff
            .as_ref()
            .map(|handoff| serde_json::to_value(handoff).expect("handoff serializes"));
        sqlx::query(
            "INSERT INTO worker_lifecycle.instances (instance_id,book_id,ordinal,predecessor_instance_id,reservation_key,token_hash,agent_name,bound_session,agent_id,transcript_path,state,warned,handoff_requests,checkpoint_sequence,handoff,settled_outcome,own_entries,last_own_sequence,opened_count,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21) ON CONFLICT (instance_id) DO UPDATE SET bound_session=EXCLUDED.bound_session,agent_id=EXCLUDED.agent_id,transcript_path=EXCLUDED.transcript_path,state=EXCLUDED.state,warned=EXCLUDED.warned,handoff_requests=EXCLUDED.handoff_requests,checkpoint_sequence=EXCLUDED.checkpoint_sequence,handoff=EXCLUDED.handoff,settled_outcome=EXCLUDED.settled_outcome,own_entries=EXCLUDED.own_entries,last_own_sequence=EXCLUDED.last_own_sequence,opened_count=EXCLUDED.opened_count,updated_at=EXCLUDED.updated_at",
        )
        .bind(&instance.instance_id)
        .bind(&instance.book_id)
        .bind(instance.ordinal)
        .bind(&instance.predecessor_instance_id)
        .bind(&instance.reservation_key)
        .bind(&instance.token_hash)
        .bind(&instance.agent_name)
        .bind(&instance.bound_session)
        .bind(&instance.agent_id)
        .bind(&instance.transcript_path)
        .bind(instance.state.as_str())
        .bind(instance.warned)
        .bind(instance.handoff_requests)
        .bind(instance.checkpoint_sequence)
        .bind(handoff)
        .bind(&instance.settled_outcome)
        .bind(instance.own_entries)
        .bind(instance.last_own_sequence)
        .bind(instance.opened_count)
        .bind(instant(instance.created_ms)?)
        .bind(instant(instance.updated_ms)?)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query("DELETE FROM worker_lifecycle.operations WHERE book_id=$1")
        .bind(&record.book_id)
        .execute(&mut **tx)
        .await?;
    for operation in &book.operations {
        sqlx::query(
            "INSERT INTO worker_lifecycle.operations (book_id,operation_id,instance_id,started_sequence) VALUES ($1,$2,$3,$4)",
        )
        .bind(&record.book_id)
        .bind(&operation.operation_id)
        .bind(&operation.instance_id)
        .bind(operation.started_sequence)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_entry(tx: &mut Tx<'_>, entry: &EntryRecord) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO worker_lifecycle.entries (book_id,sequence,swarm_id,instance_id,author,kind,body,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&entry.book_id)
    .bind(entry.sequence)
    .bind(&entry.swarm_id)
    .bind(&entry.instance_id)
    .bind(entry.author.as_str())
    .bind(&entry.kind)
    .bind(&entry.body)
    .bind(instant(entry.created_ms)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn entry_record(row: &PgRow) -> Result<EntryRecord, AppError> {
    let author: String = row.try_get("author")?;
    Ok(EntryRecord {
        swarm_id: row.try_get("swarm_id")?,
        book_id: row.try_get("book_id")?,
        sequence: row.try_get("sequence")?,
        instance_id: row.try_get("instance_id")?,
        author: EntryAuthor::parse(&author).ok_or_else(|| corrupt("entry author"))?,
        kind: row.try_get("kind")?,
        body: row.try_get("body")?,
        created_ms: millis(row, "created_at")?,
    })
}

async fn page(
    tx: &mut Tx<'_>,
    book_id: &str,
    after: i64,
    limit: i64,
) -> Result<Vec<EntryRecord>, AppError> {
    sqlx::query(
        "SELECT * FROM worker_lifecycle.entries WHERE book_id=$1 AND sequence>$2 ORDER BY sequence LIMIT $3",
    )
    .bind(book_id)
    .bind(after)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?
    .iter()
    .map(entry_record)
    .collect()
}

async fn entries_at(
    tx: &mut Tx<'_>,
    book_id: &str,
    sequences: &[i64],
) -> Result<Vec<EntryRecord>, AppError> {
    if sequences.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query(
        "SELECT * FROM worker_lifecycle.entries WHERE book_id=$1 AND sequence=ANY($2) ORDER BY sequence",
    )
    .bind(book_id)
    .bind(sequences)
    .fetch_all(&mut **tx)
    .await?
    .iter()
    .map(entry_record)
    .collect()
}

/// Coding lessons visible in `room` that carry any of the familiar's exact
/// tags: one row per lesson, bodies whole.
async fn familiar_lessons(
    tx: &mut Tx<'_>,
    room: &str,
    familiar: Familiar,
) -> Result<Vec<TaggedLesson>, AppError> {
    let tags = familiar.tags().map(RoleTag::as_str).collect::<Vec<_>>();
    sqlx::query(
        "SELECT id,title,lesson,tags FROM lessons WHERE lesson_key='coding' AND (scope='house' OR scope=$1) AND tags && $2 ORDER BY id",
    )
    .bind(room)
    .bind(tags)
    .fetch_all(&mut **tx)
    .await?
    .iter()
    .map(|row| -> Result<TaggedLesson, AppError> {
        Ok(TaggedLesson {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            lesson: row.try_get("lesson")?,
            tags: row.try_get("tags")?,
        })
    })
    .collect()
}

/// Up to `limit + 1` accepted records of one swarm in (created_at,
/// retained_id) order, after a cursor that must belong to the same swarm.
async fn retained_page(
    tx: &mut Tx<'_>,
    swarm_id: &str,
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<RetainedRecord>, AppError> {
    let cursor: Option<(DateTime<Utc>, String)> = match after {
        None => None,
        Some(retained_id) => {
            let created: DateTime<Utc> = sqlx::query_scalar(
                "SELECT created_at FROM worker_lifecycle.retained WHERE retained_id=$1 AND swarm_id=$2",
            )
            .bind(retained_id)
            .bind(swarm_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| domain_error(unknown_cursor()))?;
            Some((created, retained_id.to_owned()))
        }
    };
    let rows = match &cursor {
        None => {
            sqlx::query(
                "SELECT * FROM worker_lifecycle.retained WHERE swarm_id=$1 ORDER BY created_at, retained_id LIMIT $2",
            )
            .bind(swarm_id)
            .bind(limit + 1)
            .fetch_all(&mut **tx)
            .await?
        }
        Some((created, retained_id)) => {
            sqlx::query(
                "SELECT * FROM worker_lifecycle.retained WHERE swarm_id=$1 AND (created_at, retained_id) > ($2, $3) ORDER BY created_at, retained_id LIMIT $4",
            )
            .bind(swarm_id)
            .bind(created)
            .bind(retained_id)
            .bind(limit + 1)
            .fetch_all(&mut **tx)
            .await?
        }
    };
    rows.iter()
        .map(|row| -> Result<RetainedRecord, AppError> {
            let instance_ids: Value = row.try_get("instance_ids")?;
            Ok(RetainedRecord {
                retained_id: row.try_get("retained_id")?,
                swarm_id: row.try_get("swarm_id")?,
                book_id: row.try_get("book_id")?,
                slot_key: row.try_get("slot_key")?,
                agent_name: row.try_get("agent_name")?,
                instance_ids: serde_json::from_value(instance_ids)
                    .map_err(|_| corrupt("retained instance ids"))?,
                accepted_by_room: row.try_get("accepted_by_room")?,
                accepted_by_spirit: row.try_get("accepted_by_spirit")?,
                accepted_by_session: row.try_get("accepted_by_session")?,
                body: row.try_get("body")?,
                created_ms: millis(row, "created_at")?,
            })
        })
        .collect()
}

/// inspect without bookId: the caller's most recent live owned swarms and the
/// swarm holding the caller's own bound instance.
async fn inspect_all(tx: &mut Tx<'_>, call: &Call) -> Result<Value, AppError> {
    let actor = &call.actor;
    let owned_ids: Vec<String> = sqlx::query_scalar(
        "SELECT swarm_id FROM worker_lifecycle.swarms WHERE house_id=$1 AND room=$2 AND owner_session=$3 AND disposed_at IS NULL ORDER BY created_at DESC LIMIT $4",
    )
    .bind(&actor.house_id)
    .bind(&actor.room)
    .bind(&actor.session)
    .bind(INSPECT_SWARM_LIMIT)
    .fetch_all(&mut **tx)
    .await?;
    let mut owned = Vec::with_capacity(owned_ids.len());
    for swarm_id in &owned_ids {
        if let Some(swarm) = load_swarm(tx, swarm_id).await? {
            owned.push(swarm);
        }
    }
    let bound_id: Option<String> = sqlx::query_scalar(
        "SELECT s.swarm_id FROM worker_lifecycle.instances i JOIN worker_lifecycle.books b ON b.book_id=i.book_id JOIN worker_lifecycle.swarms s ON s.swarm_id=b.swarm_id WHERE i.bound_session=$1 AND s.house_id=$2 AND s.room=$3 ORDER BY i.updated_at DESC LIMIT 1",
    )
    .bind(&actor.session)
    .bind(&actor.house_id)
    .bind(&actor.room)
    .fetch_optional(&mut **tx)
    .await?;
    let bound = match bound_id {
        Some(swarm_id) => load_swarm(tx, &swarm_id).await?,
        None => None,
    };
    Ok(inspect_caller(actor, &owned, bound.as_ref()))
}
