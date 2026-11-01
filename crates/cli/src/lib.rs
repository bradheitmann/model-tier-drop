//! The CLI's public library surface.
//!
//! Integration tests of this package cannot reach the modules of the `tier-drop` binary target, so
//! the parts they exercise live here as well: `main.rs` stays the command line, and `atlas` is the
//! pure mapping from a pinned model-atlas export to tier-drop's registry facts.

pub mod atlas;
