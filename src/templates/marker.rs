//! pm-managed marker block in `.gitignore`.
//!
//! Marker text is **fixed** per design D4 / Resolved Decision 1. Changes
//! require a new BREAKING change because in-the-wild `.gitignore` files
//! depend on these exact strings.
//!
//! Stage 2 fills in the parse / merge logic.

/// Opening marker line. Must match the exact text on disk to be detected.
pub const BEGIN_MARKER: &str =
    "# >>> pm managed (do not edit; run `pm project gitignore` to refresh) >>>";

/// Closing marker line.
pub const END_MARKER: &str = "# <<< pm managed <<<";
