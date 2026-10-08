use super::*;

pub(super) fn material(
    id: String,
    authority: Value,
    role: &str,
    body: String,
    salience: u16,
) -> Value {
    json!({"id":id,"authority":authority,"role":role,"body":body,"salience":salience})
}

pub(super) fn pulse(state: &AppState) -> Result<Option<Value>, String> {
    let body = match std::fs::read(state.config.room_dir.join("presence-pulse.md")) {
        Ok(body) => String::from_utf8_lossy(&body).trim().to_owned(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Presence pulse unavailable: {error}")),
    };
    if body.is_empty() {
        return Ok(None);
    }
    let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
    let bounded = if body.encode_utf16().count() <= 4096 {
        body
    } else {
        format!("{}\n[truncated at 4096 characters]", clip(&body, 4096))
    };
    Ok(Some(material(
        "relationship:presence-pulse".into(),
        json!({"kind":"identity","source":"presence-pulse.md","sha256":digest}),
        "relationship",
        bounded,
        975,
    )))
}

fn excerpt(body: &str, carrier: &str) -> String {
    let body = body.trim();
    if body.encode_utf16().count() <= 700 {
        return body.into();
    }
    let hard = clip(body, 700);
    let soft = hard
        .rfind(' ')
        .filter(|index| *index > 0)
        .map(|index| &hard[..index])
        .unwrap_or(&hard);
    format!("{soft}\n[excerpt; the full text rides this session's {carrier} reminder]")
}

pub(super) fn boat(wake: &Value) -> Option<Value> {
    let id = wake["id"].as_u64().filter(|id| *id > 0)?;
    let body = excerpt(text(&wake["wake_context"]), "athanor-wake-context");
    if body.is_empty() {
        return None;
    }
    Some(material(
        format!("paper-boat:{id}"),
        json!({"kind":"paper_boat","memory_id":id}),
        "continuity",
        body,
        900,
    ))
}

pub(super) fn counsel(content: &str) -> Vec<Value> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    vec![material(
        "anamnesis:wake".into(),
        json!({"kind":"anamnesis","source":"anamnesis:wake"}),
        "counsel",
        excerpt(content, "athanor-anamnesis-wake"),
        800,
    )]
}

pub(super) fn lessons(
    baseline: &[lessons::Lesson],
    triggers: &[lessons::Lesson],
    mode: &str,
    kept_ids: Option<&std::collections::HashSet<u64>>,
) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    baseline
        .iter()
        .filter(|lesson| mode == "work" && kept_ids.is_none_or(|ids| ids.contains(&lesson.id)))
        .chain(triggers)
        .filter(|lesson| seen.insert(lesson.id) && lesson.id > 0 && !lesson.body.trim().is_empty())
        .map(|lesson| {
            material(
                format!("lesson:{}", lesson.id),
                json!({"kind":"lesson","lesson_id":lesson.id,"version":"current"}),
                "rule",
                clip(lesson.body.trim(), 4096),
                700,
            )
        })
        .collect()
}

pub(super) fn recalled(presentation: &Value) -> Vec<Value> {
    let mut materials = Vec::new();
    for candidate in array(&presentation["retrievalCandidates"]) {
        let id = candidate["memory_id"]
            .as_u64()
            .or_else(|| candidate["memoryId"].as_u64())
            .unwrap_or_default();
        let body = first_text(candidate, &["excerpt", "body", "title"]);
        if id > 0 && !body.trim().is_empty() {
            materials.push(material(
                format!("memory:{id}"),
                json!({"kind":"memory","memory_id":id}),
                "continuity",
                clip(body.trim(), 4096),
                650,
            ));
        }
    }
    for candidate in array(&presentation["canonMatches"]) {
        let identifier = ["id", "entity_id", "entityId"]
            .iter()
            .find_map(|key| candidate.get(*key).filter(|value| !value.is_null()));
        let id = match identifier {
            Some(Value::String(value)) => std::borrow::Cow::Borrowed(value.trim()),
            Some(Value::Number(value)) => std::borrow::Cow::Owned(value.to_string()),
            _ => continue,
        };
        let body = first_text(candidate, &["summary", "name"]).trim();
        if !id.is_empty() && !body.is_empty() {
            materials.push(material(
                format!("canon:{id}"),
                json!({"kind":"canon","entity_id":id}),
                "identity",
                clip(body, 4096),
                950,
            ));
        }
    }
    materials.truncate(16);
    materials
}

fn first_text<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|key| value.get(*key).filter(|value| !value.is_null()).map(text))
        .unwrap_or_default()
}

pub(super) fn board(receipt: &Value) -> String {
    if receipt["ok"] != true {
        return String::new();
    }
    let mut lines = Vec::new();
    for quest in array(&receipt["quests"]) {
        let title = text(&quest["title"])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if title.is_empty() {
            continue;
        }
        let state = text(&quest["state"]);
        let importance = text(&quest["importance"]);
        let deadline = text(&quest["deadlineAt"]);
        lines.push(format!(
            "- {title} — {}, {}, {}",
            if state.is_empty() {
                "unknown state"
            } else {
                state
            },
            if importance.is_empty() {
                "hint"
            } else {
                importance
            },
            if deadline.is_empty() {
                "no deadline".into()
            } else {
                format!("due {deadline}")
            }
        ));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("## Quest board\n{}", lines.join("\n"))
}

pub(super) fn anamnesis(result: &Value) -> String {
    if result["ok"] != true || array(&result["entries"]).is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = [
        "<athanor-memories>",
        "Automatic Anamnesis counsel (not present-state truth).",
        "The Cabinet is counsel, not present-state truth.",
        "Pillars are standing places.",
        "Active cycles are prior patterns to verify against the live turn.",
        "Never assert a cycle is active merely because it loaded.",
        "Fidelity: record=true-as-said; raw-material=true-as-reforged.",
        "Source paths are citations.",
        "",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for entry in array(&result["entries"]) {
        anamnesis_entry(entry, &mut lines);
    }
    let warnings: Vec<&str> = array(&result["warnings"])
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if !warnings.is_empty() {
        lines.push(format!("Warnings: {}", warnings.join(" | ")));
    }
    lines.push("</athanor-memories>".into());
    let output = lines.join("\n");
    if output.encode_utf16().count() <= 8000 {
        return output;
    }
    let suffix = "\n...[anamnesis context clipped]\n</athanor-memories>";
    format!(
        "{}{}",
        clip(&output, 8000 - suffix.len()).trim_end(),
        suffix
    )
}

fn anamnesis_entry(entry: &Value, lines: &mut Vec<String>) {
    let kind = text(&entry["kind"]);
    let title = text(&entry["title"]);
    let mut tags = vec![if kind.is_empty() {
        "entry".into()
    } else {
        kind.into()
    }];
    for key in ["fidelity", "activation"] {
        if !text(&entry[key]).is_empty() {
            tags.push(format!("{key}={}", text(&entry[key])));
        }
    }
    tags.push(format!(
        "state={}",
        match entry["active"].as_bool() {
            Some(true) => "active",
            Some(false) => "inactive",
            None => "unspecified",
        }
    ));
    lines.push(format!(
        "[{}] {}",
        tags.join("; "),
        if title.is_empty() {
            "(untitled)"
        } else {
            title
        }
    ));
    for (label, key) in [
        ("Shape", "shape"),
        ("Peak", "peak"),
        ("Beginning", "beginning"),
        ("Ramp", "ramp"),
        ("Counsel", "counsel"),
        ("Verify", "verify_note"),
    ] {
        if !text(&entry[key]).is_empty() {
            lines.push(format!("{label}: {}", text(&entry[key])));
        }
    }
    for (label, key) in [
        ("Tags", "tags"),
        ("Canon", "canon_links"),
        ("Sources", "source_paths"),
    ] {
        let values: Vec<&str> = array(&entry[key])
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| !value.is_empty())
            .collect();
        if !values.is_empty() {
            lines.push(format!("{label}: {}", values.join(", ")));
        }
    }
    for rep in array(&entry["reps"]) {
        let number = rep["rep_number"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| rep["rep_number"].as_u64().map(|value| value.to_string()))
            .unwrap_or_else(|| "?".into());
        let date = text(&rep["occurred_on"]);
        lines.push(format!(
            "Rep {number}{}: {}",
            if date.is_empty() {
                String::new()
            } else {
                format!(" ({date})")
            },
            text(&rep["how_it_went"])
        ));
        for (label, key) in [
            ("Portal pull", "portal_pull"),
            ("Lighter", "lighter"),
            ("Rep source", "source_path"),
        ] {
            if !text(&rep[key]).is_empty() {
                lines.push(format!("{label}: {}", text(&rep[key])));
            }
        }
    }
    lines.push(String::new());
}

pub(super) fn directives(spirit: &str, operator: &str, lessons: &[Value]) -> Vec<Value> {
    let mut directives = vec![
        json!({"id":"presence:active-spirit","kind":"enact","severity":"advisory","instruction":format!("Remain {spirit}; meet {operator} directly and preserve cited uncertainty."),"sourceIds":["identity:active-spirit"],"triggerScope":["text"]}),
        json!({"id":"presence:nonempty-response","kind":"guard","severity":"hard","instruction":"The response must contain text.","sourceIds":["identity:active-spirit"],"triggerScope":["text"]}),
    ];
    for lesson in lessons.iter().take(30) {
        directives.push(json!({"id":format!("presence:{}",text(&lesson["id"])),"kind":"enact","severity":"advisory","instruction":clip(text(&lesson["body"]),1000),"sourceIds":[lesson["id"]],"triggerScope":["text"]}));
    }
    directives
}
