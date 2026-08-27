use sha2::{Digest, Sha256};

pub(super) fn hp(h: &mut Sha256, n: &str, v: &str) {
    h.update((n.len() as u64).to_be_bytes());
    h.update(n);
    h.update((v.len() as u64).to_be_bytes());
    h.update(v)
}
pub(super) fn hs(domain: &str) -> Sha256 {
    let mut h = Sha256::new();
    hp(&mut h, "domain", domain);
    h
}
pub(super) fn hf(h: Sha256) -> String {
    format!("{:x}", h.finalize())
}
pub(super) fn ho(h: &mut Sha256, n: &str, v: Option<&str>) {
    hp(h, n, v.unwrap_or("<absent>"))
}
