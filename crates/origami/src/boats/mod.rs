
// enough: bare string constants; quest A1 moves this vocabulary into the
// memory_kinds registry with behavior flags, and consumers route on flags.

pub mod error;
pub mod identity;
pub mod record;
pub mod sleep;
pub mod wake;

pub const MEMORY_KIND: &str = "paper-boat";

pub const EVENT_KIND: &str = "boat.ready";

pub const CREASE_PATTERN: &str = "boat.ready.v1";

pub const THREAD_KEY: &str = "paper boat / sleep / for tomorrow";

pub const SLEEP_ORIGIN: &str = "paper-boat-sleep";
