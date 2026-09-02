mod anamnesis;
pub mod backup;
mod bm25f;
mod canon;
mod cluster;
mod config;
mod docket;
mod entity;
mod giga;
mod giga_worker;
mod hallway;
mod health;
pub mod insula;
pub mod insula_writer;
mod lesson;
pub mod migrations;
mod paper_boat;
mod recall;
mod remember;
mod restart;
pub mod settings;
pub mod state;
mod timeline;

pub use anamnesis::{anamnesis, anamnesis_write};
pub use backup::{backup, restore};
pub use canon::{canon_read, canon_write};
pub use cluster::{
    ClusterGroup, ClusterMembers, ClusterStaleness, cluster_is_stale, cluster_maintenance,
    cluster_staleness, spherical_kmeans,
};
pub use config::{AppError, Config, EmbeddingMode};
pub use docket::{
    AcceptanceSummary, QuestBoardItem, QuestBoardParams, QuestBoardResult, QuestChargebookParams,
    QuestChargebookResult, QuestChargebookRow, QuestChargebookTotals, QuestClaimParams,
    QuestClaimResult, QuestClockDueItem, QuestClockParams, QuestClockResult, QuestEvidenceEvent,
    QuestEvidenceItem, QuestEvidenceParams, QuestEvidenceReceipt, QuestEvidenceResult,
    QuestPostAction, QuestPostParams, QuestPostResult, QuestReportAction, QuestReportParams,
    QuestReportResult, quest_board, quest_chargebook, quest_claim, quest_clock, quest_evidence,
    quest_post, quest_report, require_docket_capability,
};
pub use entity::{EntityMatch, EntityResolveParams, EntityResolveResult, entity_resolve};
pub use giga::{
    giga_candidate_list, giga_candidate_store, giga_conversation_ingest, giga_event_claim,
    giga_event_finish, giga_event_ingest, giga_event_replay, giga_health, giga_promote,
    giga_queue_maintenance, giga_review, giga_tool_promote, giga_tool_review,
};
pub use giga_worker::{GigaWorkerHandle, giga_process, spawn_giga_worker};
pub use hallway::{
    hallway_create, hallway_inbox, hallway_join, hallway_knock, hallway_knock_claim,
    hallway_knock_policy, hallway_knock_settle, hallway_messages, hallway_post, hallway_read,
};
pub use health::{
    SubstrateHealthOptions, SubstrateHealthResult, substrate_health, substrate_health_with_config,
};
pub use insula::{
    INSULA_MAX_BATCH_EVENTS, INSULA_MAX_RETENTION_ROWS, INSULA_MAX_TRACE_ROWS,
    INSULA_MAX_UNVERIFIED_EXIT_ROWS, INSULA_MAX_VITALS_ROWS, INSULA_QUERY_VERSION,
    INSULA_SCHEMA_VERSION, IdempotencyScope, IngestBatch, IngestConflict, IngestConflictKind,
    IngestReceipt, InsulaError, ObservationEvent, ObservationPhase, OutcomeClass,
    RetentionReadResult, RetentionReceipt, RetentionReceiptRow, RetentionStatus, TraceResult,
    TraceRow, TraceScope, TrustedBinding, UnverifiedExitResult, UnverifiedExitRow, VitalsQuery,
    VitalsResult, VitalsRow, derive_idempotency_key_v1, derive_semantic_hash_v1, ingest_batch,
    query_retention, query_trace, query_unverified_exit, query_vitals, run_retention,
    validate_trusted_binding,
};
pub use insula_writer::{
    EmitterSpan, end_span, flush_insula_emitter, init_insula_emitter, record_point, start_span,
    system_binding,
};
pub use lesson::{
    DesignDocument, DesignDocumentFilters, DesignDocumentQueryParams, DesignDocumentQueryResult,
    DesignDocumentTaxonomy, DesignDocumentWriteParams, DesignDocumentWriteReceipt,
    LessonContextFilters, LessonContextMatch, LessonContextParams, LessonContextRecord,
    LessonContextResult, LessonDeleteParams, LessonFamily, LessonFilters, LessonMutationKind,
    LessonMutationReceipt, LessonQueryParams, LessonQueryResult, LessonRecord, LessonTaxonomy,
    LessonTriggerFired, LessonTriggerMatchParams, LessonTriggerMatchResult, LessonTriggerSurface,
    LessonUpdateParams, design_document_query, design_document_write, lesson_context,
    lesson_delete, lesson_query, lesson_trigger_match, lesson_update,
};
pub use paper_boat::{paper_boat_sleep, paper_boat_wake};
pub use recall::{RecallParams, RecallResult, recall, refresh_semantic_vocabulary};
pub use remember::remember;
pub use restart::{
    EXITING_DEADLINE_SECS, RELAUNCH_ATTEMPT_LIMIT, RELAUNCHING_DEADLINE_SECS, REQUESTED_TTL_SECS,
    STORM_MAX_EXITING_PER_WINDOW, STORM_WINDOW_SECS, restart_claim, restart_request,
    restart_status, restart_transition, restart_verify,
};
pub use settings::RoomSettings;
pub use timeline::{
    LessonTimelineItem, LessonTimelineParams, LessonTimelineResult, MemoryReadParams,
    MemoryReadResult, MemoryRecord, MemoryTimelineItem, MemoryTimelineParams, MemoryTimelineResult,
    lesson_timeline, memory_read, memory_timeline,
};
