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

pub use anamnesis::{
    AnamnesisParams, AnamnesisReceipt, AnamnesisResult, AnamnesisSeed, AnamnesisWrite, anamnesis,
    anamnesis_write,
};
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
pub use remember::{RememberReceipt, RememberRequest, ThreadContinuation, remember};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recall::{bounded_excerpt, candidate_terms, query_terms, term_evidence};
    use crate::remember::{chunk_body, derive_dates, normalize_threads, token_estimate};
    use chrono::NaiveDate;
    #[test]
    fn rejects_bad_room() {
        let r = RememberRequest {
            room: "House".into(),
            kind: "memory".into(),
            title: "x".into(),
            body: "y".into(),
            lesson: None,
            source_path: None,
            source_memory_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn source_is_db_only() {
        let r = RememberRequest {
            room: "room".into(),
            kind: "memory".into(),
            title: "x".into(),
            body: "y".into(),
            lesson: None,
            source_path: None,
            source_memory_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        };
        assert!(r.source_path().starts_with("db-only/"));
    }
    #[test]
    fn lesson_validation_enforces_project_and_memory_field_boundaries() {
        let mut project = RememberRequest {
            room: "room".into(),
            kind: "project-lesson".into(),
            title: "t".into(),
            body: "unicode\n多行".into(),
            lesson: None,
            source_path: None,
            source_memory_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: Some("proof".into()),
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec!["a".into()],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        };
        assert!(project.validate().is_err());
        project.project = Some("app".into());
        assert!(project.validate().is_ok());
        project.kind = "memory".into();
        assert!(project.validate().is_err());

        project.kind = "writing-lesson".into();
        project.project = None;
        project.proof_pattern = None;
        project.register = vec![" product-work ".into(), "product-work".into()];
        assert!(project.validate().is_ok());
    }
    #[test]
    fn design_lesson_validation_accepts_contract_fields_and_refuses_project_scope() {
        let mut design = RememberRequest {
            room: "room".into(),
            kind: "design-lesson".into(),
            title: "t".into(),
            body: "unicode\n多行".into(),
            lesson: None,
            source_path: None,
            source_memory_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        };
        design.voice = Some("house-design".into());
        design.shape = Some("component-contract".into());
        design.proof_pattern = Some("Check keyboard navigation.".into());
        design.trigger_context = Some("Before changing a component.".into());
        design.example_text = Some("Use the existing token.".into());
        assert!(design.validate().is_ok());

        design.scope = Some("project".into());
        assert!(design.validate().is_err());
    }

    #[test]
    fn lesson_receipt_serializes_typed_identity() {
        let value = serde_json::to_value(RememberReceipt {
            memory_id: 0,
            lesson_id: 7,
            kind: "writing-lesson".into(),
            room: "room".into(),
            source_path: "db-only/x".into(),
            durable: true,
            authority: "postgres",
            warnings: vec![],
        })
        .unwrap();
        assert_eq!(value["lesson_id"], 7);
        assert_eq!(value["kind"], "writing-lesson");
        assert!(value.get("memory_id").is_none());
    }
    #[test]
    fn design_lesson_receipt_serializes_typed_identity() {
        let value = serde_json::to_value(RememberReceipt {
            memory_id: 0,
            lesson_id: 8,
            kind: "design-lesson".into(),
            room: "room".into(),
            source_path: "db-only/x".into(),
            durable: true,
            authority: "postgres",
            warnings: vec![],
        })
        .unwrap();
        assert_eq!(value["lesson_id"], 8);
        assert_eq!(value["kind"], "design-lesson");
        assert!(value.get("memory_id").is_none());
    }
    #[test]
    fn recall_defaults_and_validation() {
        let p: RecallParams =
            serde_json::from_value(serde_json::json!({"room":"room","query":"alpha"})).unwrap();
        assert_eq!(p.semantic_top_k, 8);
        assert_eq!(p.content_top_k, 8);
        assert!(p.validate().is_ok());
    }
    #[test]
    fn anamnesis_accepts_shared_house_but_preserves_slug_rules() {
        let house: AnamnesisParams =
            serde_json::from_value(serde_json::json!({"room":"house","mode":"wake"})).unwrap();
        assert_eq!(house.validate().unwrap().0, "wake");
        let ordinary = AnamnesisParams {
            room: "Bad Room".into(),
            mode: "wake".into(),
            query: String::new(),
            limit: None,
        };
        assert!(ordinary.validate().is_err());
    }
    #[test]
    fn anamnesis_result_serializes_exact_read_envelope() {
        let value = serde_json::to_value(AnamnesisResult {
            ok: true,
            mode: "consult".into(),
            room: "house".into(),
            query: "pattern".into(),
            found: false,
            entries: vec![],
            warnings: vec!["excluded cycle 4: blank verify_note".into()],
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "ok": true, "mode":"consult", "room":"house", "query":"pattern",
                "found":false, "entries":[], "warnings":["excluded cycle 4: blank verify_note"]
            })
        );
    }
    #[test]
    fn recall_rejects_unknown_empty_and_bounds() {
        assert!(
            serde_json::from_value::<RecallParams>(
                serde_json::json!({"room":"room","query":"x","extra":1})
            )
            .is_err()
        );
        let mut p = RecallParams {
            room: "room".into(),
            query: " ".into(),
            semantic_top_k: 8,
            semantic_min_similarity: 0.5,
            content_top_k: 8,
            content_min_similarity: 0.3,
            temporal_decay: false,
        };
        assert!(p.validate().is_err());
        p.query = "x".into();
        p.semantic_min_similarity = f64::NAN;
        assert!(p.validate().is_err());
    }
    #[test]
    fn lexical_evidence_uses_wire_term_names_and_is_deterministic() {
        let terms = query_terms("Alpha 2026-07-22 alpha");
        assert_eq!(
            terms,
            vec![
                "07".to_string(),
                "2026".to_string(),
                "2026-07-22".to_string(),
                "22".to_string(),
                "alpha".to_string()
            ]
        );
        let compound = query_terms("the pais/mais thingie");
        for expected in ["pais/mais", "pais", "mais", "the", "thingie"] {
            assert!(compound.iter().any(|t| t == expected), "missing {expected}");
        }
        let (matched, missing) = term_evidence(&terms, &["An alpha memory"]);
        assert_eq!(matched, vec!["alpha".to_string()]);
        assert_eq!(
            missing,
            vec![
                "07".to_string(),
                "2026".to_string(),
                "2026-07-22".to_string(),
                "22".to_string()
            ]
        );
        let candidate = serde_json::json!({"matched_terms": matched, "missing_terms": missing, "body_excerpt": "An alpha memory"});
        assert!(candidate.get("matched_terms").is_some());
        assert!(candidate.get("missing_terms").is_some());
        assert!(candidate.get("body_excerpt").is_some());
        assert!(candidate.get("terms").is_none());
        assert!(candidate.get("excerpt").is_none());
    }
    #[test]
    fn unicode_chunks_use_utf8_bytes() {
        let c = chunk_body("éé", &RoomSettings::default());
        assert_eq!((c[0].1, c[0].2), (0, 4));
        assert_eq!(&"éé"[c[0].1..c[0].2], "éé");
        assert!(token_estimate("é") > 0);
    }
    #[test]
    fn oversized_chunks_preserve_separator_and_span() {
        let first = "a".repeat(2200);
        let body = format!("{first}\n\né{}", "b".repeat(2500));
        let chunks = chunk_body(&body, &RoomSettings::default());
        let (text, start, end, _) = &chunks[1];
        assert_eq!(text, &body[*start..*end]);
        assert!(text.contains("\n\né"));
    }
    #[test]
    fn derive_dates_uses_injected_primary_date() {
        let d = NaiveDate::from_ymd_opt(2026, 7, 22).unwrap();
        assert!(derive_dates("db-only/room/no-date", d).contains(&d));
    }
    #[test]
    fn threads_normalize() {
        assert_eq!(
            normalize_threads(&[" a ".into(), " ".into(), "a".into()]),
            vec!["a"]
        );
    }
    #[test]
    fn bounded_excerpt_is_character_safe_and_limited() {
        let body = "é".repeat(1300);
        let excerpt = bounded_excerpt(&body);
        assert!(excerpt.chars().count() <= 1201);
        assert!(excerpt.ends_with('…'));
    }
    #[test]
    fn candidate_term_coverage_is_exact() {
        let (matched, missing, coverage) =
            candidate_terms(&["alpha".into(), "beta".into()], &["alpha body"]);
        assert_eq!(matched, vec!["alpha"]);
        assert_eq!(missing, vec!["beta"]);
        assert_eq!(coverage, 0.5);
    }
}
