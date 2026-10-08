use crate::store::{RoomStateStore, timestamp};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum RoomStateRequest {
    Read,
    Patch {
        #[serde(default, deserialize_with = "deserialize_present")]
        operator: Option<String>,
        #[serde(default, deserialize_with = "deserialize_present")]
        embodied_spirit: Option<String>,
        #[serde(default, deserialize_with = "deserialize_present")]
        routing_mode_enabled: Option<bool>,
        #[serde(default, deserialize_with = "deserialize_present")]
        model_default_enabled: Option<bool>,
        #[serde(default, deserialize_with = "deserialize_nullable")]
        model_default_model: Option<Option<String>>,
    },
    ApplyPrompt {
        prompt: String,
        native_user: bool,
    },
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn deserialize_nullable<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

pub(crate) fn execute(
    store: &RoomStateStore,
    room_dir: &Path,
    configured_spirit: &str,
    request: RoomStateRequest,
) -> Result<Value, String> {
    store.migrate_legacy_files(room_dir)?;
    let mut root = complete_state(store, room_dir, configured_spirit)?;
    let changed = match request {
        RoomStateRequest::Read => return Ok(Value::Object(root)),
        RoomStateRequest::Patch {
            operator,
            embodied_spirit,
            routing_mode_enabled,
            model_default_enabled,
            model_default_model,
        } => apply_patch(
            &mut root,
            configured_spirit,
            operator,
            embodied_spirit,
            routing_mode_enabled,
            model_default_enabled,
            model_default_model,
        )?,
        RoomStateRequest::ApplyPrompt {
            prompt,
            native_user,
        } => native_user && apply_prompt(&mut root, configured_spirit, &prompt)?,
    };
    if changed {
        let now = timestamp();
        root.insert("lastUpdatedAt".into(), Value::String(now));
        let result = Value::Object(root.clone());
        store.write_room_state(&result)?;
        if let Err(error) = write_active_spirit(store, room_dir, &root) {
            return Err(format!(
                "room state was saved, but active_spirit.md mirror failed: {error}"
            ));
        }
    }
    Ok(Value::Object(root))
}

fn complete_state(
    store: &RoomStateStore,
    room_dir: &Path,
    configured_spirit: &str,
) -> Result<Map<String, Value>, String> {
    store.load()?;
    let mut root = store
        .read_root()?
        .as_object()
        .cloned()
        .ok_or_else(|| "room state root must be a JSON object".to_owned())?;
    let embodied = store.embodied_spirit(configured_spirit)?;
    root.entry("version").or_insert(json!(1));
    root.entry("operator")
        .or_insert_with(|| Value::String(room_marker_operator(room_dir)));
    root.insert(
        "agentName".into(),
        Value::String(configured_spirit.to_owned()),
    );
    root.insert("embodiedSpirit".into(), Value::String(embodied));
    root.entry("ignoredSpiritDirective").or_insert(Value::Null);
    root.entry("lastSpiritChangeAt").or_insert(Value::Null);
    root.entry("lastUpdatedAt").or_insert(Value::Null);
    ensure_object_defaults(
        &mut root,
        "routingMode",
        [("enabled", json!(false)), ("updatedAt", Value::Null)],
    )?;
    ensure_object_defaults(
        &mut root,
        "modelDefault",
        [
            ("enabled", json!(false)),
            ("model", Value::Null),
            ("updatedAt", Value::Null),
        ],
    )?;
    Ok(root)
}

fn room_marker_operator(room_dir: &Path) -> String {
    fs::read_to_string(room_dir.join(".athanor-room.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|marker| {
            marker
                .get("operator")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .and_then(|operator| normalize_name(&operator).ok())
        .unwrap_or_else(|| "Operator".to_owned())
}

fn ensure_object_defaults<const N: usize>(
    root: &mut Map<String, Value>,
    key: &str,
    defaults: [(&str, Value); N],
) -> Result<(), String> {
    let value = root.entry(key).or_insert_with(|| Value::Object(Map::new()));
    let object = value
        .as_object_mut()
        .ok_or_else(|| format!("room state {key} must be an object"))?;
    for (name, value) in defaults {
        object.entry(name).or_insert(value);
    }
    Ok(())
}

fn apply_patch(
    root: &mut Map<String, Value>,
    configured_spirit: &str,
    operator: Option<String>,
    embodied_spirit: Option<String>,
    routing_mode_enabled: Option<bool>,
    model_default_enabled: Option<bool>,
    model_default_model: Option<Option<String>>,
) -> Result<bool, String> {
    let mut changed = false;
    if let Some(operator) = operator {
        root.insert("operator".into(), Value::String(normalize_name(&operator)?));
        changed = true;
    }
    if let Some(embodied_spirit) = embodied_spirit {
        let spirit = normalize_name(&embodied_spirit)?;
        root.insert("embodiedSpirit".into(), Value::String(spirit));
        root.insert(
            "agentName".into(),
            Value::String(configured_spirit.to_owned()),
        );
        root.insert("lastSpiritChangeAt".into(), Value::String(timestamp()));
        root.insert("ignoredSpiritDirective".into(), Value::Null);
        changed = true;
    }
    if let Some(enabled) = routing_mode_enabled {
        let routing = root
            .get_mut("routingMode")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "room state routingMode must be an object".to_owned())?;
        routing.insert("enabled".into(), Value::Bool(enabled));
        routing.insert("updatedAt".into(), Value::String(timestamp()));
        changed = true;
    }
    if model_default_enabled.is_some() || model_default_model.is_some() {
        let model = root
            .get_mut("modelDefault")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "room state modelDefault must be an object".to_owned())?;
        if let Some(enabled) = model_default_enabled {
            model.insert("enabled".into(), Value::Bool(enabled));
        }
        if let Some(model_name) = model_default_model {
            model.insert(
                "model".into(),
                model_name.map(Value::String).unwrap_or(Value::Null),
            );
        }
        model.insert("updatedAt".into(), Value::String(timestamp()));
        changed = true;
    }
    Ok(changed)
}

fn apply_prompt(
    root: &mut Map<String, Value>,
    configured_spirit: &str,
    prompt: &str,
) -> Result<bool, String> {
    let operator = last_directive(prompt, &["Operator"]);
    let spirit = last_directive(prompt, &["EMBODY", "CONJURE", "SUMMON"]);
    let dismiss = has_directive(prompt, "DISMISS");
    let mut changed = false;
    if let Some(operator) = operator.filter(|value| !value.is_empty()) {
        root.insert("operator".into(), Value::String(normalize_name(&operator)?));
        changed = true;
    }
    if dismiss {
        root.insert("ignoredSpiritDirective".into(), Value::Null);
        changed = true;
    }
    if let Some(spirit) = spirit.filter(|value| !value.is_empty()) {
        match normalize_name(&spirit) {
            Ok(spirit) => {
                root.insert("embodiedSpirit".into(), Value::String(spirit));
                root.insert(
                    "agentName".into(),
                    Value::String(configured_spirit.to_owned()),
                );
                root.insert("lastSpiritChangeAt".into(), Value::String(timestamp()));
                root.insert("ignoredSpiritDirective".into(), Value::Null);
            }
            Err(_) => {
                root.insert("ignoredSpiritDirective".into(), Value::String(spirit));
            }
        }
        changed = true;
    }
    Ok(changed)
}

fn last_directive(prompt: &str, labels: &[&str]) -> Option<String> {
    let mut found = None;
    for line in prompt.lines() {
        let line = line.trim_start();
        for label in labels {
            if line
                .get(..label.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(label))
            {
                let rest = line[label.len()..].trim_start();
                if let Some(value) = rest.strip_prefix(':') {
                    if !value.is_empty() {
                        found = Some(value.trim().to_owned());
                    }
                }
            }
        }
    }
    found
}

fn has_directive(prompt: &str, label: &str) -> bool {
    prompt.lines().any(|line| {
        let line = line.trim_start();
        if !line
            .get(..label.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(label))
        {
            return false;
        }
        let rest = line[label.len()..].trim_start();
        rest.is_empty()
            || rest
                .strip_prefix(':')
                .is_some_and(|value| !value.trim().is_empty())
    })
}

fn normalize_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.encode_utf16().count() > 80
        || value
            .chars()
            .any(|character| matches!(character, '\r' | '\n' | '|'))
    {
        return Err("room state display name is invalid".into());
    }
    Ok(value.to_owned())
}

fn write_active_spirit(
    store: &RoomStateStore,
    room_dir: &Path,
    root: &Map<String, Value>,
) -> Result<(), String> {
    let path = room_dir.join("active_spirit.md");
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let body = manual_body(&existing);
    let spirit = root
        .get("embodiedSpirit")
        .and_then(Value::as_str)
        .unwrap_or("Spirit");
    let agent = root
        .get("agentName")
        .and_then(Value::as_str)
        .unwrap_or("Spirit");
    let operator = root
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("Operator");
    let body = if body.is_empty() {
        format!("# SPIRIT: {spirit}\n")
    } else {
        body.to_owned()
    };
    let content = format!(
        "# Active Spirit: {spirit}\nAgent: {agent} | Operator: {operator}\nEmbodied: {spirit} | Conjured: none | Summoned: none\n\n{body}"
    );
    store.write_text_mirror(&path, &content)
}

fn manual_body(existing: &str) -> &str {
    let mut lines = existing.splitn(5, '\n');
    let first = lines.next().unwrap_or_default();
    let second = lines.next().unwrap_or_default();
    let third = lines.next().unwrap_or_default();
    let separator = lines.next().unwrap_or_default();
    if first.starts_with("# Active Spirit:")
        && second.starts_with("Agent:")
        && third.starts_with("Embodied:")
        && separator.is_empty()
    {
        lines.next().unwrap_or_default()
    } else {
        existing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn fixture() -> (std::path::PathBuf, RoomStateStore) {
        let dir = std::env::temp_dir().join(format!("athanor-room-state-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".omp/runtime")).expect("fixture directory");
        let path = dir.join(".omp/runtime/athanor-house-state.json");
        let state = json!({
            "room": "room-one",
            "operator": "Operator",
            "agentName": "Configured",
            "embodiedSpirit": "Configured",
            "recallPolicy": {
                "requestedMode": "auto", "resolvedMode": "quiet", "activeProject": null,
                "resolutionReason": "test", "lastRefreshReason": null, "lastRefreshAt": null,
                "workingSetEntries": 0, "recoveryPending": false, "recoveryTerms": [],
                "degraded": null, "updatedAt": null
            },
            "unrelated": {"keep": true}
        });
        fs::write(&path, serde_json::to_vec(&state).unwrap()).expect("state file");
        (dir, RoomStateStore::new(path, "room-one".into()))
    }

    #[test]
    fn nullable_model_field_distinguishes_omission_from_clear() {
        let omitted: RoomStateRequest = serde_json::from_value(json!({"action":"patch"})).unwrap();
        let clear: RoomStateRequest =
            serde_json::from_value(json!({"action":"patch","modelDefaultModel":null})).unwrap();
        assert!(matches!(
            omitted,
            RoomStateRequest::Patch {
                model_default_model: None,
                ..
            }
        ));
        assert!(matches!(
            clear,
            RoomStateRequest::Patch {
                model_default_model: Some(None),
                ..
            }
        ));
        assert!(
            serde_json::from_value::<RoomStateRequest>(json!({"action":"patch","surprise":true}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<RoomStateRequest>(json!({"action":"patch","operator":null}))
                .is_err()
        );
    }

    #[test]
    fn model_clear_changes_state_only_when_null_is_present() {
        let (dir, store) = fixture();
        let set_model = RoomStateRequest::Patch {
            operator: None,
            embodied_spirit: None,
            routing_mode_enabled: None,
            model_default_enabled: None,
            model_default_model: Some(Some("pi/default".into())),
        };
        let set = execute(&store, &dir, "Configured", set_model).unwrap();
        assert_eq!(set["modelDefault"]["model"], "pi/default");

        let omitted = RoomStateRequest::Patch {
            operator: None,
            embodied_spirit: None,
            routing_mode_enabled: None,
            model_default_enabled: None,
            model_default_model: None,
        };
        let unchanged = execute(&store, &dir, "Configured", omitted).unwrap();
        assert_eq!(unchanged["modelDefault"]["model"], "pi/default");

        let clear_model = RoomStateRequest::Patch {
            operator: None,
            embodied_spirit: None,
            routing_mode_enabled: None,
            model_default_enabled: None,
            model_default_model: Some(None),
        };
        let cleared = execute(&store, &dir, "Configured", clear_model).unwrap();
        assert_eq!(cleared["modelDefault"]["model"], Value::Null);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn configured_spirit_fallback_refuses_malformed_or_foreign_state() {
        let (dir, store) = fixture();
        let path = dir.join(".omp/runtime/athanor-house-state.json");
        let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        state.as_object_mut().unwrap().remove("embodiedSpirit");
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(store.embodied_spirit("Configured").unwrap(), "Configured");

        state["embodiedSpirit"] = Value::Null;
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(store.embodied_spirit("Configured").is_err());

        state["room"] = Value::String("foreign-room".into());
        state["embodiedSpirit"] = Value::String("Visitor".into());
        fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(store.embodied_spirit("Configured").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn embodiment_keeps_configured_agent_and_manual_spirit_body() {
        let (dir, store) = fixture();
        fs::write(dir.join("active_spirit.md"), "# Active Spirit: Configured\nAgent: Configured | Operator: Operator\nEmbodied: Configured | Conjured: none | Summoned: none\n\nThe manual body stays.\n").unwrap();
        let output = execute(
            &store,
            &dir,
            "Configured",
            RoomStateRequest::ApplyPrompt {
                prompt: "EMBODY: Earlier\nCONJURE: Later\nSUMMON: New Spirit".into(),
                native_user: true,
            },
        )
        .unwrap();
        assert_eq!(output["agentName"], "Configured");
        assert_eq!(output["embodiedSpirit"], "New Spirit");
        assert_eq!(output["unrelated"]["keep"], true);
        assert!(
            fs::read_to_string(dir.join("active_spirit.md"))
                .unwrap()
                .ends_with("The manual body stays.\n")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_files_move_only_when_canonical_files_are_absent() {
        let (dir, store) = fixture();
        let runtime = dir.join(".omp/runtime");
        let state = runtime.join("athanor-house-state.json");
        let legacy_state = runtime.join("solarisael-house-state.json");
        let original_state = fs::read(&state).unwrap();
        let legacy_marker = dir.join(".solarisael-room.json");
        let marker = dir.join(".athanor-room.json");
        fs::rename(&state, &legacy_state).unwrap();
        fs::write(&legacy_marker, "{\"room\":\"room-one\"}").unwrap();

        store.migrate_legacy_files(&dir).unwrap();
        assert_eq!(fs::read(&state).unwrap(), original_state);
        assert_eq!(
            fs::read(&marker).unwrap(),
            b"{\"room\":\"room-one\"}".to_vec()
        );
        assert!(!legacy_state.exists());
        assert!(!legacy_marker.exists());

        fs::write(&legacy_state, "do not replace canonical state").unwrap();
        fs::write(&legacy_marker, "do not replace canonical marker").unwrap();
        store.migrate_legacy_files(&dir).unwrap();
        assert_eq!(fs::read(&state).unwrap(), original_state);
        assert_eq!(
            fs::read(&marker).unwrap(),
            b"{\"room\":\"room-one\"}".to_vec()
        );
        assert!(legacy_state.exists());
        assert!(legacy_marker.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn non_native_prompt_does_not_apply_directives() {
        let (dir, store) = fixture();
        let output = execute(
            &store,
            &dir,
            "Configured",
            RoomStateRequest::ApplyPrompt {
                prompt: "EMBODY: Stranger".into(),
                native_user: false,
            },
        )
        .unwrap();
        assert_eq!(output["embodiedSpirit"], "Configured");
        assert!(!dir.join("active_spirit.md").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
