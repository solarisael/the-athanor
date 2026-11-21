use super::*;
use akasha::operator_card::{OperatorCardRow, operator_card_rows};

const CARD_MAX_ROWS: usize = 40;
const CARD_MAX_CHARS: usize = 4096;
const CARD_ROW_MAX_CHARS: usize = 200;
const CARD_CUT_MARK: &str =
    "\n[operator card cut: more weighty facts exist; list them with canon_read and the subject]";

pub(super) fn material(
    id: String,
    authority: Value,
    role: &str,
    body: String,
    salience: u16,
) -> Value {
    json!({"id":id,"authority":authority,"role":role,"body":body,"salience":salience})
}

pub(super) async fn fetch_operator_card(state: &AppState, room: &str) -> Vec<Value> {
    let Some(reader) = state.config.operator.as_deref() else {
        return Vec::new();
    };
    let Some(pool) = state.hallway_pool.as_ref() else {
        tracing::warn!(room, "Presence operator card unavailable: no database pool");
        return Vec::new();
    };

    match operator_card_rows(pool, room, reader, (CARD_MAX_ROWS + 1) as i64).await {
        Ok(rows) => operator_card(rows),
        Err(error) => {
            tracing::warn!(%error, room, "Presence operator card unavailable");
            Vec::new()
        }
    }
}

/// One card line: name, kind, and the start of the summary. `canon_read` returns the complete row.
fn operator_card_line(row: &OperatorCardRow) -> String {
    let mut characters = row
        .name
        .chars()
        .chain(" (".chars())
        .chain(row.kind.chars())
        .chain("): ".chars())
        .chain(row.summary.chars())
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        });
    let mut line: String = characters.by_ref().take(CARD_ROW_MAX_CHARS).collect();

    if characters.next().is_some() {
        line.pop();
        line.push('…');
    }

    line
}

/// Standing canon index. Presence refuses a frame over its limits, so the card arrives inside
/// them: at most `CARD_MAX_ROWS` rows and `CARD_MAX_CHARS` characters, the cut mark included.
pub(super) fn operator_card(rows: Vec<OperatorCardRow>) -> Vec<Value> {
    let mut card = Vec::with_capacity(rows.len().min(CARD_MAX_ROWS));
    let mut remaining = CARD_MAX_CHARS;
    let mut cut = rows.len() > CARD_MAX_ROWS;

    for (index, row) in rows.iter().take(CARD_MAX_ROWS).enumerate() {
        let line = operator_card_line(row);
        let line_chars = line.chars().count();

        if line_chars > remaining {
            cut = true;
            break;
        }

        remaining -= line_chars;
        card.push(material(
            format!("operator-card:{}", row.entity_id),
            json!({"kind":"canon","entity_id":row.entity_id.to_string()}),
            "continuity",
            line,
            960 - index as u16,
        ));
    }

    if !cut {
        return card;
    }

    let mark_chars = CARD_CUT_MARK.chars().count();

    while remaining < mark_chars {
        let Some(dropped) = card.pop() else {
            return card;
        };

        remaining += dropped["body"]
            .as_str()
            .map_or(0, |body| body.chars().count());
    }

    if let Some(Value::String(body)) = card.last_mut().and_then(|last| last.get_mut("body")) {
        body.push_str(CARD_CUT_MARK);
    }

    card
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
    // Active spirit identity comes from authenticated room state, not retrieved canon.
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
                "continuity",
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

#[cfg(test)]
mod tests {
    use super::*;
    use summoning::presence::{PresenceAuthority, PresenceMaterial, PresenceMaterialRole};

    fn card_row(entity_id: i64, summary: String) -> OperatorCardRow {
        OperatorCardRow {
            entity_id,
            name: "role".into(),
            kind: "role".into(),
            summary,
        }
    }

    fn typed_card(rows: Vec<OperatorCardRow>) -> Vec<PresenceMaterial> {
        operator_card(rows)
            .into_iter()
            .map(|value| {
                serde_json::from_value(value).expect("card material matches Presence wire shape")
            })
            .collect()
    }

    #[test]
    fn retrieved_canon_keeps_authority_without_becoming_spirit_identity() {
        for kind in ["project", "person", "spirit"] {
            let presentation = json!({
                "canonMatches": [{
                    "id": "entity-42",
                    "kind": kind,
                    "name": "Example",
                    "summary": "An authoritative topical fact."
                }]
            });
            let materials = recalled(&presentation);

            assert_eq!(materials.len(), 1);
            let material: PresenceMaterial = serde_json::from_value(materials[0].clone()).unwrap();

            assert_eq!(material.id, "canon:entity-42");
            assert_eq!(
                material.authority,
                PresenceAuthority::Canon {
                    entity_id: "entity-42".into()
                }
            );
            assert_eq!(material.role, PresenceMaterialRole::Continuity);
            assert_eq!(material.body, "An authoritative topical fact.");
        }
    }

    #[test]
    fn canon_identity_requires_a_record_id_and_preserves_identifier_aliases() {
        for identifier in [
            json!({"id": "entity-42"}),
            json!({"entity_id": "entity-42"}),
            json!({"entityId": "entity-42"}),
            json!({"id": 42}),
        ] {
            let mut candidate = identifier;
            candidate["summary"] = json!("An authoritative topical fact.");
            let materials = recalled(&json!({"canonMatches": [candidate.clone()]}));
            let expected_id = if candidate["id"] == 42 {
                "42"
            } else {
                "entity-42"
            };

            assert_eq!(materials.len(), 1);
            assert_eq!(materials[0]["id"], format!("canon:{expected_id}"));
            assert_eq!(materials[0]["authority"]["entity_id"], expected_id);
            assert_eq!(materials[0]["role"], "continuity");
        }

        assert!(
            recalled(&json!({
                "canonMatches": [{"name": "Example", "summary": "No record identity."}]
            }))
            .is_empty()
        );
    }

    #[test]
    fn operator_card_marks_41_rows_but_not_40() {
        for count in [40, 41] {
            let card = typed_card(
                (1..=count)
                    .map(|id| card_row(id, "Brief fact".into()))
                    .collect(),
            );

            assert_eq!(
                card.len(),
                40,
                "the row cap must neither leak nor drop row 40"
            );
            assert_eq!(card[0].body, "role (role): Brief fact");
            assert_eq!(card[39].id, "operator-card:40");
            assert_eq!(
                card.iter()
                    .filter(|material| material.body.contains("[operator card cut:"))
                    .count(),
                usize::from(count == 41),
                "only an omitted row earns a cut mark"
            );

            if count == 41 {
                assert!(
                    card[39]
                        .body
                        .lines()
                        .last()
                        .unwrap()
                        .starts_with("[operator card cut:")
                );
            }
        }
    }

    #[test]
    fn operator_card_reserves_the_cut_mark_inside_4096_characters() {
        // 21 lines fit in 4095 characters; the mark must evict the twenty-first.
        let card = typed_card((1..=40).map(|id| card_row(id, "𝔽".repeat(182))).collect());
        let characters: usize = card
            .iter()
            .map(|material| material.body.chars().count())
            .sum();

        assert_eq!(
            card.len(),
            20,
            "a byte budget or an unreserved mark loses the boundary"
        );
        assert!(
            characters <= 4096,
            "the complete card costs {characters} characters, mark included"
        );

        for (index, material) in card.iter().enumerate() {
            assert_eq!(material.id, format!("operator-card:{}", index + 1));
            assert_eq!(material.body.lines().next().unwrap().chars().count(), 195);
        }

        assert_eq!(
            card.iter()
                .filter(|material| material.body.contains("[operator card cut:"))
                .count(),
            1
        );
        assert!(
            card.last()
                .unwrap()
                .body
                .lines()
                .last()
                .unwrap()
                .starts_with("[operator card cut:")
        );
    }

    #[test]
    fn operator_card_accepts_exactly_4096_characters_and_marks_one_over() {
        for extra in [0, 1] {
            let mut rows = (1..=20)
                .map(|id| card_row(id, "x".repeat(187)))
                .collect::<Vec<_>>();
            rows.push(card_row(21, "x".repeat(83 + extra)));
            let card = typed_card(rows);

            if extra == 0 {
                assert_eq!(card.len(), 21, "the exact character ceiling is admissible");
                assert_eq!(
                    card.iter()
                        .map(|material| material.body.chars().count())
                        .sum::<usize>(),
                    4096
                );
                assert!(
                    card.iter()
                        .all(|material| !material.body.contains("[operator card cut:"))
                );
            } else {
                assert_eq!(card.len(), 20);
                assert!(
                    card[19].body.contains("[operator card cut:"),
                    "one character over must be visible as a cut"
                );
                assert!(
                    card.iter()
                        .map(|material| material.body.chars().count())
                        .sum::<usize>()
                        <= 4096
                );
            }
        }
    }

    #[test]
    fn operator_card_clips_only_after_200_characters() {
        for extra in [0, 1] {
            let card = typed_card(vec![card_row(42, "x".repeat(187 + extra))]);
            let body = &card[0].body;

            assert_eq!(body.chars().count(), 200);
            assert_eq!(body.lines().count(), 1);
            assert_eq!(
                body.ends_with('…'),
                extra == 1,
                "only a clipped row ends in an ellipsis"
            );
            assert!(body.starts_with("role (role): "));
        }
    }

    #[test]
    fn operator_card_flattens_summary_newlines_without_losing_words() {
        let card = typed_card(vec![card_row(
            42,
            "First\nsecond\r\nthird\tfourth\u{2028}fifth".into(),
        )]);
        let body = &card[0].body;

        assert_eq!(body.lines().count(), 1);
        assert_eq!(
            body.split_whitespace().collect::<Vec<_>>(),
            [
                "role", "(role):", "First", "second", "third", "fourth", "fifth"
            ]
        );
    }

    #[test]
    fn operator_card_clips_unicode_by_characters_not_bytes_or_utf16_units() {
        for extra in [0, 1] {
            let card = typed_card(vec![card_row(42, "𝔽".repeat(187 + extra))]);
            let expected = if extra == 0 {
                format!("role (role): {}", "𝔽".repeat(187))
            } else {
                format!("role (role): {}…", "𝔽".repeat(186))
            };

            assert_eq!(card[0].body, expected);
            assert_eq!(card[0].body.chars().count(), 200);
        }
    }

    #[test]
    fn operator_card_materials_keep_canon_authority_in_a_distinct_id_namespace() {
        let ids = [1, 42, i64::MAX];
        let card = typed_card(
            ids.into_iter()
                .map(|id| card_row(id, "Brief fact".into()))
                .collect(),
        );

        assert_eq!(card.len(), ids.len());

        for (material, id) in card.iter().zip(ids) {
            assert_eq!(material.id, format!("operator-card:{id}"));
            assert!(
                !material.id.starts_with("canon:"),
                "recall reserves the canon: namespace"
            );
            assert_eq!(
                material.authority,
                PresenceAuthority::Canon {
                    entity_id: id.to_string()
                }
            );
            assert_eq!(material.role, PresenceMaterialRole::Continuity);
        }
    }

    #[test]
    fn operator_card_with_no_rows_emits_neither_material_nor_mark() {
        assert!(typed_card(Vec::new()).is_empty());
    }
}
