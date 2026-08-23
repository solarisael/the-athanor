use crate::error::DomainError;
use crate::room::RoomKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaQueueState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

impl GigaQueueState {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::UnknownGigaValue {
                field: "queue_state".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaQueueMaintenanceOperation {
    Check,
    PurgeStuck,
}

impl GigaQueueMaintenanceOperation {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "check" => Ok(Self::Check),
            "purge_stuck" => Ok(Self::PurgeStuck),
            other => Err(DomainError::UnknownGigaValue {
                field: "operation".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::PurgeStuck => "purge_stuck",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaQueueMaintenanceScope {
    Room,
    All,
}

impl GigaQueueMaintenanceScope {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "room" => Ok(Self::Room),
            "all" => Ok(Self::All),
            other => Err(DomainError::UnknownGigaValue {
                field: "scope".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::All => "all",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaQueueMaintenanceRequest {
    room: RoomKey,
    operation: GigaQueueMaintenanceOperation,
    scope: GigaQueueMaintenanceScope,
}

impl GigaQueueMaintenanceRequest {
    pub const fn new(
        room: RoomKey,
        operation: GigaQueueMaintenanceOperation,
        scope: GigaQueueMaintenanceScope,
    ) -> Self {
        Self {
            room,
            operation,
            scope,
        }
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }

    pub const fn operation(&self) -> GigaQueueMaintenanceOperation {
        self.operation
    }

    pub const fn scope(&self) -> GigaQueueMaintenanceScope {
        self.scope
    }
}
