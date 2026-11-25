//! The Host's one door from a [`ProtocolErrorBody`] to a [`Failure`].
//!
//! AKASHA, the Orihon organ, and the Vault worker journal still answer with a
//! `ProtocolErrorBody`, whose code is its only typed fact. The Host turns that
//! code into a class here and keeps everything else as it came.

use protocol::{Failure, FailureClass, ProtocolErrorBody};

/// Code, message, and details pass through unchanged. The class comes from the
/// code alone; an unknown code is `internal` and keeps its word. The body's
/// `retryable` flag is dropped: the class now decides the retry.
#[track_caller]
pub(crate) fn from_error_body(body: ProtocolErrorBody) -> Failure {
    let ProtocolErrorBody {
        code,
        message,
        details,
        ..
    } = body;
    let mut failure = Failure::new(class_of(&code), code, message);
    failure.details = details;
    failure
}

// ponytail: exact-code table at the Host edge; upgrade when slice D makes akasha declare FailureClass.
// The Orihon organ and the Vault journal codes leave it when slice C gives those modules Failure.
fn class_of(code: &str) -> FailureClass {
    match code {
        "body_too_large"
        | "empty_report"
        | "giga_replay_refused"
        | "hallway_not_found"
        | "invalid_params"
        | "invalid_parent"
        | "kind_not_allowed"
        | "knock_message_mismatch"
        | "knock_not_found"
        | "knock_parent_mismatch"
        | "knock_turns_exceed_policy"
        | "malformed_request"
        | "malformed_uuid"
        | "message_not_found"
        | "missing_parent"
        | "orihon_prefix_invalid"
        | "prepared_batch_too_large"
        | "report_too_large"
        | "thread_not_found"
        | "unknown_attempt"
        | "unknown_book"
        | "unknown_cursor"
        | "unknown_instance"
        | "unknown_intent"
        | "unknown_memory"
        | "unknown_method"
        | "unknown_operation"
        | "unknown_quest"
        | "unknown_reservation"
        | "unknown_swarm"
        | "unpreserved_book"
        | "usage_invalid" => FailureClass::InputRefused,
        "claimant_binding"
        | "docket_capability"
        | "executor_cannot_settle"
        | "exit_not_authorized"
        | "foreign_room"
        | "knock_policy_denied"
        | "not_authorized"
        | "parent_only"
        | "restart_capability"
        | "review_independence"
        | "room_not_allowed"
        | "spirit_mismatch"
        | "verify_not_authorized"
        | "worker_only" => FailureClass::NotPermitted,
        "agent_mismatch"
        | "conflicting_page"
        | "conflicting_record"
        | "exit_not_requested"
        | "giga_closed"
        | "handoff_already_complete"
        | "handoff_incomplete"
        | "handoff_only"
        | "idempotency_reuse"
        | "illegal_transition"
        | "instance_retired"
        | "intent_expired"
        | "intent_pending"
        | "knock_already_requested"
        | "knock_state_conflict"
        | "not_claimable"
        | "not_settleable"
        | "not_verifiable"
        | "operation_already_open"
        | "orihon_boundary_missing"
        | "orihon_boundary_stale"
        | "orihon_generation_changed"
        | "orihon_lineage_pending"
        | "orihon_not_live"
        | "orihon_owner_busy"
        | "orihon_owner_closed"
        | "orihon_participants_full"
        | "orihon_view_not_ready"
        | "predecessor_active"
        | "predecessor_superseded"
        | "presence_not_live"
        | "repair_not_needed"
        | "request_id_conflict"
        | "reservation_claimed"
        | "reservation_key_taken"
        | "restart_storm"
        | "session_already_bound"
        | "slot_already_prepared"
        | "stale_lease"
        | "swarm_active"
        | "swarm_disposed"
        | "verify_expired" => FailureClass::Conflict,
        "knock_exchange_exhausted" => FailureClass::BudgetExhausted,
        // The room lock connection was open and did not answer within 10 s.
        "orihon_owner_lost" => FailureClass::Deadline,
        // One code covers connect, query, and provider failures alike; AKASHA
        // marks them all retryable, and this class keeps that reaction.
        "database" | "embedding" => FailureClass::NoConnection,
        "familiar_lessons_unsupported" | "protocol" | "protocol_mismatch" => {
            FailureClass::InvalidResponse
        }
        "configuration"
        | "familiar_lesson_incomplete"
        | "familiar_lessons_missing"
        | "familiar_lessons_too_large"
        | "familiar_lessons_unavailable"
        | "io"
        | "orihon_folder_unconfigured"
        | "orihon_io" => FailureClass::Configuration,
        // Known codes for stored Orihon state that broke an invariant; the
        // fallback below is for codes this table has never seen.
        "book_full"
        | "conflicting_export"
        | "fold_snapshot_limit"
        | "incomplete_book"
        | "invalid_book"
        | "invalid_fold_children"
        | "invalid_fold_sources"
        | "malformed_book"
        | "orihon_unit_missing"
        | "orihon_units_gap"
        | "orihon_units_partial" => FailureClass::Internal,
        _ => FailureClass::Internal,
    }
}
