mod binding;
mod error;
mod event;
mod hash;
mod idempotency;
mod ingest;
mod lock;
mod retention;
mod trace;
mod version;
mod vitals;

pub use binding::{TrustedBinding, validate_trusted_binding};
pub use error::InsulaError;
pub use event::{IdempotencyScope, ObservationEvent, ObservationPhase, OutcomeClass};
pub use idempotency::{derive_idempotency_key_v1, derive_semantic_hash_v1};
pub use ingest::{
    INSULA_MAX_BATCH_EVENTS, IngestBatch, IngestConflict, IngestConflictKind, IngestReceipt,
    ingest_batch,
};
pub use retention::{
    INSULA_MAX_RETENTION_ROWS, RetentionReadResult, RetentionReceipt, RetentionReceiptRow,
    RetentionStatus, query_retention, run_retention,
};
pub use trace::{INSULA_MAX_TRACE_ROWS, TraceResult, TraceRow, TraceScope, query_trace};
pub use version::{INSULA_QUERY_VERSION, INSULA_SCHEMA_VERSION};
pub use vitals::{INSULA_MAX_VITALS_ROWS, VitalsQuery, VitalsResult, VitalsRow, query_vitals};
