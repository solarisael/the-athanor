use serde::{
    Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError, ser::SerializeStruct,
};

pub const HALLWAY_MAX_BODY_BYTES: usize = 32 * 1024;
pub const HALLWAY_MAX_ALLOWED_ROOMS: usize = 32;
pub const HALLWAY_MAX_READ_LIMIT: u32 = 200;
pub const HALLWAY_DEFAULT_MESSAGES_LIMIT: u32 = 30;
pub const HALLWAY_MAX_KNOCK_TURNS: u8 = 8;
pub const HALLWAY_DEFAULT_KNOCK_TURNS: u8 = 4;
pub const HALLWAY_MAX_KNOCK_REASON_BYTES: usize = 2048;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayCreateRequest {
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub allowed_rooms: Vec<String>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayJoinRequest {
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayPostRequest {
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub idempotency_key: String,
    pub body: String,
    #[serde(default)]
    pub reply_to: Option<i64>,
    /// Structured recipients resolved by the composer: stable room keys,
    /// never parsed from body text. Empty means no targeted attention.
    #[serde(default)]
    pub to_rooms: Vec<String>,
}

fn default_read_limit() -> u32 {
    50
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayReadRequest {
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    #[serde(default)]
    pub after: Option<i64>,
    /// Exact daily thread to read. Filtered reads acknowledge only returned
    /// messages and never advance the session cursor across other threads.
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default = "default_read_limit")]
    pub limit: u32,
    #[serde(default)]
    pub advance_cursor: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HallwayCreateDisposition {
    Created,
    Duplicate,
}

impl HallwayCreateDisposition {
    pub const fn is_created(self) -> bool {
        matches!(self, Self::Created)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HallwayReceipt {
    pub ok: bool,
    pub hallway: String,
    pub disposition: HallwayCreateDisposition,
    pub operator_visible: bool,
    pub wake_policy: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HallwayReceiptWire {
    ok: bool,
    hallway: String,
    created: bool,
    duplicate: bool,
    operator_visible: bool,
    wake_policy: String,
}

impl Serialize for HallwayReceipt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("HallwayReceipt", 6)?;
        state.serialize_field("ok", &self.ok)?;
        state.serialize_field("hallway", &self.hallway)?;
        state.serialize_field("created", &self.disposition.is_created())?;
        state.serialize_field("duplicate", &!self.disposition.is_created())?;
        state.serialize_field("operatorVisible", &self.operator_visible)?;
        state.serialize_field("wakePolicy", &self.wake_policy)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for HallwayReceipt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = HallwayReceiptWire::deserialize(deserializer)?;
        let disposition = match (wire.created, wire.duplicate) {
            (true, false) => HallwayCreateDisposition::Created,
            (false, true) => HallwayCreateDisposition::Duplicate,
            _ => return Err(D::Error::custom("created and duplicate must be opposite")),
        };
        Ok(Self {
            ok: wire.ok,
            hallway: wire.hallway,
            disposition,
            operator_visible: wire.operator_visible,
            wake_policy: wire.wake_policy,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HallwayJoinDisposition {
    Joined,
    Duplicate,
}

impl HallwayJoinDisposition {
    pub const fn is_joined(self) -> bool {
        matches!(self, Self::Joined)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HallwayPresenceReceipt {
    pub ok: bool,
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub disposition: HallwayJoinDisposition,
    pub read_cursor: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HallwayPresenceReceiptWire {
    ok: bool,
    hallway: String,
    room: String,
    spirit: String,
    session: String,
    joined: bool,
    duplicate: bool,
    read_cursor: i64,
}

impl Serialize for HallwayPresenceReceipt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("HallwayPresenceReceipt", 8)?;
        state.serialize_field("ok", &self.ok)?;
        state.serialize_field("hallway", &self.hallway)?;
        state.serialize_field("room", &self.room)?;
        state.serialize_field("spirit", &self.spirit)?;
        state.serialize_field("session", &self.session)?;
        state.serialize_field("joined", &self.disposition.is_joined())?;
        state.serialize_field("duplicate", &!self.disposition.is_joined())?;
        state.serialize_field("readCursor", &self.read_cursor)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for HallwayPresenceReceipt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = HallwayPresenceReceiptWire::deserialize(deserializer)?;
        let disposition = match (wire.joined, wire.duplicate) {
            (true, false) => HallwayJoinDisposition::Joined,
            (false, true) => HallwayJoinDisposition::Duplicate,
            _ => return Err(D::Error::custom("joined and duplicate must be opposite")),
        };
        Ok(Self {
            ok: wire.ok,
            hallway: wire.hallway,
            room: wire.room,
            spirit: wire.spirit,
            session: wire.session,
            disposition,
            read_cursor: wire.read_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayMessage {
    pub id: i64,
    pub sequence: i64,
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub body: String,
    pub reply_to: Option<i64>,
    pub created_at: String,
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub to_rooms: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HallwayPostDisposition {
    Posted,
    Duplicate,
}

impl HallwayPostDisposition {
    pub const fn is_duplicate(self) -> bool {
        matches!(self, Self::Duplicate)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HallwayPostReceipt {
    pub ok: bool,
    pub disposition: HallwayPostDisposition,
    pub message: HallwayMessage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HallwayPostReceiptWire {
    ok: bool,
    duplicate: bool,
    message: HallwayMessage,
}

impl Serialize for HallwayPostReceipt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("HallwayPostReceipt", 3)?;
        state.serialize_field("ok", &self.ok)?;
        state.serialize_field("duplicate", &self.disposition.is_duplicate())?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for HallwayPostReceipt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = HallwayPostReceiptWire::deserialize(deserializer)?;
        Ok(Self {
            ok: wire.ok,
            disposition: if wire.duplicate {
                HallwayPostDisposition::Duplicate
            } else {
                HallwayPostDisposition::Posted
            },
            message: wire.message,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayReadReceipt {
    pub ok: bool,
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub previous_cursor: i64,
    pub read_cursor: i64,
    pub messages: Vec<HallwayMessage>,
    pub has_more: bool,
    #[serde(default)]
    pub room_read_sequence: i64,
    #[serde(default)]
    pub acked_mentions: i64,
    #[serde(default)]
    pub thread: Option<String>,
    pub wake_policy: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayInboxRequest {
    pub room: String,
    pub spirit: String,
    pub session: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayInboxNotification {
    pub message_id: i64,
    pub sequence: i64,
    pub thread: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayInboxEntry {
    pub hallway: String,
    /// Derived: next_sequence - 1 - room read_sequence. Never stored.
    pub unread: i64,
    /// Pending targeted notifications for this room.
    pub mentions: i64,
    pub notification_revision: i64,
    pub latest_sequence: i64,
    pub latest_room: Option<String>,
    pub latest_spirit: Option<String>,
    pub latest_excerpt: Option<String>,
    pub latest_created_at: Option<String>,
    /// Pending Bell rows with enough stable identity to open and acknowledge
    /// the exact message thread. Peer prose never crosses in this structure.
    #[serde(default)]
    pub notifications: Vec<HallwayInboxNotification>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayInboxReceipt {
    pub ok: bool,
    pub room: String,
    pub spirit: String,
    pub hallways: Vec<HallwayInboxEntry>,
}

/// Frozen wire shape for the Host's `hallway/messages` panel door: the Pulse
/// GUI builds directly to these names, so a rename here renames the panel.
/// Deliberately not [`HallwayMessage`] - `sequence` and `session` are cursor
/// and identity bookkeeping the glass has no business carrying.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayMessagesRequest {
    pub hallway: String,
    pub room: String,
    pub limit: u32,
    pub before: Option<HallwayMessagesCursor>,
}

/// Keyset door, never an offset: a hallway grows under the scroller while it
/// is open, so a page is named by the id it walks back from.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayMessagesCursor {
    pub id: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayMessagesItem {
    pub id: i64,
    pub room: String,
    pub spirit: String,
    pub body: String,
    pub reply_to: Option<i64>,
    pub created_at: String,
    pub thread: String,
    pub to_rooms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayMessagesPage {
    pub hallway: String,
    pub messages: Vec<HallwayMessagesItem>,
    pub has_more: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HallwayKnockPolicyMode {
    Manual,
    AllowList,
}

impl HallwayKnockPolicyMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AllowList => "allow_list",
        }
    }
}

const fn default_knock_max_turns() -> u8 {
    HALLWAY_DEFAULT_KNOCK_TURNS
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockPolicyRequest {
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub idempotency_key: String,
    pub mode: HallwayKnockPolicyMode,
    #[serde(default)]
    pub allowed_rooms: Vec<String>,
    #[serde(default = "default_knock_max_turns")]
    pub max_turns: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockPolicyReceipt {
    pub ok: bool,
    pub duplicate: bool,
    pub hallway: String,
    pub room: String,
    pub mode: HallwayKnockPolicyMode,
    pub allowed_rooms: Vec<String>,
    pub max_turns: u8,
    pub revision: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockRequest {
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub idempotency_key: String,
    pub message_id: i64,
    pub recipient_room: String,
    #[serde(default)]
    pub parent_knock_id: Option<String>,
    #[serde(default = "default_knock_max_turns")]
    pub max_turns: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockPointer {
    pub knock_id: String,
    pub hallway: String,
    pub message_id: i64,
    pub sequence: i64,
    pub thread: String,
    pub from_room: String,
    pub from_spirit: String,
    pub recipient_room: String,
    pub parent_knock_id: Option<String>,
    pub root_knock_id: String,
    pub turn_index: u8,
    pub max_turns: u8,
    pub status: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockReceipt {
    pub ok: bool,
    pub duplicate: bool,
    pub knock: HallwayKnockPointer,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockClaimRequest {
    pub room: String,
    pub spirit: String,
    pub session: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockClaimReceipt {
    pub ok: bool,
    pub knock: Option<HallwayKnockPointer>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HallwayKnockOutcome {
    Started,
    Completed,
    Failed,
}

impl HallwayKnockOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockSettleRequest {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub knock_id: String,
    pub outcome: HallwayKnockOutcome,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockSettleReceipt {
    pub ok: bool,
    pub duplicate: bool,
    pub knock_id: String,
    pub status: String,
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_identity(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= max
        && !value.chars().any(|character| character.is_control())
}

fn validate_binding(room: &str, spirit: &str, session: &str) -> Result<(), String> {
    if !valid_slug(room) {
        return Err("room must be a lowercase kebab-case key of at most 160 bytes".into());
    }
    if !valid_identity(spirit, 160) {
        return Err("spirit must be 1-160 printable characters".into());
    }
    if !valid_identity(session, 512) {
        return Err("session must be 1-512 printable characters".into());
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), String> {
    if !valid_identity(value, 512) {
        return Err("idempotencyKey must be 1-512 printable characters".into());
    }
    Ok(())
}

impl HallwayCreateRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.hallway) {
            return Err("hallway must be a lowercase kebab-case key of at most 160 bytes".into());
        }
        validate_binding(&self.room, &self.spirit, &self.session)?;
        validate_idempotency_key(&self.idempotency_key)?;
        if self.allowed_rooms.is_empty() || self.allowed_rooms.len() > HALLWAY_MAX_ALLOWED_ROOMS {
            return Err(format!(
                "allowedRooms must contain 1-{HALLWAY_MAX_ALLOWED_ROOMS} rooms"
            ));
        }
        if !self.allowed_rooms.iter().all(|room| valid_slug(room)) {
            return Err("allowedRooms entries must be lowercase kebab-case room keys".into());
        }
        if !self.allowed_rooms.iter().any(|room| room == &self.room) {
            return Err("allowedRooms must include the creating room".into());
        }
        let mut rooms = self.allowed_rooms.clone();
        rooms.sort();
        rooms.dedup();
        if rooms.len() != self.allowed_rooms.len() {
            return Err("allowedRooms must not contain duplicates".into());
        }
        Ok(())
    }
}

impl HallwayJoinRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.hallway) {
            return Err("hallway must be a lowercase kebab-case key of at most 160 bytes".into());
        }
        validate_binding(&self.room, &self.spirit, &self.session)?;
        validate_idempotency_key(&self.idempotency_key)
    }
}

impl HallwayPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        HallwayJoinRequest {
            hallway: self.hallway.clone(),
            room: self.room.clone(),
            spirit: self.spirit.clone(),
            session: self.session.clone(),
            idempotency_key: self.idempotency_key.clone(),
        }
        .validate()?;
        if self.body.trim().is_empty() {
            return Err("body must be non-empty".into());
        }
        if self.body.len() > HALLWAY_MAX_BODY_BYTES {
            return Err(format!(
                "body must be at most {HALLWAY_MAX_BODY_BYTES} UTF-8 bytes"
            ));
        }
        if self.reply_to.is_some_and(|id| id <= 0) {
            return Err("replyTo must be a positive message id".into());
        }
        if self.to_rooms.len() > HALLWAY_MAX_ALLOWED_ROOMS {
            return Err(format!(
                "toRooms must name at most {HALLWAY_MAX_ALLOWED_ROOMS} rooms"
            ));
        }
        for room in &self.to_rooms {
            if !valid_slug(room) {
                return Err(
                    "toRooms entries must be lowercase kebab-case keys of at most 160 bytes".into(),
                );
            }
        }
        if self
            .to_rooms
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != self.to_rooms.len()
        {
            return Err("toRooms must not contain duplicates".into());
        }
        Ok(())
    }
}

impl HallwayReadRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.hallway) {
            return Err("hallway must be a lowercase kebab-case key of at most 160 bytes".into());
        }
        if self
            .thread
            .as_deref()
            .is_some_and(|thread| !valid_slug(thread))
        {
            return Err("thread must be a lowercase kebab-case key of at most 160 bytes".into());
        }
        validate_binding(&self.room, &self.spirit, &self.session)?;
        if self.after.is_some_and(|id| id < 0) {
            return Err("after must be zero or a positive message id".into());
        }
        if self.limit == 0 || self.limit > HALLWAY_MAX_READ_LIMIT {
            return Err(format!("limit must be 1-{HALLWAY_MAX_READ_LIMIT}"));
        }
        Ok(())
    }
}

impl HallwayInboxRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_binding(&self.room, &self.spirit, &self.session)
    }
}

impl HallwayMessagesRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.hallway) {
            return Err("hallway must be a lowercase kebab-case key of at most 160 bytes".into());
        }
        if !valid_slug(&self.room) {
            return Err("room must be a lowercase kebab-case key of at most 160 bytes".into());
        }
        if self.before.is_some_and(|cursor| cursor.id <= 0) {
            return Err("before.id must be a positive message id".into());
        }
        Ok(())
    }

    /// A scroller may ask for any page size; the panel still answers under
    /// the same ceiling `hallway_read` honors.
    pub fn page_limit(&self) -> u32 {
        self.limit.clamp(1, HALLWAY_MAX_READ_LIMIT)
    }
}
fn validate_knock_max_turns(max_turns: u8) -> Result<(), String> {
    if !(1..=HALLWAY_MAX_KNOCK_TURNS).contains(&max_turns) {
        return Err(format!("maxTurns must be 1-{HALLWAY_MAX_KNOCK_TURNS}"));
    }
    Ok(())
}

impl HallwayKnockPolicyRequest {
    pub fn validate(&self) -> Result<(), String> {
        HallwayJoinRequest {
            hallway: self.hallway.clone(),
            room: self.room.clone(),
            spirit: self.spirit.clone(),
            session: self.session.clone(),
            idempotency_key: self.idempotency_key.clone(),
        }
        .validate()?;
        validate_knock_max_turns(self.max_turns)?;
        if self.allowed_rooms.len() > HALLWAY_MAX_ALLOWED_ROOMS {
            return Err(format!(
                "allowedRooms must name at most {HALLWAY_MAX_ALLOWED_ROOMS} rooms"
            ));
        }
        if !self.allowed_rooms.iter().all(|room| valid_slug(room)) {
            return Err("allowedRooms entries must be lowercase kebab-case room keys".into());
        }
        let mut rooms = self.allowed_rooms.clone();
        rooms.sort();
        rooms.dedup();
        if rooms.len() != self.allowed_rooms.len() {
            return Err("allowedRooms must not contain duplicates".into());
        }
        if self.mode == HallwayKnockPolicyMode::Manual && !self.allowed_rooms.is_empty() {
            return Err("manual policy must have an empty allowedRooms list".into());
        }
        Ok(())
    }
}

impl HallwayKnockRequest {
    pub fn validate(&self) -> Result<(), String> {
        HallwayJoinRequest {
            hallway: self.hallway.clone(),
            room: self.room.clone(),
            spirit: self.spirit.clone(),
            session: self.session.clone(),
            idempotency_key: self.idempotency_key.clone(),
        }
        .validate()?;
        if self.message_id <= 0 {
            return Err("messageId must be a positive message id".into());
        }
        if !valid_slug(&self.recipient_room) {
            return Err(
                "recipientRoom must be a lowercase kebab-case key of at most 160 bytes".into(),
            );
        }
        if self.recipient_room == self.room {
            return Err("recipientRoom must name a different room".into());
        }
        validate_knock_max_turns(self.max_turns)
    }
}

impl HallwayKnockClaimRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_binding(&self.room, &self.spirit, &self.session)
    }
}

impl HallwayKnockSettleRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_binding(&self.room, &self.spirit, &self.session)?;
        if let Some(reason) = &self.reason {
            if !valid_identity(reason, HALLWAY_MAX_KNOCK_REASON_BYTES) {
                return Err(format!(
                    "reason must be 1-{HALLWAY_MAX_KNOCK_REASON_BYTES} printable characters"
                ));
            }
        }
        Ok(())
    }
}
