//! `.gitignore` content synthesis.
//!
//! Stage 2 fills in the actual composition / dedup logic. Stage 1 wires
//! the module path so consumers (`commands/project.rs::cmd_gitignore`,
//! the new init/sync paths) can refer to `templates::synthesize::*` from
//! the start.

#![allow(dead_code)] // wired up in Stage 2

use crate::templates::Category;

/// Placeholder signature for the synthesis function. The body is supplied
/// in Stage 2 (Group 4 of the tasks list).
pub fn synthesize_managed_body(_categories: &[Category], _framework_extra: Option<&str>) -> String {
    String::new()
}
