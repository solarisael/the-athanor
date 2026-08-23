//! Origami — the House message-shape family.
//!
//! Origami names every mechanic that folds a record and moves it through
//! the House. PostgreSQL is authoritative for every body. NATS carries
//! delivery only. A shape never becomes a second authority.
//!
//! Current shapes:
//! - Boats ([`boats`]): stasis in the Sea, with a return point. A spirit
//!   sleeps; the currents return the boat when the spirit is called.
//! - Cranes: movement in the Sea, with a destination point. A message
//!   flies from one spirit to another spirit or to the operator. The
//!   crane runtime lives in `house-delivery`; its shared vocabulary
//!   moves here as the ontology work (goal A) extracts it.
//!
//! Hallways and project routing depend on the same NATS spine. They join
//! this family as the extraction continues.
//!
//! This crate owns the shared vocabulary that the write side
//! (`house-substrate`) and the flight side (`house-delivery`) both obey.
//! One declaration, two consumers, no duplicated literals.

pub mod boats;
