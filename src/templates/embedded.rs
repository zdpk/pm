//! Compile-time-embedded gitignore template constants.
//!
//! This file pulls in the auto-generated module written by
//! [`build.rs`](../../../build.rs). The generated content lives in
//! `OUT_DIR/embedded_gitignore.rs` and exposes:
//!
//! - One `pub static <KEY>: &str` per category
//! - `ALL_CATEGORIES: &[(&str, &str)]` aggregate
//!
//! Everything originates from `vendor/github-gitignore/` (CC0). See
//! `LICENSES/github-gitignore-CC0.txt` for the upstream license.

include!(concat!(env!("OUT_DIR"), "/embedded_gitignore.rs"));
