//! Hallways — House letters between rooms. Channels, presences,
//! sequenced messages, Bells, and Knocks.
//!
//! Transport honesty (census 2026-08-23, confirmed by full-file walk):
//! hallway writes land in PostgreSQL and reach readers through Host
//! projections. No NATS lane, outbox row, or spine event exists behind
//! hallways today. Zero SQL triggers or functions in the whole family;
//! every invariant beyond constraints lives in Rust
//! (house-substrate/src/hallway.rs). Hallways join Origami as a shape
//! now; a NATS transport is a later, explicit quest.
//!
//! Known census gaps to carry into extraction: health.rs checks the
//! crane trio and never the hallway tables; the substrate stdio
//! protocol exposes 7 of 9 hallway methods (knock claim/settle reach
//! only through house-host).

// enough: skeleton module; extraction moves hallway.rs (1,510 lines,
// one grab-bag file today) into these lean concern files.

pub mod bells;
pub mod channels;
pub mod knocks;
pub mod messages;
