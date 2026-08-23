use crate::authority::Authority;
use crate::error::DomainError;
use crate::room::RoomKey;

pub const PAPER_BOAT_MAX_BODY_BYTES: usize = 64 * 1024;
pub const PAPER_BOAT_MAX_UNBOATED: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatSleepRequest {
    room: RoomKey,
    body: String,
    backup: bool,
}

impl PaperBoatSleepRequest {
    pub fn new(room: String, body: String, backup: bool) -> Result<Self, DomainError> {
        let room = RoomKey::for_memory_write(room)?;
        if body.trim().is_empty() {
            return Err(DomainError::InvalidPaperBoat {
                field: "body".into(),
                message: "must not be empty".into(),
            });
        }
        if body.len() > PAPER_BOAT_MAX_BODY_BYTES {
            return Err(DomainError::InvalidPaperBoat {
                field: "body".into(),
                message: format!("must be at most {PAPER_BOAT_MAX_BODY_BYTES} UTF-8 bytes"),
            });
        }
        Ok(Self { room, body, backup })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub const fn backup(&self) -> bool {
        self.backup
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatWakeRequest {
    room: RoomKey,
}

impl PaperBoatWakeRequest {
    pub fn new(room: String) -> Result<Self, DomainError> {
        Ok(Self {
            room: RoomKey::for_memory_write(room)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaperBoatBackupStatus {
    NotRequested,
    Completed,
    Failed,
}

impl PaperBoatBackupStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatSleepReceipt {
    memory_id: u64,
    room: RoomKey,
    source_path: String,
    outbox_event_id: String,
    inserted: bool,
    backup_status: PaperBoatBackupStatus,
    warnings: Vec<String>,
}

impl PaperBoatSleepReceipt {
    pub fn committed(
        memory_id: u64,
        room: RoomKey,
        source_path: String,
        outbox_event_id: String,
        inserted: bool,
        backup_status: PaperBoatBackupStatus,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if memory_id == 0 {
            return Err(DomainError::InvalidPaperBoat {
                field: "memory_id".into(),
                message: "must be positive".into(),
            });
        }
        if source_path.trim().is_empty() {
            return Err(DomainError::EmptySourcePath);
        }
        if outbox_event_id.trim().is_empty() {
            return Err(DomainError::InvalidPaperBoat {
                field: "outbox_event_id".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            memory_id,
            room,
            source_path,
            outbox_event_id,
            inserted,
            backup_status,
            warnings,
        })
    }

    pub const fn memory_id(&self) -> u64 {
        self.memory_id
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    pub fn outbox_event_id(&self) -> &str {
        &self.outbox_event_id
    }

    pub const fn inserted(&self) -> bool {
        self.inserted
    }

    pub const fn backup_status(&self) -> PaperBoatBackupStatus {
        self.backup_status
    }

    pub const fn durable(&self) -> bool {
        true
    }

    pub const fn authority(&self) -> Authority {
        Authority::Full
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnboatedMemory {
    pub id: u64,
    pub title: String,
    pub kind: String,
    pub source_path: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatRecord {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub date: Option<String>,
    pub source_path: String,
    pub created_at: String,
    pub unboated: Vec<UnboatedMemory>,
    pub unboated_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatWakeReceipt {
    room: RoomKey,
    boat: Option<PaperBoatRecord>,
    warnings: Vec<String>,
}

impl PaperBoatWakeReceipt {
    pub fn new(
        room: RoomKey,
        boat: Option<PaperBoatRecord>,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if let Some(boat) = &boat {
            if boat.id == 0 || boat.body.trim().is_empty() {
                return Err(DomainError::InvalidPaperBoat {
                    field: "record".into(),
                    message: "requires a positive ID and non-empty body".into(),
                });
            }
            if boat.unboated.len() > PAPER_BOAT_MAX_UNBOATED {
                return Err(DomainError::InvalidPaperBoat {
                    field: "unboated".into(),
                    message: format!("must contain at most {PAPER_BOAT_MAX_UNBOATED} records"),
                });
            }
        }
        Ok(Self {
            room,
            boat,
            warnings,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }

    pub fn boat(&self) -> Option<&PaperBoatRecord> {
        self.boat.as_ref()
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_boat_requests_bound_body_and_preserve_room_scope() {
        assert!(PaperBoatSleepRequest::new("".into(), "body".into(), true).is_err());
        assert!(PaperBoatSleepRequest::new("lab".into(), " ".into(), true).is_err());
        assert!(
            PaperBoatSleepRequest::new(
                "lab".into(),
                "x".repeat(PAPER_BOAT_MAX_BODY_BYTES + 1),
                true,
            )
            .is_err()
        );
        let request = PaperBoatSleepRequest::new("lab".into(), "body".into(), true).unwrap();
        assert_eq!(request.room().as_str(), "lab");
        assert!(request.backup());
        assert!(PaperBoatWakeRequest::new("other-room".into()).is_ok());
    }

    #[test]
    fn failed_backup_receipt_keeps_postgres_durability_explicit() {
        let receipt = PaperBoatSleepReceipt::committed(
            7,
            RoomKey::for_memory_write("lab").unwrap(),
            "db-only/paper-boats/sha256-deadbeef.md".into(),
            "event-7".into(),
            true,
            PaperBoatBackupStatus::Failed,
            vec!["backup failed after PostgreSQL commit".into()],
        )
        .unwrap();
        assert!(receipt.durable());
        assert_eq!(receipt.authority(), Authority::Full);
        assert_eq!(receipt.backup_status(), PaperBoatBackupStatus::Failed);
        assert_eq!(receipt.warnings().len(), 1);
    }
}
