//! Hallways — House letters between rooms. Channels, presences,
//! sequenced messages, Bells, and Knocks.
//!
//! Transport honesty (census 2026-08-23, confirmed by full-file walk):
//! hallway writes land in PostgreSQL and reach readers through Host
//! projections. No NATS lane, outbox row, or spine event exists behind
//! hallways today. Zero SQL triggers or functions in the whole family;
//! every invariant beyond a column constraint lives in Rust — and now
//! it lives here. Hallways join Origami as a shape now; a NATS
//! transport is a later, explicit quest.
//!
//! One concern per file, each with its own door:
//! - [`channels`]: the door itself and who is standing in it.
//! - [`messages`]: sequenced letters, the cursor, the room inbox.
//! - [`bells`]: durable targeted attention rows, minted by a post and
//!   quieted only by a covering read.
//! - [`knocks`]: bounded-turn wake requests, never commands.
//! - [`errors`]: what a call returns when it is not a receipt.
//!
//! Census warnings carried by this extraction and deliberately NOT
//! fixed by it — each is its own quest:
//! - `house-substrate/src/health.rs` checks the crane trio and never
//!   touches a hallway table. A hallway family that is completely
//!   broken still reports healthy. Nothing here changes that: do not
//!   read a green health line as evidence about hallways.
//! - The substrate stdio protocol exposes 7 of the 9 hallway methods.
//!   [`knocks::claim`] and [`knocks::settle`] are reachable only
//!   through house-host. Unreachable from stdio is not unused: the
//!   Knock lifecycle is incomplete without them, and they are held to
//!   exactly the rules the seven reachable doors are held to.
//!
//! `house-substrate/src/hallway.rs` is now a thin adapter over this
//! module: it hands `Config` values in and maps [`HallwayError`] back
//! to the substrate's `AppError`.

pub mod bells;
pub mod channels;
pub mod errors;
pub mod knocks;
pub mod messages;

pub use errors::HallwayError;
