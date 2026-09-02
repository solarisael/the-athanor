use sha2::{Digest, Sha256};

pub fn payload_digest(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

/// Length-prefixed per part — ["ab","c"] and ["a","bc"] must not
/// collide, or a reused idempotency key with different content reads as
/// a duplicate success.
pub fn idempotency_digest(parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub fn subject_owns(subject: &str, filter: &str) -> bool {
    match filter.strip_suffix(".>") {
        Some(prefix) => subject.len() > prefix.len() + 1 && subject.starts_with(prefix),
        None => subject == filter,
    }
}

#[cfg(test)]
mod tests {
    use super::idempotency_digest;

    #[test]
    // these hex values sit in the live hallway tables; computed with
    // python hashlib, not by running the function under test
    fn digests_stay_byte_identical_to_the_rows_already_persisted() {
        assert_eq!(
            idempotency_digest(&["family-hallway", "kodo", "Kodo", "session-1"]),
            "0f9ad9a3caec4ebd10afe3c15dab4718663afd3ae9482fab17c2bec9a3c621fe"
        );
        assert_eq!(
            idempotency_digest(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parts_cannot_be_rearranged_into_the_same_digest() {
        assert_eq!(
            idempotency_digest(&["ab", "c"]),
            "601d5476e2ccfe2c87a2bba7a322659734a05749d5b5aa781f513e4912db0d5f"
        );
        assert_eq!(
            idempotency_digest(&["a", "bc"]),
            "3fafa1cf2f19a7c1129beb20cf0983f73a489a221fc0dd2f16d1be292d089205"
        );
        assert_ne!(
            idempotency_digest(&["ab", "c"]),
            idempotency_digest(&["a", "bc"])
        );
    }
}
