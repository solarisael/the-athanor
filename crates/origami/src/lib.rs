//! Origami — the House message-shape family.
//!
//! Origami names every mechanic that folds a record and moves it through
//! the House. PostgreSQL is authoritative for every body. NATS carries
//! delivery only. A shape never becomes a second authority.
//!
//! Current shapes:
//! - Boats ([`boats`]): stasis in the Sea, with a return point. A spirit
//!   sleeps; the currents return the boat when the spirit is called.
//! - Cranes ([`cranes`]): movement in the Sea, with a destination point.
//!   A message flies to another spirit or to the operator.
//! - Hallways ([`hallways`]): House letters between rooms. The census of
//!   2026-08-23 found their transport is PostgreSQL plus Host
//!   projections; no NATS lane exists behind them today. They join the
//!   family as a shape now; transport unification is a later quest.
//!
//! [`sea`] holds the shared spine: digests, envelope hygiene, and
//! subject ownership rules every shape obeys.
//!
//! This crate owns the shared vocabulary and, as the A-series quests
//! land, the shape logic itself. Skeleton functions carry their absorb
//! target as a `file:line` from the six-nose census of 2026-08-23.

pub mod boats;
pub mod cranes;
pub mod hallways;
pub mod sea;
