use hearth::{Authority, BackupOutcome, DomainError, OperatorKey, RoomKey};

pub const PAPER_BOAT_MAX_BODY_BYTES: usize = 64 * 1024;
pub const PAPER_BOAT_MAX_UNBOATED: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatSleepRequest {
    room: RoomKey,
    body: String,
    backup: bool,
    author: Option<String>,
    operator: Option<OperatorKey>,
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
        Ok(Self {
            room,
            body,
            backup,
            author: None,
            operator: None,
        })
    }

    pub fn authored_by_operator(
        mut self,
        author: String,
        operator: Option<OperatorKey>,
    ) -> Result<Self, DomainError> {
        if author.trim().is_empty() {
            return Err(DomainError::InvalidPaperBoat {
                field: "author".into(),
                message: "must not be empty".into(),
            });
        }
        self.author = Some(author);
        self.operator = operator;
        Ok(self)
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn operator(&self) -> Option<&OperatorKey> {
        self.operator.as_ref()
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatSleepReceipt {
    memory_id: u64,
    room: RoomKey,
    source_path: String,
    outbox_event_id: String,
    inserted: bool,
    backup: BackupOutcome,
    warnings: Vec<String>,
}

impl PaperBoatSleepReceipt {
    pub fn committed(
        memory_id: u64,
        room: RoomKey,
        source_path: String,
        outbox_event_id: String,
        inserted: bool,
        backup: BackupOutcome,
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
            backup,
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

    /// The file backup that followed the PostgreSQL commit.
    pub const fn backup(&self) -> &BackupOutcome {
        &self.backup
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

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PaperBoatAuthor {
    Operator { name: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatRecord {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub date: Option<String>,
    pub source_path: String,
    pub created_at: String,
    pub author: Option<PaperBoatAuthor>,
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
            if let Some(PaperBoatAuthor::Operator { name }) = &boat.author
                && name.trim().is_empty()
            {
                return Err(DomainError::InvalidPaperBoat {
                    field: "author".into(),
                    message: "must not be empty".into(),
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
