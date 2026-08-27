use crate::canon::{
    CanonAttribution, CanonAuthority, CanonPointer, CanonReadRequest, CanonWriteRequest,
};
use crate::remember::RememberKind;

#[test]
fn canon_is_a_distinct_typed_store_with_strict_selectors() {
    assert!(RememberKind::parse("canon").is_err());
    assert_eq!(
        CanonAuthority::parse("active").unwrap(),
        CanonAuthority::Active
    );
    assert!(CanonAuthority::parse("current").is_err());
    let attribution = CanonAttribution::new("Kintsu".into(), "omp:Sol:call-1".into()).unwrap();
    let pointer = CanonPointer::new("canon/source.md".into(), Some((4, 9))).unwrap();
    let write = CanonWriteRequest::new(
        "house".into(),
        "The Athanor".into(),
        "project".into(),
        "PostgreSQL-authoritative canon".into(),
        vec!["Athanor".into()],
        None,
        true,
        vec![pointer],
        Some("2026-08-10".into()),
        vec![7, 7],
        attribution,
    )
    .unwrap();
    assert_eq!(write.supersedes(), &[7]);
    assert!(CanonReadRequest::new("house".into(), Some(7), None, true).is_ok());
    assert!(
        CanonReadRequest::new("house".into(), Some(7), Some("The Athanor".into()), true,)
            .is_err()
    );
}
