//! The sleep write: a spirit casts one boat into the Sea.
//!
//! Sleep has three parts. The paperwork is deterministic and pure:
//! identity, title, thread, metadata — [`plan`]. The row write is the
//! shared `memories` insert with the boat's DO-NOTHING conflict rule —
//! see [`write_boat_tx`], which is still at the substrate and says why.
//! The pointer read is the one seam where the boat shape touches the
//! crane shape — [`ready_pointer`].
//!
//! Cost and state (coding#195): [`plan`] touches nothing. Both other
//! calls run inside a caller-owned transaction and never commit; the
//! caller commits, and only then is the boat durable.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};

use super::error::BoatResult;
use super::identity::{self, DIGEST_LABEL};
use super::{EVENT_KIND, SLEEP_ORIGIN, THREAD_KEY};

/// The title prefix a cast boat is filed under, followed by its date.
const TITLE_PREFIX: &str = "paper boat — ";

/// The outbox aggregate kind a boat row is enqueued under. The pointer
/// event names the memory row, never the body.
const OUTBOX_AGGREGATE_KIND: &str = "memory";

/// The deterministic paperwork of one cast boat: everything the write
/// needs that does not touch the database.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoatPlan {
    /// The content-addressed provenance path; also the idempotency key.
    pub source_path: String,
    /// The boat title, dated in the caller's clock.
    pub title: String,
    /// The primary date the memory row is filed under.
    pub date: NaiveDate,
    /// The single thread every boat is filed under.
    pub threads: Vec<String>,
    /// The row metadata: origin, recording time, identity.
    pub metadata: Value,
}

/// Fold the paperwork for one boat: identity, title, thread, metadata.
/// Absorbs the deterministic half of
/// house-substrate/src/paper_boat.rs:18 `paper_boat_sleep`; the halves
/// that need `Config`, embeddings, the backup hook, or a receipt stay at
/// the substrate door by contract.
pub fn plan(room: &str, body: &str, now: DateTime<Utc>) -> BoatPlan {
    let source_path = identity::source_identity(room, body);
    let digest = identity::digest_of(&source_path).unwrap_or_default();
    let date = now.date_naive();
    BoatPlan {
        title: format!("{TITLE_PREFIX}{date}"),
        date,
        threads: vec![THREAD_KEY.to_owned()],
        metadata: json!({
            "origin": SLEEP_ORIGIN,
            "recorded_at": now.to_rfc3339(),
            "identity": format!("{DIGEST_LABEL}:{digest}"),
        }),
        source_path,
    }
}

/// Insert the boat row with source-path idempotency inside one transaction.
/// Absorbs the boat branch of house-substrate/src/remember.rs:448.
//
// enough: the boat branch stays in house-substrate `write_memory_tx`.
// The branch is only the ON CONFLICT DO NOTHING head of one insert whose
// column list, tail, and inputs are generic memory machinery: the shared
// `memories` column set, `PreparedMemoryWrite` (chunking, embedding
// vectors, thread fan-out), the supersedes update, and the chunk rows.
// Moving the head alone would split one row write across two crates and
// give the `memories` column shape two homes — the duplicate authority
// this extraction exists to remove. The DO-NOTHING conflict path is
// load-bearing and stays exactly where it is proven, untouched.
// The way up: quest A1's memory-kind registry, where "conflict is a
// refusal, not an update" becomes a flag on the kind and the branch
// disappears instead of moving.
pub fn write_boat_tx() {
    todo!("deferred at remember.rs:448; see the enough mark above")
}

/// Read the `boat.ready` outbox pointer the 0016 trigger enqueued,
/// inside the same transaction. This is the one seam between the boat
/// shape and the crane shape.
/// Absorbs house-substrate/src/paper_boat.rs:56.
pub async fn ready_pointer(
    tx: &mut Transaction<'_, Postgres>,
    memory_id: i64,
) -> BoatResult<String> {
    let event_id: String = sqlx::query_scalar(
        "SELECT event_id::text FROM crane_outbox
         WHERE aggregate_kind=$1 AND aggregate_id=$2 AND event_kind=$3",
    )
    .bind(OUTBOX_AGGREGATE_KIND)
    .bind(memory_id)
    .bind(EVENT_KIND)
    .fetch_one(&mut **tx)
    .await?;
    Ok(event_id)
}
