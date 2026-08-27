use crate::error::DomainError;
use crate::room::RoomKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaVisibility {
    Private,
    Shared,
}

impl GigaVisibility {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "private" => Ok(Self::Private),
            "shared" => Ok(Self::Shared),
            other => Err(DomainError::UnknownGigaValue {
                field: "visibility".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaScope {
    room: Option<RoomKey>,
    project: Option<String>,
    visibility: GigaVisibility,
    publication_review_required: bool,
}

impl GigaScope {
    pub fn new(
        room: Option<String>,
        project: Option<String>,
        visibility: GigaVisibility,
        publication_review_required: bool,
    ) -> Result<Self, DomainError> {
        let room = room.map(RoomKey::new).transpose()?;
        if (visibility == GigaVisibility::Private) != room.is_some() {
            return Err(DomainError::InvalidGiga {
                field: "scope.room".into(),
                message: "private scope requires one room and shared scope requires null room"
                    .into(),
            });
        }
        Ok(Self {
            room,
            project: project
                .map(|value| giga_nonempty("project", value))
                .transpose()?,
            visibility,
            publication_review_required,
        })
    }

    pub fn room(&self) -> Option<&RoomKey> {
        self.room.as_ref()
    }
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    pub const fn visibility(&self) -> GigaVisibility {
        self.visibility
    }
    pub const fn publication_review_required(&self) -> bool {
        self.publication_review_required
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaSourceType {
    Turn,
    LifecycleEvent,
    ToolResultSummary,
    TaskContract,
}

impl GigaSourceType {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "turn" => Ok(Self::Turn),
            "lifecycle_event" => Ok(Self::LifecycleEvent),
            "tool_result_summary" => Ok(Self::ToolResultSummary),
            "task_contract" => Ok(Self::TaskContract),
            other => Err(DomainError::UnknownGigaValue {
                field: "source_type".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::LifecycleEvent => "lifecycle_event",
            Self::ToolResultSummary => "tool_result_summary",
            Self::TaskContract => "task_contract",
        }
    }
}

pub(crate) fn giga_nonempty(field: &str, value: String) -> Result<String, DomainError> {
    if value.trim().is_empty() {
        Err(DomainError::InvalidGiga {
            field: field.into(),
            message: "must not be empty".into(),
        })
    } else {
        Ok(value)
    }
}

pub(crate) fn giga_strings(field: &str, values: Vec<String>) -> Result<Vec<String>, DomainError> {
    for value in &values {
        giga_nonempty(field, value.clone())?;
    }
    Ok(values)
}

pub(crate) fn giga_hash(field: &str, value: String) -> Result<String, DomainError> {
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().all(|byte| !byte.is_ascii_uppercase())
    {
        Ok(value)
    } else {
        Err(DomainError::InvalidGigaHash {
            field: field.into(),
        })
    }
}

pub(crate) fn giga_rfc3339(field: &str, value: String) -> Result<String, DomainError> {
    fn digits(bytes: &[u8]) -> bool {
        bytes.iter().all(u8::is_ascii_digit)
    }
    fn number(bytes: &[u8]) -> u32 {
        bytes
            .iter()
            .fold(0, |value, byte| value * 10 + u32::from(byte - b'0'))
    }
    let bytes = value.as_bytes();
    let invalid = || DomainError::InvalidGiga {
        field: field.into(),
        message: "must be an RFC3339 timestamp".into(),
    };
    if bytes.len() < 20
        || !digits(&bytes[0..4])
        || bytes[4] != b'-'
        || !digits(&bytes[5..7])
        || bytes[7] != b'-'
        || !digits(&bytes[8..10])
        || bytes[10] != b'T'
        || !digits(&bytes[11..13])
        || bytes[13] != b':'
        || !digits(&bytes[14..16])
        || bytes[16] != b':'
        || !digits(&bytes[17..19])
    {
        return Err(invalid());
    }
    let year = number(&bytes[0..4]);
    let month = number(&bytes[5..7]);
    let day = number(&bytes[8..10]);
    let hour = number(&bytes[11..13]);
    let minute = number(&bytes[14..16]);
    let second = number(&bytes[17..19]);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if day == 0 || day > max_day || hour > 23 || minute > 59 || second > 59 {
        return Err(invalid());
    }
    let mut zone = 19;
    if bytes.get(zone) == Some(&b'.') {
        zone += 1;
        let start = zone;
        while bytes.get(zone).is_some_and(u8::is_ascii_digit) {
            zone += 1;
        }
        if zone == start {
            return Err(invalid());
        }
    }
    match bytes.get(zone) {
        Some(b'Z') if zone + 1 == bytes.len() => {}
        Some(b'+' | b'-')
            if zone + 6 == bytes.len()
                && digits(&bytes[zone + 1..zone + 3])
                && bytes[zone + 3] == b':'
                && digits(&bytes[zone + 4..zone + 6])
                && number(&bytes[zone + 1..zone + 3]) <= 23
                && number(&bytes[zone + 4..zone + 6]) <= 59 => {}
        _ => return Err(invalid()),
    }
    Ok(value)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaSourceRange {
    start: u64,
    end: u64,
}

impl GigaSourceRange {
    pub fn new(start: u64, end: u64) -> Result<Self, DomainError> {
        if start >= end {
            return Err(DomainError::InvalidGiga {
                field: "range".into(),
                message: "start must be less than end".into(),
            });
        }
        Ok(Self { start, end })
    }
    pub const fn start(&self) -> u64 {
        self.start
    }
    pub const fn end(&self) -> u64 {
        self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaSourceRef {
    source_type: GigaSourceType,
    source_id: String,
    role: String,
    timestamp: String,
    content_hash: String,
    scope: GigaScope,
    range: Option<GigaSourceRange>,
}

impl GigaSourceRef {
    pub fn new(
        source_type: GigaSourceType,
        source_id: String,
        role: String,
        timestamp: String,
        content_hash: String,
        scope: GigaScope,
        range: Option<GigaSourceRange>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            source_type,
            source_id: giga_nonempty("source_id", source_id)?,
            role: giga_nonempty("role", role)?,
            timestamp: giga_rfc3339("timestamp", timestamp)?,
            content_hash: giga_hash("content_hash", content_hash)?,
            scope,
            range,
        })
    }
    pub const fn source_type(&self) -> GigaSourceType {
        self.source_type
    }
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    pub fn role(&self) -> &str {
        &self.role
    }
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn scope(&self) -> &GigaScope {
        &self.scope
    }
    pub fn range(&self) -> Option<&GigaSourceRange> {
        self.range.as_ref()
    }
}
