use super::source::{GigaScope, GigaSourceRef, GigaSourceType, GigaVisibility};

pub(crate) fn giga_test_source(
    source_id: &str,
    hash_digit: char,
    scope: GigaScope,
) -> GigaSourceRef {
    GigaSourceRef::new(
        GigaSourceType::Turn,
        source_id.into(),
        "user".into(),
        "2026-07-24T12:00:00Z".into(),
        hash_digit.to_string().repeat(64),
        scope,
        None,
    )
    .unwrap()
}

pub(crate) fn giga_private_scope() -> GigaScope {
    GigaScope::new(Some("lab".into()), None, GigaVisibility::Private, false).unwrap()
}

pub(crate) fn giga_project_scope() -> GigaScope {
    GigaScope::new(
        Some("lab".into()),
        Some("athanor".into()),
        GigaVisibility::Private,
        true,
    )
    .unwrap()
}
