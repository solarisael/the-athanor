//!
//! The digest domain separator is frozen: the boat memory kind, one NUL
//! byte, the room, one NUL byte, the body. Those bytes are the identity
//! of every boat already in the database. Changing them renames every
//! boat that exists, so they never change.

use sha2::{Digest, Sha256};

use super::MEMORY_KIND;

pub const SOURCE_PATH_PREFIX: &str = "db-only/paper-boats/sha256-";

pub const SOURCE_PATH_SUFFIX: &str = ".md";

pub const DIGEST_LABEL: &str = "sha256";

/// The frozen separator byte between the digest domains.
const DOMAIN_SEPARATOR: &[u8] = b"\0";

pub fn source_identity(room: &str, body: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(MEMORY_KIND.as_bytes());
    digest.update(DOMAIN_SEPARATOR);
    digest.update(room.as_bytes());
    digest.update(DOMAIN_SEPARATOR);
    digest.update(body.as_bytes());
    format!(
        "{SOURCE_PATH_PREFIX}{:x}{SOURCE_PATH_SUFFIX}",
        digest.finalize()
    )
}

/// Read the digest back out of a boat source path, when the path is one
/// this module minted. `None` means the path is not a boat identity.
pub fn digest_of(source_path: &str) -> Option<&str> {
    source_path
        .strip_prefix(SOURCE_PATH_PREFIX)
        .and_then(|value| value.strip_suffix(SOURCE_PATH_SUFFIX))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_is_deterministic_and_room_scoped() {
        let first = source_identity("kintsu", "same body");
        assert_eq!(first, source_identity("kintsu", "same body"));
        assert_ne!(first, source_identity("other-room", "same body"));
        assert_ne!(first, source_identity("kintsu", "different body"));
    }

    /// The separator bytes are a database-wide fact, not a style choice.
    /// These vectors were taken from `sha256sum` over the raw bytes
    /// outside this crate, so the guard fails if the domain ever drifts.
    #[test]
    fn identity_digest_domain_stays_byte_for_byte() {
        assert_eq!(
            source_identity("kintsu", "same body"),
            format!(
                "{SOURCE_PATH_PREFIX}\
                 e4861a0639fbd7f05ad16d1ef907fcddb69414d34f0f7b9a686de200d908cb0f\
                 {SOURCE_PATH_SUFFIX}"
            )
        );
        assert_eq!(
            source_identity("other-room", "same body"),
            format!(
                "{SOURCE_PATH_PREFIX}\
                 881da23f68b9ead6b3cd6999b16b0b71b7d0c00f7c6719d85f8f5953b32729f8\
                 {SOURCE_PATH_SUFFIX}"
            )
        );
    }
}
