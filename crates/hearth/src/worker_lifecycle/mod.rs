//! Worker lifecycle: task-lineage continuity for bounded workers.
//!
//! One parent task invocation owns a swarm. Each task slot owns one append-only
//! book, and successive worker instances share that book only through an
//! explicit predecessor link. Instances move monotonically through
//! `working -> wrapping -> handoff_only -> retired`.
//!
//! This module is the single owner of the ownership and transition rules. The
//! AKASHA PostgreSQL store and the Host Vault runtime journal both lock one
//! swarm aggregate, call [`apply`], and persist the aggregate plus the effects
//! it returns, so the two stores cannot drift on rules. Nothing here does I/O.

mod engine;
mod model;
mod request;
mod wake;

#[cfg(test)]
mod tests;

pub use engine::{
    Applied, Effects, LifecycleError, Step, apply, changed_books, deliver, inspect_caller,
    page_retained, read_response, refused, render_open, replay, retained_response,
    unknown_cursor,
};
pub use model::{
    BookRecord, BookState, DispositionRecord, EntryAuthor, EntryRecord, HandoffRecord,
    InstanceRecord, InstanceState, OperationRecord, RequestRecord, RetainedRecord, SwarmRecord,
    SwarmState,
};
pub use request::{
    Accept, Action, Actor, Append, Call, Checkpoint, Dispose, Inspect, Observe, Open,
    PredecessorRef, Prepare, Read, Repair, Retained, Settle, SettleOutcome, Target, token_hash,
};
pub use wake::{WakeKind, WakePlan};

/// Context usage at which a worker is warned once to start wrapping up.
pub const WARN_PERCENT: i128 = 40;
/// Context usage at which a worker latches handoff-only mode.
pub const HANDOFF_PERCENT: i128 = 60;
/// Model requests a handoff-only worker may start before it must retire.
pub const HANDOFF_REQUEST_LIMIT: i64 = 2;
/// Upper bound on rendered wake text, in characters.
pub const WAKE_LIMIT_CHARS: usize = 12_000;
pub const READ_LIMIT_DEFAULT: i64 = 20;
pub const READ_LIMIT_MAX: i64 = 50;
/// Serialized size cap for one appended entry body. The adapter truncates
/// tool and message content into bounded excerpts before it reaches here.
pub const BODY_LIMIT_BYTES: usize = 64 * 1024;
/// Serialized size cap for checkpoints, handoffs, repairs, and acceptances.
pub const REPORT_LIMIT_BYTES: usize = 256 * 1024;
/// The parent's task text is already in the worker prompt; the book keeps an
/// excerpt for inspection only.
pub const TASK_EXCERPT_CHARS: usize = 2_000;
pub const INSPECT_SWARM_LIMIT: i64 = 20;
pub const INSPECT_BOOK_LIMIT: usize = 50;
pub const INSPECT_INSTANCE_LIMIT: usize = 20;
pub const PENDING_LIMIT: usize = 50;
