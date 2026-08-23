//! Hallways ride PostgreSQL + Host projections; no NATS lane exists
//! behind them (census 2026-08-23) — that transport is its own quest.
//! Known gaps, carried not fixed: health.rs never checks hallway
//! tables; stdio mounts 7 of 9 methods (knocks::claim/settle are
//! house-host only).

pub mod bells;
pub mod channels;
pub mod errors;
pub mod knocks;
pub mod messages;

pub use errors::HallwayError;
