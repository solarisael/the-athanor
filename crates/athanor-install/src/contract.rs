//! The installer's door onto the House's cross-crate pins.
//!
//! `crates/protocol/src/contract.rs` is the single declaration of these knobs;
//! this module includes that file verbatim rather than restating any of it, so
//! there is still exactly one place a schema version or a WebSocket path is
//! written down.
//!
//! Why an include and not a dependency: `protocol` measures at 27 additional
//! crates for this installer -- `hearth`, ast-grep, and four tree-sitter
//! grammars with their C build -- which is a heavy price for three constants in
//! a binary whose graph is otherwise small and pure Rust. The include costs no
//! dependency and cannot drift, since it is literally the same source text. The
//! price it does carry is that the shared fragment must stay plain; its own
//! header says so.

// The fragment declares every pin; this crate reads three of them today and the
// rest come along with the file.
#![allow(dead_code)]

include!("../../protocol/src/contract.rs");
