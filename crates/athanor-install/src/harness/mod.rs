//! Harness ownership for the canonical Athanor app.
//!
//! The GUI holds no process authority: it asks over the loopback wire in
//! `house_protocol::harness`, and this owner keeps every child handle. Keep this
//! spawn path away from `supervisor.rs`; that one runs headless SCM service
//! children with null handles, these are consoles an operator looks at.
//!
//! This folder keeps each harness concern in one file. `config` reads the
//! registry, `owner` keeps generic ownership, `omp` hosts the keeper, and
//! `control` carries requests over loopback.

mod config;
mod control;
mod omp;
mod owner;

pub use config::{
    ConsoleMode, HARNESS_REGISTRY_FORMAT, HarnessDriver, HarnessEntry, HarnessKind, HarnessLaunch,
    HarnessRegistry, HarnessRegistryFile, HarnessSpec, REGISTRY_ENV, control_token, registry_path,
};
pub use control::{CONTROL_ADDR_ENV, CONTROL_TOKEN_ENV, ControlServer};
pub use owner::{HarnessOwner, STOP_TIMEOUT};
