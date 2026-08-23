use crate::error::DomainError;
use crate::room::RoomKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonAuthority {
    Active,
    Superseded,
    Archived,
}

impl CanonAuthority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "archived" => Ok(Self::Archived),
            other => Err(DomainError::InvalidCanon {
                field: "authority".into(),
                message: format!("unknown authority: {other}"),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonAttribution {
    actor: String,
    origin: String,
}

impl CanonAttribution {
    pub fn new(actor: String, origin: String) -> Result<Self, DomainError> {
        let actor = canon_nonempty("attribution.actor", actor)?;
        let origin = canon_nonempty("attribution.origin", origin)?;
        Ok(Self { actor, origin })
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonPointer {
    file: String,
    lines: Option<(u32, u32)>,
}

impl CanonPointer {
    pub fn new(file: String, lines: Option<(u32, u32)>) -> Result<Self, DomainError> {
        let file = canon_nonempty("pointerFiles.file", file)?;
        if matches!(lines, Some((start, end)) if start > end) {
            return Err(DomainError::InvalidCanon {
                field: "pointerFiles.lines".into(),
                message: "start must not exceed end".into(),
            });
        }
        Ok(Self { file, lines })
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn lines(&self) -> Option<(u32, u32)> {
        self.lines
    }
}

fn canon_nonempty(field: &str, value: String) -> Result<String, DomainError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(DomainError::InvalidCanon {
            field: field.into(),
            message: "must not be blank".into(),
        });
    }
    Ok(value)
}

const MAX_CANON_VALUES: usize = 64;

fn canon_values(field: &str, values: Vec<String>) -> Result<Vec<String>, DomainError> {
    if values.len() > MAX_CANON_VALUES {
        return Err(DomainError::TooManyValues {
            field: field.into(),
        });
    }
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = canon_nonempty(field, value)?;
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    Ok(normalized)
}

fn canon_date(value: String) -> Result<String, DomainError> {
    let bytes = value.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if !shaped {
        return Err(DomainError::InvalidCanon {
            field: "summaryAsOf".into(),
            message: "must use YYYY-MM-DD".into(),
        });
    }
    Ok(value)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonWriteRequest {
    room: RoomKey,
    name: String,
    kind: String,
    summary: String,
    aliases: Vec<String>,
    search_boost: Option<String>,
    weighty: bool,
    pointer_files: Vec<CanonPointer>,
    summary_as_of: Option<String>,
    supersedes: Vec<u64>,
    attribution: CanonAttribution,
}

impl CanonWriteRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        room: String,
        name: String,
        kind: String,
        summary: String,
        aliases: Vec<String>,
        search_boost: Option<String>,
        weighty: bool,
        pointer_files: Vec<CanonPointer>,
        summary_as_of: Option<String>,
        supersedes: Vec<u64>,
        attribution: CanonAttribution,
    ) -> Result<Self, DomainError> {
        let room = RoomKey::for_canon(room)?;
        let name = canon_nonempty("name", name)?;
        let kind = canon_nonempty("kind", kind)?;
        let summary = canon_nonempty("summary", summary)?;
        let aliases = canon_values("aliases", aliases)?;
        let search_boost = search_boost
            .map(|value| canon_nonempty("searchBoost", value))
            .transpose()?;
        if pointer_files.len() > MAX_CANON_VALUES {
            return Err(DomainError::TooManyValues {
                field: "pointerFiles".into(),
            });
        }
        if supersedes.len() > MAX_CANON_VALUES {
            return Err(DomainError::TooManyValues {
                field: "supersedes".into(),
            });
        }
        if supersedes.iter().any(|id| *id == 0) {
            return Err(DomainError::InvalidCanon {
                field: "supersedes".into(),
                message: "IDs must be positive".into(),
            });
        }
        let mut supersedes = supersedes;
        supersedes.sort_unstable();
        supersedes.dedup();
        let summary_as_of = summary_as_of.map(canon_date).transpose()?;
        Ok(Self {
            room,
            name,
            kind,
            summary,
            aliases,
            search_boost,
            weighty,
            pointer_files,
            summary_as_of,
            supersedes,
            attribution,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    pub fn search_boost(&self) -> Option<&str> {
        self.search_boost.as_deref()
    }
    pub fn weighty(&self) -> bool {
        self.weighty
    }
    pub fn pointer_files(&self) -> &[CanonPointer] {
        &self.pointer_files
    }
    pub fn summary_as_of(&self) -> Option<&str> {
        self.summary_as_of.as_deref()
    }
    pub fn supersedes(&self) -> &[u64] {
        &self.supersedes
    }
    pub fn attribution(&self) -> &CanonAttribution {
        &self.attribution
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonSelector {
    Id(u64),
    Name(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonReadRequest {
    room: RoomKey,
    selector: CanonSelector,
    include_history: bool,
}

impl CanonReadRequest {
    pub fn new(
        room: String,
        id: Option<u64>,
        name: Option<String>,
        include_history: bool,
    ) -> Result<Self, DomainError> {
        let room = RoomKey::for_canon(room)?;
        let selector = match (id, name) {
            (Some(id), None) if id > 0 => CanonSelector::Id(id),
            (None, Some(name)) => CanonSelector::Name(canon_nonempty("name", name)?),
            _ => {
                return Err(DomainError::InvalidCanon {
                    field: "selector".into(),
                    message: "provide exactly one positive id or nonblank name".into(),
                });
            }
        };
        Ok(Self {
            room,
            selector,
            include_history,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn selector(&self) -> &CanonSelector {
        &self.selector
    }
    pub fn include_history(&self) -> bool {
        self.include_history
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonWriteReceipt {
    entity_id: u64,
    room: RoomKey,
    name: String,
    superseded_entity_ids: Vec<u64>,
    attribution: CanonAttribution,
}

impl CanonWriteReceipt {
    pub fn new(
        entity_id: u64,
        room: String,
        name: String,
        superseded_entity_ids: Vec<u64>,
        attribution: CanonAttribution,
    ) -> Result<Self, DomainError> {
        if entity_id == 0 {
            return Err(DomainError::InvalidCanon {
                field: "entityId".into(),
                message: "must be positive".into(),
            });
        }
        Ok(Self {
            entity_id,
            room: RoomKey::for_canon(room)?,
            name: canon_nonempty("name", name)?,
            superseded_entity_ids,
            attribution,
        })
    }

    pub fn entity_id(&self) -> u64 {
        self.entity_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn superseded_entity_ids(&self) -> &[u64] {
        &self.superseded_entity_ids
    }
    pub fn attribution(&self) -> &CanonAttribution {
        &self.attribution
    }
}
