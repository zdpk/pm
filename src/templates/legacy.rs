//! Detection and removal of v0.4.x bundled `.gitignore` lines.
//!
//! Before this change, `pm project init` / `sync` wrote a small fixed set
//! of patterns directly into the user's `.gitignore` (no marker block).
//! On the first `pm project gitignore` after upgrading, we scan the user
//! region for those exact lines and strip them — they will reappear inside
//! the new managed block.
//!
//! Lines we don't recognize are left in place so user customizations are
//! never silently deleted.

/// Patterns that v0.4.x bundles wrote, indexed by language `common` group.
/// Each entry is matched against a trimmed line of the user's `.gitignore`.
///
/// The list is intentionally narrow: only the exact strings v0.4.x's
/// `configs/<lang>/common/.gitignore` contained. Anything resembling them
/// but slightly different stays as user content (safer fallback).
const LEGACY_PATTERNS: &[&str] = &[
    // configs/rust/common/.gitignore
    "/target",
    "**/*.rs.bk",
    "*.pdb",
    // configs/ts/common/.gitignore
    "node_modules/",
    "dist/",
    ".env",
    ".env.local",
    "*.tsbuildinfo",
    // configs/python/common/.gitignore
    "__pycache__/",
    "*.py[cod]",
    "*.egg-info/",
    ".venv/",
    ".ruff_cache/",
    ".pytest_cache/",
    ".mypy_cache/",
    // configs/dart/common/.gitignore  (v0.4.x had .gitignore via dart/flutter only)
    // (no extra dart entries — flutter's only line was empty list before)
];

/// Strip recognized v0.4.x bundle lines from `user_region`. Returns the
/// cleaned text and the count of lines removed.
///
/// Comments and blank lines are always preserved. Pattern lines that don't
/// match the legacy set are also preserved.
pub fn strip_legacy_patterns(user_region: &str) -> (String, usize) {
    let mut removed = 0usize;
    let mut out = String::with_capacity(user_region.len());

    for line in user_region.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if LEGACY_PATTERNS.contains(&trimmed) {
            removed += 1;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    // Preserve a missing-trailing-newline if the input had none.
    if !user_region.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }

    (out, removed)
}

/// Emit a one-line stderr notice when migration removed any lines.
pub fn emit_migration_notice(removed: usize) {
    if removed == 0 {
        return;
    }
    eprintln!(
        "pm: migrated {removed} legacy line{} into the pm-managed `.gitignore` block",
        if removed == 1 { "" } else { "s" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_removes_known_rust_lines() {
        let input = "/target\n**/*.rs.bk\n*.pdb\nuser-line\n";
        let (out, n) = strip_legacy_patterns(input);
        assert_eq!(n, 3);
        assert_eq!(out, "user-line\n");
    }

    #[test]
    fn strip_preserves_user_lines() {
        let input = "my-private-dir/\nsecret.json\n";
        let (out, n) = strip_legacy_patterns(input);
        assert_eq!(n, 0);
        assert_eq!(out, input);
    }

    #[test]
    fn strip_preserves_comments_and_blanks() {
        let input = "# my comment\n\n/target\n# another\n";
        let (out, n) = strip_legacy_patterns(input);
        assert_eq!(n, 1);
        assert!(out.contains("# my comment"));
        assert!(out.contains("# another"));
        assert!(!out.contains("/target"));
    }

    #[test]
    fn strip_keeps_lookalikes_distinct_from_legacy() {
        // Trailing slash difference, leading-slash difference: keep as user.
        let input = "/target/\ntarget\nnode_modules\n.env.staging\n";
        let (out, n) = strip_legacy_patterns(input);
        assert_eq!(n, 0, "non-exact matches must not be stripped");
        assert_eq!(out, input);
    }

    #[test]
    fn strip_returns_empty_for_all_legacy_input() {
        let input = "/target\nnode_modules/\n*.tsbuildinfo\n";
        let (out, n) = strip_legacy_patterns(input);
        assert_eq!(n, 3);
        assert_eq!(out, "");
    }

    #[test]
    fn strip_no_trailing_newline_input_no_trailing_newline_output() {
        let input = "user-line";
        let (out, n) = strip_legacy_patterns(input);
        assert_eq!(n, 0);
        assert_eq!(out, "user-line");
    }
}
