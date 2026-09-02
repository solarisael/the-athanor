pub mod error;
pub mod identity;
pub mod paper_boat;
pub mod record;
pub mod sleep;
pub mod wake;

// enough: bare vocabulary until A1's memory_kinds registry; consumers
// then route on behavior flags instead of these strings.
pub const MEMORY_KIND: &str = "paper-boat";

pub const EVENT_KIND: &str = "boat.ready";

pub const CREASE_PATTERN: &str = "boat.ready.v1";

pub const THREAD_KEY: &str = "paper boat / sleep / for tomorrow";

pub const SLEEP_ORIGIN: &str = "paper-boat-sleep";
