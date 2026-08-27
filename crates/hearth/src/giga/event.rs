use crate::error::DomainError;
use crate::room::RoomKey;

use super::lifecycle::{GigaEventType, GigaLifecycle};
use super::queue::GigaQueueState;
use super::source::{
    GigaSourceRef, GigaSourceType, GigaVisibility, giga_nonempty, giga_rfc3339, giga_strings,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEvent {
    event_schema_version: u8,
    event_id: String,
    event_type: GigaEventType,
    room: RoomKey,
    session_id: String,
    project_keys: Vec<String>,
    source_refs: Vec<GigaSourceRef>,
    lifecycle: GigaLifecycle,
    created_at: String,
}
impl GigaEvent {
    pub fn new(
        event_id: String,
        event_type: GigaEventType,
        room: RoomKey,
        session_id: String,
        project_keys: Vec<String>,
        source_refs: Vec<GigaSourceRef>,
        lifecycle: GigaLifecycle,
        created_at: String,
    ) -> Result<Self, DomainError> {
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "must not be empty".into(),
            });
        }
        if lifecycle.event_type() != event_type {
            return Err(DomainError::InvalidGiga {
                field: "lifecycle".into(),
                message: "does not match event_type".into(),
            });
        }
        let project_keys = giga_strings("project_keys", project_keys)?;
        let has = |kind| {
            source_refs
                .iter()
                .any(|source| source.source_type() == kind)
        };
        let valid_sources = match event_type {
            GigaEventType::ConversationWindow => has(GigaSourceType::Turn),
            GigaEventType::TaskStarted => has(GigaSourceType::TaskContract),
            GigaEventType::TaskCompleted => {
                has(GigaSourceType::TaskContract) && has(GigaSourceType::LifecycleEvent)
            }
            GigaEventType::SubagentDispatched => has(GigaSourceType::TaskContract),
            GigaEventType::SubagentCompleted => {
                has(GigaSourceType::TaskContract) && has(GigaSourceType::LifecycleEvent)
            }
            GigaEventType::TodoTransition => has(GigaSourceType::LifecycleEvent),
            GigaEventType::ToolOutcome => has(GigaSourceType::ToolResultSummary),
            GigaEventType::ManualReprocess => source_refs
                .iter()
                .any(|source| source.range() == lifecycle.source_range()),
        };
        if !valid_sources {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "missing source type required by event_type".into(),
            });
        }
        if event_type == GigaEventType::TaskStarted {
            let project = lifecycle.field("project_key").expect("validated lifecycle");
            if project_keys.len() != 1 || project_keys[0] != project {
                return Err(DomainError::InvalidGiga {
                    field: "project_keys".into(),
                    message: "task_started requires its one lifecycle project_key".into(),
                });
            }
        }
        let mut source_project: Option<&str> = None;
        for source in &source_refs {
            if source.scope().visibility() == GigaVisibility::Private
                && source.scope().room() != Some(&room)
            {
                return Err(DomainError::GigaScopeViolation);
            }
            if let Some(project) = source.scope().project() {
                if source_project.is_some_and(|known| known != project) {
                    return Err(DomainError::GigaScopeViolation);
                }
                source_project = Some(project);
            }
        }
        if let Some(project) = source_project {
            if project_keys.len() != 1 || project_keys[0] != project {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        Ok(Self {
            event_schema_version: 1,
            event_id: giga_nonempty("event_id", event_id)?,
            event_type,
            room,
            session_id: giga_nonempty("session_id", session_id)?,
            project_keys,
            source_refs,
            lifecycle,
            created_at: giga_rfc3339("created_at", created_at)?,
        })
    }
    pub const fn event_schema_version(&self) -> u8 {
        self.event_schema_version
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub const fn event_type(&self) -> GigaEventType {
        self.event_type
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn project_keys(&self) -> &[String] {
        &self.project_keys
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn lifecycle(&self) -> &GigaLifecycle {
        &self.lifecycle
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
}

pub const GIGA_MAX_PROCESS_SOURCES: usize = 8;
pub const GIGA_MAX_PROCESS_SOURCE_BYTES: usize = 8_000;
pub const GIGA_MAX_PROCESS_WINDOW_BYTES: usize = 24_000;

pub const GIGA_MAX_LEASE_SECONDS: u32 = 3_600;
pub const GIGA_MAX_EVENT_ATTEMPTS: u32 = 5;
pub const GIGA_MAX_CANDIDATES_PER_EVENT: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventClaimRequest {
    room: RoomKey,
    worker_id: String,
    lease_seconds: u32,
}

impl GigaEventClaimRequest {
    pub fn new(room: RoomKey, worker_id: String, lease_seconds: u32) -> Result<Self, DomainError> {
        if lease_seconds == 0 || lease_seconds > GIGA_MAX_LEASE_SECONDS {
            return Err(DomainError::InvalidGiga {
                field: "lease_seconds".into(),
                message: format!("must be between 1 and {GIGA_MAX_LEASE_SECONDS}"),
            });
        }
        Ok(Self {
            room,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            lease_seconds,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub const fn lease_seconds(&self) -> u32 {
        self.lease_seconds
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaEventFinishOutcome {
    Succeeded,
    Retry,
    Failed,
}

impl GigaEventFinishOutcome {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "retry" => Ok(Self::Retry),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::UnknownGigaValue {
                field: "outcome".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Retry => "retry",
            Self::Failed => "failed",
        }
    }
}

fn giga_error_class(value: String) -> Result<String, DomainError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(DomainError::InvalidGiga {
            field: "error_class".into(),
            message: "must be a redacted ASCII class token of at most 128 bytes".into(),
        });
    }
    Ok(value)
}

fn giga_finish_candidate_count(
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
) -> Result<(), DomainError> {
    if candidate_count > GIGA_MAX_CANDIDATES_PER_EVENT {
        return Err(DomainError::InvalidGiga {
            field: "candidate_count".into(),
            message: format!("must be at most {GIGA_MAX_CANDIDATES_PER_EVENT}"),
        });
    }
    if outcome != GigaEventFinishOutcome::Succeeded && candidate_count != 0 {
        return Err(DomainError::InvalidGiga {
            field: "candidate_count".into(),
            message: "retry and failed outcomes cannot report a stored candidate".into(),
        });
    }
    Ok(())
}

fn giga_finish_fields(
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<String>,
    retry_after_seconds: Option<u32>,
) -> Result<(Option<String>, Option<u32>), DomainError> {
    giga_finish_candidate_count(outcome, candidate_count)?;
    if outcome != GigaEventFinishOutcome::Retry && retry_after_seconds.is_some() {
        return Err(DomainError::InvalidGiga {
            field: "retry_after_seconds".into(),
            message: "is valid only for a retry outcome".into(),
        });
    }
    if outcome == GigaEventFinishOutcome::Retry
        && retry_after_seconds.is_some_and(|seconds| seconds > GIGA_MAX_LEASE_SECONDS)
    {
        return Err(DomainError::InvalidGiga {
            field: "retry_after_seconds".into(),
            message: format!("must be at most {GIGA_MAX_LEASE_SECONDS}"),
        });
    }
    match outcome {
        GigaEventFinishOutcome::Succeeded if error_class.is_some() => {
            return Err(DomainError::InvalidGiga {
                field: "error_class".into(),
                message: "is not valid for a succeeded outcome".into(),
            });
        }
        GigaEventFinishOutcome::Retry | GigaEventFinishOutcome::Failed if error_class.is_none() => {
            return Err(DomainError::InvalidGiga {
                field: "error_class".into(),
                message: "is required for retry and failed outcomes".into(),
            });
        }
        _ => {}
    }
    Ok((
        error_class.map(giga_error_class).transpose()?,
        retry_after_seconds,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventFinishRequest {
    room: RoomKey,
    event_id: String,
    worker_id: String,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<String>,
    retry_after_seconds: Option<u32>,
}

impl GigaEventFinishRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        room: RoomKey,
        event_id: String,
        worker_id: String,
        outcome: GigaEventFinishOutcome,
        candidate_count: u32,
        error_class: Option<String>,
        retry_after_seconds: Option<u32>,
    ) -> Result<Self, DomainError> {
        let (error_class, retry_after_seconds) =
            giga_finish_fields(outcome, candidate_count, error_class, retry_after_seconds)?;
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            outcome,
            candidate_count,
            error_class,
            retry_after_seconds,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub const fn outcome(&self) -> GigaEventFinishOutcome {
        self.outcome
    }
    pub const fn candidate_count(&self) -> u32 {
        self.candidate_count
    }
    pub fn error_class(&self) -> Option<&str> {
        self.error_class.as_deref()
    }
    pub const fn retry_after_seconds(&self) -> Option<u32> {
        self.retry_after_seconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventReplayRequest {
    room: RoomKey,
    event_id: String,
    operator_identity: String,
    authorization_basis: String,
}

impl GigaEventReplayRequest {
    pub fn new(
        room: RoomKey,
        event_id: String,
        operator_identity: String,
        authorization_basis: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            authorization_basis: giga_nonempty("authorization_basis", authorization_basis)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.operator_identity
    }
    pub fn authorization_basis(&self) -> &str {
        &self.authorization_basis
    }
}

fn giga_attempt_count(value: u32) -> Result<u32, DomainError> {
    if value == 0 || value > GIGA_MAX_EVENT_ATTEMPTS {
        return Err(DomainError::InvalidGiga {
            field: "attempt_count".into(),
            message: format!("must be between 1 and {GIGA_MAX_EVENT_ATTEMPTS}"),
        });
    }
    Ok(value)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventClaimReceipt {
    room: RoomKey,
    worker_id: String,
    claimed_at: String,
    event: Option<GigaEvent>,
    lease_expires_at: Option<String>,
    attempt_count: Option<u32>,
}

impl GigaEventClaimReceipt {
    pub fn new(
        room: RoomKey,
        worker_id: String,
        claimed_at: String,
        event: Option<GigaEvent>,
        lease_expires_at: Option<String>,
        attempt_count: Option<u32>,
    ) -> Result<Self, DomainError> {
        if event.is_some() != lease_expires_at.is_some()
            || event.is_some() != attempt_count.is_some()
        {
            return Err(DomainError::InvalidGiga {
                field: "claim_receipt".into(),
                message: "event, lease_expires_at, and attempt_count must all be present or absent"
                    .into(),
            });
        }
        if event.as_ref().is_some_and(|event| event.room() != &room) {
            return Err(DomainError::GigaScopeViolation);
        }
        Ok(Self {
            room,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            claimed_at: giga_rfc3339("claimed_at", claimed_at)?,
            event,
            lease_expires_at: lease_expires_at
                .map(|value| giga_rfc3339("lease_expires_at", value))
                .transpose()?,
            attempt_count: attempt_count.map(giga_attempt_count).transpose()?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub fn claimed_at(&self) -> &str {
        &self.claimed_at
    }
    pub fn event(&self) -> Option<&GigaEvent> {
        self.event.as_ref()
    }
    pub fn lease_expires_at(&self) -> Option<&str> {
        self.lease_expires_at.as_deref()
    }
    pub const fn attempt_count(&self) -> Option<u32> {
        self.attempt_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventFinishReceipt {
    room: RoomKey,
    event_id: String,
    worker_id: String,
    outcome: GigaEventFinishOutcome,
    queue_state: GigaQueueState,
    attempt_count: u32,
    candidate_count: u32,
    available_at: Option<String>,
    finished_at: String,
}

impl GigaEventFinishReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        room: RoomKey,
        event_id: String,
        worker_id: String,
        outcome: GigaEventFinishOutcome,
        queue_state: GigaQueueState,
        attempt_count: u32,
        candidate_count: u32,
        available_at: Option<String>,
        finished_at: String,
    ) -> Result<Self, DomainError> {
        let expected_state = match outcome {
            GigaEventFinishOutcome::Succeeded => GigaQueueState::Succeeded,
            GigaEventFinishOutcome::Retry => GigaQueueState::Pending,
            GigaEventFinishOutcome::Failed => GigaQueueState::Failed,
        };
        if queue_state != expected_state {
            return Err(DomainError::InvalidGiga {
                field: "queue_state".into(),
                message: "does not match finish outcome".into(),
            });
        }
        let attempt_count = giga_attempt_count(attempt_count)?;
        if outcome == GigaEventFinishOutcome::Retry && attempt_count == GIGA_MAX_EVENT_ATTEMPTS {
            return Err(DomainError::InvalidGiga {
                field: "outcome".into(),
                message: "the final bounded attempt must terminate as succeeded or failed".into(),
            });
        }
        giga_finish_candidate_count(outcome, candidate_count)?;
        if outcome != GigaEventFinishOutcome::Retry && available_at.is_some() {
            return Err(DomainError::InvalidGiga {
                field: "available_at".into(),
                message: "is valid only for a retry receipt".into(),
            });
        }
        let available_at = available_at
            .map(|value| giga_rfc3339("available_at", value))
            .transpose()?;
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            outcome,
            queue_state,
            attempt_count,
            candidate_count,
            available_at,
            finished_at: giga_rfc3339("finished_at", finished_at)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub const fn outcome(&self) -> GigaEventFinishOutcome {
        self.outcome
    }
    pub const fn queue_state(&self) -> GigaQueueState {
        self.queue_state
    }
    pub const fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
    pub const fn candidate_count(&self) -> u32 {
        self.candidate_count
    }
    pub fn available_at(&self) -> Option<&str> {
        self.available_at.as_deref()
    }
    pub fn finished_at(&self) -> &str {
        &self.finished_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventReplayReceipt {
    room: RoomKey,
    event_id: String,
    operator_identity: String,
    previous_state: GigaQueueState,
    queue_state: GigaQueueState,
    attempt_count: u32,
    replayed_at: String,
}

impl GigaEventReplayReceipt {
    pub fn new(
        room: RoomKey,
        event_id: String,
        operator_identity: String,
        previous_state: GigaQueueState,
        queue_state: GigaQueueState,
        attempt_count: u32,
        replayed_at: String,
    ) -> Result<Self, DomainError> {
        if previous_state != GigaQueueState::Failed
            || queue_state != GigaQueueState::Pending
            || attempt_count != 0
        {
            return Err(DomainError::InvalidGiga {
                field: "replay_receipt".into(),
                message: "replay must reset failed work to pending with attempt_count zero".into(),
            });
        }
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            previous_state,
            queue_state,
            attempt_count,
            replayed_at: giga_rfc3339("replayed_at", replayed_at)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.operator_identity
    }
    pub const fn previous_state(&self) -> GigaQueueState {
        self.previous_state
    }
    pub const fn queue_state(&self) -> GigaQueueState {
        self.queue_state
    }
    pub const fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
    pub fn replayed_at(&self) -> &str {
        &self.replayed_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::giga::fixtures::{giga_private_scope, giga_test_source};

    #[test]
    fn giga_queue_claim_finish_and_replay_requests_preserve_authorized_fields() {
        let room = RoomKey::new("lab").unwrap();
        let claim = GigaEventClaimRequest::new(room.clone(), "agents-a1".into(), 60).unwrap();
        assert_eq!(claim.room(), &room);
        assert_eq!(claim.worker_id(), "agents-a1");
        assert_eq!(claim.lease_seconds(), 60);
        let source = giga_test_source("turn-1", 'a', giga_private_scope());
        let event = GigaEvent::new(
            "event-1".into(),
            GigaEventType::ConversationWindow,
            room.clone(),
            "session-1".into(),
            vec![],
            vec![source.clone()],
            GigaLifecycle::conversation_window(),
            "2026-07-24T11:59:00Z".into(),
        )
        .unwrap();
        let claim_receipt = GigaEventClaimReceipt::new(
            room.clone(),
            "agents-a1".into(),
            "2026-07-24T12:00:00Z".into(),
            Some(event),
            Some("2026-07-24T12:01:00Z".into()),
            Some(1),
        )
        .unwrap();
        assert_eq!(claim_receipt.attempt_count(), Some(1));
        assert_eq!(claim_receipt.event().unwrap().source_refs(), &[source]);

        let succeeded = GigaEventFinishRequest::new(
            room.clone(),
            "event-1".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Succeeded,
            1,
            None,
            None,
        )
        .unwrap();
        assert_eq!(succeeded.outcome(), GigaEventFinishOutcome::Succeeded);
        assert_eq!(succeeded.candidate_count(), 1);
        assert_eq!(succeeded.error_class(), None);

        let retry = GigaEventFinishRequest::new(
            room.clone(),
            "event-2".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Retry,
            0,
            Some("model_timeout".into()),
            Some(60),
        )
        .unwrap();
        assert_eq!(retry.error_class(), Some("model_timeout"));
        assert_eq!(retry.retry_after_seconds(), Some(60));

        let failed = GigaEventFinishRequest::new(
            room.clone(),
            "event-3".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Failed,
            0,
            Some("invalid_output".into()),
            None,
        )
        .unwrap();
        assert_eq!(failed.outcome(), GigaEventFinishOutcome::Failed);

        let replay = GigaEventReplayRequest::new(
            room,
            "event-3".into(),
            "sol".into(),
            "operator requested replay after prompt repair".into(),
        )
        .unwrap();
        assert_eq!(replay.operator_identity(), "sol");
        assert_eq!(
            replay.authorization_basis(),
            "operator requested replay after prompt repair"
        );
    }

    #[test]
    fn giga_queue_enforces_lease_bounds_retry_ceiling_and_receipt_shapes() {
        let room = RoomKey::new("lab").unwrap();
        for lease_seconds in [1, GIGA_MAX_LEASE_SECONDS] {
            assert!(
                GigaEventClaimRequest::new(room.clone(), "agents-a1".into(), lease_seconds).is_ok()
            );
        }
        for lease_seconds in [0, GIGA_MAX_LEASE_SECONDS + 1] {
            assert!(matches!(
                GigaEventClaimRequest::new(
                    room.clone(),
                    "agents-a1".into(),
                    lease_seconds,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "lease_seconds"
            ));
        }

        for (outcome, retry_after_seconds) in [
            (GigaEventFinishOutcome::Retry, Some(1)),
            (GigaEventFinishOutcome::Failed, None),
        ] {
            assert!(matches!(
                GigaEventFinishRequest::new(
                    room.clone(),
                    "event-1".into(),
                    "agents-a1".into(),
                    outcome,
                    0,
                    None,
                    retry_after_seconds,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "error_class"
            ));
        }

        assert!(
            GigaEventClaimReceipt::new(
                room.clone(),
                "agents-a1".into(),
                "2026-07-24T12:00:00Z".into(),
                None,
                None,
                None,
            )
            .is_ok()
        );
        assert!(matches!(
            GigaEventClaimReceipt::new(
                room.clone(),
                "agents-a1".into(),
                "2026-07-24T12:00:00Z".into(),
                None,
                Some("2026-07-24T12:01:00Z".into()),
                Some(1),
            ),
            Err(DomainError::InvalidGiga { field, .. }) if field == "claim_receipt"
        ));

        assert!(
            GigaEventFinishReceipt::new(
                room.clone(),
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Retry,
                GigaQueueState::Pending,
                GIGA_MAX_EVENT_ATTEMPTS - 1,
                0,
                Some("2026-07-24T12:02:00Z".into()),
                "2026-07-24T12:01:00Z".into(),
            )
            .is_ok()
        );
        assert!(matches!(
            GigaEventFinishReceipt::new(
                room.clone(),
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Retry,
                GigaQueueState::Pending,
                GIGA_MAX_EVENT_ATTEMPTS,
                0,
                Some("2026-07-24T12:02:00Z".into()),
                "2026-07-24T12:01:00Z".into(),
            ),
            Err(DomainError::InvalidGiga { field, .. }) if field == "outcome"
        ));
        assert!(
            GigaEventFinishReceipt::new(
                room.clone(),
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Failed,
                GigaQueueState::Failed,
                GIGA_MAX_EVENT_ATTEMPTS,
                0,
                None,
                "2026-07-24T12:01:00Z".into(),
            )
            .is_ok()
        );
        assert!(
            GigaEventFinishReceipt::new(
                room,
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Succeeded,
                GigaQueueState::Pending,
                1,
                1,
                None,
                "2026-07-24T12:01:00Z".into(),
            )
            .is_err()
        );
    }

    #[test]
    fn giga_replay_requires_explicit_authorization_and_exact_reset_receipt() {
        let room = RoomKey::new("lab").unwrap();
        for (operator_identity, authorization_basis) in
            [("", "operator requested replay"), ("sol", " ")]
        {
            assert!(
                GigaEventReplayRequest::new(
                    room.clone(),
                    "event-1".into(),
                    operator_identity.into(),
                    authorization_basis.into(),
                )
                .is_err()
            );
        }

        let receipt = GigaEventReplayReceipt::new(
            room.clone(),
            "event-1".into(),
            "sol".into(),
            GigaQueueState::Failed,
            GigaQueueState::Pending,
            0,
            "2026-07-24T12:03:00Z".into(),
        )
        .unwrap();
        assert_eq!(receipt.previous_state(), GigaQueueState::Failed);
        assert_eq!(receipt.queue_state(), GigaQueueState::Pending);
        assert_eq!(receipt.attempt_count(), 0);

        for (previous, current, attempts) in [
            (GigaQueueState::Running, GigaQueueState::Pending, 0),
            (GigaQueueState::Failed, GigaQueueState::Running, 0),
            (GigaQueueState::Failed, GigaQueueState::Pending, 1),
        ] {
            assert!(
                GigaEventReplayReceipt::new(
                    room.clone(),
                    "event-1".into(),
                    "sol".into(),
                    previous,
                    current,
                    attempts,
                    "2026-07-24T12:03:00Z".into(),
                )
                .is_err()
            );
        }
    }
}
