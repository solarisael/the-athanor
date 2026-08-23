/// PostgreSQL `pointer_files` is store-shaped JSON with legacy variants: plain
/// string paths and objects carrying extra keys such as `note`. The recall wire
/// contract is exactly `{file, lines}` (protocol `RecallCanonFile`,
/// deny_unknown_fields), so one legacy key would refuse the whole recall
/// envelope downstream. Normalize here; the database row keeps its full shape.
pub(super) fn protocol_pointer_files(stored: &serde_json::Value) -> serde_json::Value {
    let Some(entries) = stored.as_array() else {
        return serde_json::Value::Array(Vec::new());
    };
    let mut files = Vec::new();
    for entry in entries {
        if let Some(file) = entry.as_str() {
            if !file.trim().is_empty() {
                files.push(serde_json::json!({ "file": file }));
            }
            continue;
        }
        let Some(file) = entry
            .get("file")
            .and_then(serde_json::Value::as_str)
            .filter(|file| !file.trim().is_empty())
        else {
            continue;
        };
        let lines: Vec<u64> = entry
            .get("lines")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_u64)
                    .collect()
            })
            .unwrap_or_default();
        if lines.is_empty() {
            files.push(serde_json::json!({ "file": file }));
        } else {
            files.push(serde_json::json!({ "file": file, "lines": lines }));
        }
    }
    serde_json::Value::Array(files)
}
