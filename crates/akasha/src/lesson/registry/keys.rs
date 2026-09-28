//! Language and technology keys match through their family. A lesson keyed
//! `bend-2` stays keyed `bend-2`, because Bend 1 and Bend 2 are different
//! languages. Storage keeps the version; only comparison reads it:
//! - `bend` reaches `bend`, `bend-1`, and `bend-2`;
//! - `bend-2` reaches `bend-2` and the version-free `bend`, never `bend-1`.

use std::collections::BTreeSet;

/// The SQL twin of `key_family`, applied to one stored key named `k`.
/// `lesson_query_key_families_match_key_scope` holds it to the Rust cases.
pub(crate) const KEY_FAMILY_SQL: &str = "regexp_replace(lower(k), '(-[0-9]+)+$', '')";

/// Trailing all-digit segments are a version: `bend-2` and `godot-4` fold to
/// `bend` and `godot`; `github-actions` and `x86` stay as they are.
pub(crate) fn key_family(key: &str) -> String {
    let mut family = key.trim().to_lowercase();
    while let Some((name, version)) = family.rsplit_once('-')
        && !name.is_empty()
        && !version.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
    {
        family.truncate(name.len());
    }
    family
}

/// What one request asks for on one axis (language or technology).
#[derive(Debug, Default)]
pub(crate) struct KeyScope {
    /// The requested keys as written.
    pub(crate) requested: BTreeSet<String>,
    /// Stored keys that pass as they are: every requested key and its family.
    pub(crate) accepted: BTreeSet<String>,
}

impl KeyScope {
    pub(crate) fn new(keys: &[String]) -> Self {
        let mut scope = Self::default();
        for key in keys {
            let key = key.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            scope.accepted.insert(key_family(&key));
            scope.accepted.insert(key.clone());
            scope.requested.insert(key);
        }
        scope
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.requested.is_empty()
    }

    /// An unkeyed lesson is eligible everywhere. A keyed lesson needs one key
    /// that is accepted as written, or whose family is a requested key.
    pub(crate) fn admits(&self, stored: &[String]) -> bool {
        stored.is_empty()
            || stored.iter().any(|key| {
                self.accepted.contains(&key.trim().to_lowercase())
                    || self.requested.contains(&key_family(key))
            })
    }
}
