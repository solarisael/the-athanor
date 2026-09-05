//! Insula: the House's interoception. Trusted writers ingest observation
//! events; the queries read vitals, traces, and unverified exits back;
//! retention proves what it deleted.

mod event;
mod ingest;
mod query;
mod retention;

use sha2::{Digest, Sha256};
use thiserror::Error;

pub use event::{
    IdempotencyScope, ObservationEvent, ObservationPhase, OutcomeClass, TrustedBinding,
    derive_idempotency_key_v1, derive_semantic_hash_v1, validate_trusted_binding,
};
pub use ingest::{
    INSULA_MAX_BATCH_EVENTS, IngestBatch, IngestConflict, IngestConflictKind, IngestReceipt,
    ingest_batch,
};
pub use query::{
    INSULA_MAX_SPAN_ROWS, INSULA_MAX_TRACE_ROWS, INSULA_MAX_UNVERIFIED_EXIT_ROWS,
    INSULA_MAX_VITALS_ROWS, SpanRow, SpanWindow, SpansQuery, SpansResult, TraceResult, TraceRow,
    TraceScope, UnverifiedExitResult, UnverifiedExitRow, VitalsQuery, VitalsResult, VitalsRow,
    query_spans, query_trace, query_unverified_exit, query_vitals,
};
pub use retention::{
    INSULA_MAX_RETENTION_ROWS, RetentionReadResult, RetentionReceipt, RetentionReceiptRow,
    RetentionStatus, query_retention, run_retention,
};

pub const INSULA_SCHEMA_VERSION: i16 = 1;
pub const INSULA_QUERY_VERSION: i16 = 1;

#[derive(Debug, Error)]
pub enum InsulaError {
    #[error("invalid Insula field {field}: {code}")]
    Validation {
        field: &'static str,
        code: &'static str,
    },
    #[error("Insula database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Insula persistence invariant failed: {0}")]
    Invariant(&'static str),
}
fn bad(field: &'static str, code: &'static str) -> InsulaError {
    InsulaError::Validation { field, code }
}

async fn lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    h: &str,
    exclusive: bool,
) -> Result<(), InsulaError> {
    let q = if exclusive {
        "SELECT pg_advisory_xact_lock(hashtextextended($1,723684291))"
    } else {
        "SELECT pg_advisory_xact_lock_shared(hashtextextended($1,723684291))"
    };
    sqlx::query(q).bind(h).execute(&mut **tx).await?;
    Ok(())
}

// Length-prefixed name/value pairs under a domain: one hashing recipe for
// idempotency keys, semantic hashes, and retention coverage proofs.
fn hp(h: &mut Sha256, n: &str, v: &str) {
    h.update((n.len() as u64).to_be_bytes());
    h.update(n);
    h.update((v.len() as u64).to_be_bytes());
    h.update(v)
}
fn hs(domain: &str) -> Sha256 {
    let mut h = Sha256::new();
    hp(&mut h, "domain", domain);
    h
}
fn hf(h: Sha256) -> String {
    format!("{:x}", h.finalize())
}
fn ho(h: &mut Sha256, n: &str, v: Option<&str>) {
    hp(h, n, v.unwrap_or("<absent>"))
}
