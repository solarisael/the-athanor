use super::*;
use crate::lesson::registry::context::intersects;
use crate::lesson::registry::mutation::update::patch_trigger_spec;
use std::collections::BTreeSet;

#[test]
fn trigger_match_params_refuse_a_shapeless_request() {
    let params: LessonTriggerMatchParams = serde_json::from_value(serde_json::json!({
        "room": "kodo",
        "session": "s-1",
        "surfaces": [{"kind": "tool", "tool": "edit", "path": "a.rs", "text": "x"}]
    }))
    .unwrap();
    assert!(params.validate().is_ok());
    let empty: LessonTriggerMatchParams = serde_json::from_value(serde_json::json!({
        "room": "kodo", "session": "s-1", "surfaces": []
    }))
    .unwrap();
    assert!(empty.validate().is_err());
    let blank_session: LessonTriggerMatchParams = serde_json::from_value(serde_json::json!({
        "room": "kodo",
        "session": "  ",
        "surfaces": [{"kind": "prose", "text": "x"}]
    }))
    .unwrap();
    assert!(blank_session.validate().is_err());
    let shouting: LessonTriggerMatchParams = serde_json::from_value(serde_json::json!({
        "room": "Kodo",
        "session": "s-1",
        "surfaces": [{"kind": "prose", "text": "x"}]
    }))
    .unwrap();
    assert!(shouting.validate().is_err());
}

#[test]
fn a_trigger_patch_is_judged_before_any_row_is_locked() {
    let spec = patch_trigger_spec(
        serde_json::json!({"condition": ["\\bunwrap\\(\\)"], "interruptMode": "remind"})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(spec.condition, vec!["\\bunwrap\\(\\)".to_owned()]);
    assert_eq!(spec.validate_fields(), Ok(()));
    assert_eq!(validate_patterns(&spec), Ok(()));

    let broken = patch_trigger_spec(
        serde_json::json!({"condition": ["unwrap("]})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(broken.validate_fields(), Ok(()), "shape is fine");
    assert!(validate_patterns(&broken).is_err(), "the regex is not");

    assert!(
        patch_trigger_spec(
            serde_json::json!({"condition": "unwrap"})
                .as_object()
                .unwrap()
        )
        .is_err(),
        "a bare string is not a condition array"
    );
    assert!(
        patch_trigger_spec(
            serde_json::json!({"repeatCooldownSecs": "600"})
                .as_object()
                .unwrap()
        )
        .is_err()
    );
    // A patch that clears the policy columns is legal on its own: the
    // stored patterns still carry the lesson.
    let cleared = patch_trigger_spec(
        serde_json::json!({"interruptMode": null, "repeatCooldownSecs": null})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert!(cleared.is_empty());
    assert_eq!(cleared.validate_fields(), Ok(()));
}

#[test]
fn lesson_query_preserves_typed_filters_and_bounds() {
    let params: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kintsu",
        "type": "audio",
        "stage": "mix",
        "languageKeys": [],
        "technologyKeys": [],
        "limit": 12
    }))
    .unwrap();
    assert_eq!(params.family, LessonFamily::Audio);
    assert_eq!(params.stage.as_deref(), Some("mix"));
    assert!(params.validate().is_ok());
    let invalid: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kintsu", "type": "project", "limit": 12
    }))
    .unwrap();
    assert!(invalid.validate().is_err());
}

#[test]
fn context_eligibility_requires_declared_axis_overlap() {
    let rust = BTreeSet::from([String::from("rust")]);
    assert!(intersects(&[], &rust));
    assert!(intersects(&[String::from("rust")], &rust));
    assert!(!intersects(&[String::from("python")], &rust));
    assert!(!intersects(&[String::from("rust")], &BTreeSet::new()));
}

#[test]
fn bigint_guards_accept_decimal_strings_without_javascript_precision_loss() {
    let delete: LessonDeleteParams = serde_json::from_value(serde_json::json!({
        "kind": "coding-lesson",
        "id": "9223372036854775807",
        "expectedTitle": "Exact"
    }))
    .unwrap();
    assert_eq!(delete.id, i64::MAX);
    let design: DesignDocumentWriteParams = serde_json::from_value(serde_json::json!({
        "system": "solarisael",
        "docType": "token",
        "name": "color.accent",
        "supersedes": "42"
    }))
    .unwrap();
    assert_eq!(design.supersedes, Some(42));
}

#[test]
fn mutation_receipts_keep_exact_family_identity() {
    let receipt = LessonMutationReceipt::Updated {
        kind: "design-lesson".into(),
        id: 9,
        title: "Keyboard floor".into(),
        always_on: false,
        project: None,
    };
    assert_eq!(
        serde_json::to_value(receipt).unwrap(),
        serde_json::json!({
            "ok": true,
            "kind": "design-lesson",
            "id": 9,
            "title": "Keyboard floor",
            "updated": true,
            "alwaysOn": false,
            "project": null
        })
    );
}
