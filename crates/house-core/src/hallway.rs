use serde::{Deserialize, Serialize};

pub const HALLWAY_MAX_BODY_BYTES: usize = 32 * 1024;
pub const HALLWAY_MAX_ALLOWED_ROOMS: usize = 32;
pub const HALLWAY_MAX_READ_LIMIT: u32 = 200;

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
    #[serde(default = "default_read_limit")]
    pub limit: u32,
    #[serde(default)]
    pub advance_cursor: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayReceipt {
    pub ok: bool,
    pub hallway: String,
    pub created: bool,
    pub duplicate: bool,
    pub operator_visible: bool,
    pub wake_policy: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayPresenceReceipt {
    pub ok: bool,
    pub hallway: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub joined: bool,
    pub duplicate: bool,
    pub read_cursor: i64,
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
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HallwayPostReceipt {
    pub ok: bool,
    pub duplicate: bool,
    pub message: HallwayMessage,
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
    pub wake_policy: String,
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
        Ok(())
    }
}

impl HallwayReadRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.hallway) {
            return Err("hallway must be a lowercase kebab-case key of at most 160 bytes".into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiple_sessions_may_share_one_spirit_identity() {
        let first = HallwayJoinRequest {
            hallway: "shared-hallway".into(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-session-one".into(),
            idempotency_key: "join-one".into(),
        };
        let second = HallwayJoinRequest {
            session: "kintsu-session-two".into(),
            idempotency_key: "join-two".into(),
            ..first.clone()
        };
        assert!(first.validate().is_ok());
        assert!(second.validate().is_ok());
        assert_ne!(first.session, second.session);
        assert_eq!(first.spirit, second.spirit);
    }
}
