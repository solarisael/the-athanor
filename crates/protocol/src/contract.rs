// Cross-crate pins: the numbers and paths more than one crate must agree on.
//
// This file is the one clean door for these knobs, and it is deliberately
// plain: `std` only, ASCII literals, no `use`, no derives, and line comments
// rather than `//!` module docs, because `athanor-install` pulls it in with
// `include!` and an inner attribute cannot survive a macro expansion. It reads
// the file instead of depending on `protocol` because that dependency measures
// at 27 extra crates -- hearth, ast-grep, and four tree-sitter grammars with
// their C build -- to deliver a port, a path, and a schema number to an
// installer whose whole graph is otherwise tiny. Anything added here compiles
// inside the installer too, so keep it plain or it stops being includable.
//
// Every knob is declared exactly once as a literal inside a macro, and each
// other spelling is assembled from it. That is the point: a port that appears
// once cannot be retyped somewhere and drift.

/// The head of `substrate/migrations/`: the schema version this source tree
/// ships, and therefore the only one its binaries know.
///
/// enough: a hand-typed head is honest only while someone bumps it, and twice
/// nobody did -- `origami` froze at 17 the day it added
/// `0017_crane_delivery.sql`, `athanor-install` froze at 18 the day it added
/// `0018_hallway_chatrooms.sql`, and migrations ran on to 0025 with both
/// numbers still claiming to be current. A new
/// `substrate/migrations/NNNN_*.sql` must bump this line together with
/// `installer/dependencies.json` `schemaVersion`, which the release build
/// stamps into the manifest that `athanor_install::manifest::REQUIRED_SCHEMA`
/// checks. The ceiling: derive this from the migration registry the way
/// `akasha::migrations::consolidated_version_labels` already derives the backup
/// allowlist, once a crate that both readers may depend on owns that registry.
pub const SUBSTRATE_SCHEMA_VERSION: u32 = 28;

macro_rules! loopback_host_literal {
    () => {
        "127.0.0.1"
    };
}
macro_rules! default_host_ws_port_literal {
    () => {
        8787
    };
}
macro_rules! default_host_ws_path_literal {
    () => {
        "/athanor/v1/ws"
    };
}

/// The only address a House binds or dials.
///
/// enough: the Athanor is a single-operator House and every runtime endpoint is
/// loopback by contract rather than by default -- `athanor_install::supervisor`
/// refuses to plan a process on any other address. Widening this is a security
/// decision, not a settings row.
pub const LOOPBACK_HOST: &str = loopback_host_literal!();

/// Default Host WebSocket port. Real room ports are per-room and come from the
/// House configuration; this is the fallback for an unconfigured default room.
pub const DEFAULT_HOST_WS_PORT: u16 = default_host_ws_port_literal!();

/// The Host WebSocket route. The installer writes it into the OMP client
/// projection and `host::config` binds it, so changing it is a migration for
/// every installed operator, never a rename.
pub const DEFAULT_HOST_WS_PATH: &str = default_host_ws_path_literal!();

/// The default-room WebSocket endpoint, assembled from the three knobs above so
/// it cannot disagree with them.
pub const DEFAULT_HOST_WS_URL: &str = concat!(
    "ws://",
    loopback_host_literal!(),
    ":",
    default_host_ws_port_literal!(),
    default_host_ws_path_literal!()
);
