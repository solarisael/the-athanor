//! The effective room marker: the House page with the room page laid over it.
//!
//! The House page lives at `<parent of room_dir>/house/.athanor-house.json`.
//! The room page lives at `<room_dir>/.athanor-room.json`. A room key replaces
//! the House value whole, the same rule the spellbook uses. `room` is never
//! inherited: a grant approves one named room, and only the room page names it.
//! A House page that is missing or malformed contributes nothing.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub const HOUSE_MARKER_FILENAME: &str = ".athanor-house.json";
pub const ROOM_MARKER_FILENAME: &str = ".athanor-room.json";

/// A parentless room has no House page.
pub fn house_marker_path(room_dir: &Path) -> Option<PathBuf> {
    Some(room_dir.parent()?.join("house").join(HOUSE_MARKER_FILENAME))
}

/// The effective marker, read synchronously. `Value::Null` when neither page parses.
pub fn read_room_marker(room_dir: &Path) -> Value {
    let house = house_marker_path(room_dir).and_then(|path| parse(std::fs::read(path).ok()));
    let room = parse(std::fs::read(room_dir.join(ROOM_MARKER_FILENAME)).ok());

    layer_marker(house, room)
}

/// The effective marker, read on the async runtime. `Value::Null` when neither page parses.
pub async fn read_room_marker_async(room_dir: &Path) -> Value {
    let house = match house_marker_path(room_dir) {
        Some(path) => parse(tokio::fs::read(path).await.ok()),
        None => None,
    };
    let room = parse(tokio::fs::read(room_dir.join(ROOM_MARKER_FILENAME)).await.ok());

    layer_marker(house, room)
}

fn parse(bytes: Option<Vec<u8>>) -> Option<Map<String, Value>> {
    match serde_json::from_slice(&bytes?).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

pub fn layer_marker(house: Option<Map<String, Value>>, room: Option<Map<String, Value>>) -> Value {
    let mut merged = house.unwrap_or_default();
    merged.remove("room");
    if let Some(room) = room {
        merged.extend(room);
    }

    if merged.is_empty() {
        Value::Null
    } else {
        Value::Object(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> Option<Map<String, Value>> {
        match value {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    #[test]
    fn room_keys_replace_house_values_whole_and_room_is_never_inherited() {
        let house = object(json!({
            "room": "house",
            "recallTelemetry": true,
            "jevGiga": {"mode": "active", "grant": {"policyRevision": "house-r1"}},
            "jevMode": {"mode": "active"},
        }));
        let room = object(json!({
            "room": "kodo",
            "jevGiga": {"mode": "shadow"},
        }));

        let merged = layer_marker(house, room);
        assert_eq!(merged["room"], "kodo");
        assert_eq!(merged["recallTelemetry"], true);
        assert_eq!(merged["jevMode"]["mode"], "active");
        assert_eq!(merged["jevGiga"], json!({"mode": "shadow"}));
    }

    #[test]
    fn a_house_page_alone_never_names_a_room() {
        let house = object(json!({"room": "house", "jevGiga": {"mode": "active"}}));
        let merged = layer_marker(house, None);
        assert!(merged.get("room").is_none());
        assert_eq!(merged["jevGiga"]["mode"], "active");
        assert_eq!(layer_marker(None, None), Value::Null);
    }

    #[test]
    fn the_effective_marker_reads_both_pages_from_disk() {
        let root = std::env::temp_dir().join(format!("athanor-marker-{}", uuid::Uuid::new_v4().simple()));
        let room_dir = root.join("kodo");
        std::fs::create_dir_all(root.join("house")).unwrap();
        std::fs::create_dir_all(&room_dir).unwrap();
        std::fs::write(
            root.join("house").join(HOUSE_MARKER_FILENAME),
            r#"{"jevGiga":{"mode":"active"},"recallTelemetry":true}"#,
        )
        .unwrap();
        std::fs::write(room_dir.join(ROOM_MARKER_FILENAME), r#"{"room":"kodo"}"#).unwrap();

        let merged = read_room_marker(&room_dir);
        assert_eq!(merged["room"], "kodo");
        assert_eq!(merged["jevGiga"]["mode"], "active");
        assert_eq!(merged["recallTelemetry"], true);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
