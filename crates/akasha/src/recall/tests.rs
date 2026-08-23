use super::pointer_files::protocol_pointer_files;
use super::semantic_vocabulary::{
    SEMANTIC_VOCABULARY_MAX_TERMS, SEMANTIC_VOCABULARY_TOP_K, semantic_vocabulary_terms,
};
use super::temporal::giga_temporal_factor;
use crate::settings::RoomSettings;
use chrono::{Duration, TimeZone, Utc};
use serde_json::{Value, json};

fn giga_meta(durability: Value, anchor: &str) -> Value {
    json!({
        "origin": "giga-promotion",
        "giga": {
            "durability": durability,
            "decay_anchor": "candidate_created_at",
            "decay_anchor_at": anchor,
        },
    })
}

#[test]
fn pointer_files_normalize_to_the_recall_wire_contract() {
    let stored = json!([
        "memory/2026-05-01_plain_path.md",
        { "file": "canon/source.md", "lines": [4, 9], "note": "legacy annotation" },
        { "file": "canon/no_lines.md" },
        { "file": "   " },
        { "lines": [1, 2] },
        42,
    ]);
    let normalized = protocol_pointer_files(&stored);
    assert_eq!(
        normalized,
        json!([
            { "file": "memory/2026-05-01_plain_path.md" },
            { "file": "canon/source.md", "lines": [4, 9] },
            { "file": "canon/no_lines.md" },
        ])
    );
    assert_eq!(protocol_pointer_files(&json!("junk")), json!([]));
}

#[test]
fn temporal_factor_treats_unsafe_metadata_as_legacy_weight() {
    let now = Utc.with_ymd_and_hms(2030, 2, 1, 0, 0, 0).single().unwrap();
    let settings = RoomSettings::default();
    let anchor = (now - Duration::days(7)).to_rfc3339();
    let cases = [
        json!({}),
        giga_meta(json!("not-a-number"), &anchor),
        json!({
            "origin": "manual",
            "giga": {
                "durability": 0.0,
                "decay_anchor": "candidate_created_at",
                "decay_anchor_at": anchor,
            },
        }),
    ];

    for meta in cases {
        assert_eq!(giga_temporal_factor(&meta, now, &settings), (None, 1.0));
    }
}

#[test]
fn temporal_factor_follows_the_durability_shaped_half_life() {
    let now = Utc.with_ymd_and_hms(2030, 2, 1, 0, 0, 0).single().unwrap();
    let settings = RoomSettings::default();
    let seven_days_ago = (now - Duration::days(7)).to_rfc3339();
    let twenty_eight_days_ago = (now - Duration::days(28)).to_rfc3339();

    let (durability_zero, factor_zero) =
        giga_temporal_factor(&giga_meta(json!(0.0), &seven_days_ago), now, &settings);
    assert_eq!(durability_zero, Some(0.0));
    assert!((factor_zero - 0.5).abs() < 1e-12);

    let (durability_half, factor_half) = giga_temporal_factor(
        &giga_meta(json!(0.5), &twenty_eight_days_ago),
        now,
        &settings,
    );
    assert_eq!(durability_half, Some(0.5));
    assert!((factor_half - 0.5).abs() < 1e-12);
}

#[test]
fn temporal_factor_never_decays_permanent_or_future_memories() {
    let now = Utc.with_ymd_and_hms(2030, 2, 1, 0, 0, 0).single().unwrap();
    let settings = RoomSettings::default();
    let old_anchor = (now - Duration::days(365)).to_rfc3339();
    let future_anchor = (now + Duration::days(1)).to_rfc3339();

    assert_eq!(
        giga_temporal_factor(&giga_meta(json!(1.0), &old_anchor), now, &settings),
        (Some(1.0), 1.0)
    );
    assert_eq!(
        giga_temporal_factor(&giga_meta(json!(0.0), &future_anchor), now, &settings),
        (Some(0.0), 1.0)
    );
}
#[test]
fn semantic_vocabulary_terms_are_deduplicated_and_hard_capped() {
    assert_eq!(SEMANTIC_VOCABULARY_TOP_K, 3);
    let concepts = (0..4)
        .map(|concept| {
            json!({
                "concept": format!("concept-{concept}"),
                "terms": (0..4).map(|term| format!("term-{concept}-{term}")).collect::<Vec<_>>(),
                "source_kind": "named_entity",
                "similarity": 0.5,
            })
        })
        .collect::<Vec<_>>();
    let terms = semantic_vocabulary_terms(&concepts);
    assert_eq!(terms.len(), SEMANTIC_VOCABULARY_MAX_TERMS);
    assert_eq!(terms.first().map(String::as_str), Some("term-0-0"));
    assert_eq!(terms.last().map(String::as_str), Some("term-2-3"));
}
