//! Every address the installer writes into a House's runtime configuration.
//!
//! One home, because these were spelled by hand wherever they were needed: the
//! managed PostgreSQL DSN existed three times over (`native_runtime`, `service`,
//! and `installer` writing its parts), and the Host WebSocket URL was rebuilt
//! with a format string sitting a crate away from the `protocol` const that
//! already declared its path and port.

use crate::contract::{DEFAULT_HOST_WS_PATH, LOOPBACK_HOST};

/// The role and database the managed PostgreSQL is provisioned with.
///
/// enough: `substrate/deploy-local.ps1` and `0001_initial.sql` create exactly
/// this role and this database. Renaming either is a deploy-day migration with
/// a dump and restore, never a settings row, so the name stays a constant here
/// and the outward path is the provisioning script.
pub const MANAGED_DATABASE_USER: &str = "athanor";
pub const MANAGED_DATABASE_NAME: &str = "athanor";

/// Ports for the substrate this installer manages itself.
///
/// enough: the installer downloads, installs, and supervises these two servers,
/// so it chooses their ports rather than discovering them; `supervisor` already
/// refuses a House whose room port collides with either. An operator who wants
/// different ports wants an external database, which
/// `Secrets::external_database_url` already provides a door for.
pub const MANAGED_DATABASE_PORT: u16 = 5432;
pub const MANAGED_NATS_PORT: u16 = 4222;

/// The DSN for the managed PostgreSQL this installer provisioned.
///
/// Reached only when `external_database_url` is absent: an operator-supplied DSN
/// always wins, and this is the fallback for a House we installed ourselves.
pub fn managed_database_url(postgres_password: &str) -> String {
    format!(
        "postgresql://{MANAGED_DATABASE_USER}:{postgres_password}@{LOOPBACK_HOST}:{MANAGED_DATABASE_PORT}/{MANAGED_DATABASE_NAME}"
    )
}

/// The Host WebSocket endpoint for one room.
///
/// The port is per-room and comes from the House configuration; the scheme,
/// address, and path come from the pins, so this can never disagree with
/// `protocol::DEFAULT_HOST_WS_URL` about where a Host answers.
pub fn host_ws_url(room_port: u16) -> String {
    format!("ws://{LOOPBACK_HOST}:{room_port}{DEFAULT_HOST_WS_PATH}")
}
