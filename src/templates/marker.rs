//! pm-managed marker block in `.gitignore`.
//!
//! Marker text is **fixed** per design D4 / Resolved Decision 1. Changes
//! require a new BREAKING change because in-the-wild `.gitignore` files
//! depend on these exact strings.
//!
//! The model: a `.gitignore` is split into three regions —
//!   1. user content above the begin marker
//!   2. pm-managed body between the markers (overwritten on each refresh)
//!   3. user content below the end marker
//!
//! `pm` only reads/writes region 2; regions 1 and 3 are preserved
//! byte-for-byte. The first time `pm` writes a managed block to a file
//! that has none, the block is appended to the end (preceded by a blank
//! line if needed).

/// Opening marker line. Must match the exact text on disk to be detected.
pub const BEGIN_MARKER: &str =
    "# >>> pm managed (do not edit; run `pm project gitignore` to refresh) >>>";

/// Closing marker line.
pub const END_MARKER: &str = "# <<< pm managed <<<";

/// Result of parsing a `.gitignore` into its three regions.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedGitignore {
    /// Content above the BEGIN marker (verbatim, preserved on write).
    pub before: String,
    /// Content strictly between BEGIN and END markers, exclusive of both
    /// marker lines. `None` if no marker block exists.
    pub managed: Option<String>,
    /// Content below the END marker (verbatim).
    pub after: String,
}

/// Split `content` into [`ParsedGitignore`] regions. Tolerant of files
/// that have no marker, or files where the markers exist but the END
/// marker is missing (treated as if no marker were present, leaving the
/// caller to recover by appending a fresh block).
pub fn parse(content: &str) -> ParsedGitignore {
    let Some(begin_idx) = find_line(content, BEGIN_MARKER) else {
        return ParsedGitignore {
            before: content.to_string(),
            managed: None,
            after: String::new(),
        };
    };

    // Search for the closing marker AFTER the begin line.
    let after_begin_offset = begin_idx + BEGIN_MARKER.len();
    let tail = &content[after_begin_offset..];
    let Some(rel_end) = find_line(tail, END_MARKER) else {
        // Malformed: begin without end. Treat the whole file as user
        // content so we don't lose anything; the caller will append a
        // fresh block at the bottom.
        return ParsedGitignore {
            before: content.to_string(),
            managed: None,
            after: String::new(),
        };
    };
    let end_idx = after_begin_offset + rel_end;
    let end_line_end = end_idx + END_MARKER.len();

    // BEGIN marker line spans [line_start_of_begin, after_begin_offset].
    // We include the trailing newline if present (so `before` ends with `\n`).
    let mut before_end = begin_idx;
    // Strip a single trailing newline before the marker for cleanliness;
    // keep everything else exactly.
    if before_end > 0 && content.as_bytes()[before_end - 1] == b'\n' {
        before_end -= 1;
    }
    let before = content[..before_end].to_string();

    // Managed body is between the end of the BEGIN line and the start of
    // the END marker. Skip the newline immediately after BEGIN, and the
    // newline immediately before END, so the body is clean.
    let mut managed_start = after_begin_offset;
    if managed_start < content.len() && content.as_bytes()[managed_start] == b'\n' {
        managed_start += 1;
    }
    let mut managed_end = end_idx;
    if managed_end > 0 && content.as_bytes()[managed_end - 1] == b'\n' {
        managed_end -= 1;
    }
    let managed = content[managed_start..managed_end].to_string();

    // After the END marker, optionally skip one trailing newline.
    let mut after_start = end_line_end;
    if after_start < content.len() && content.as_bytes()[after_start] == b'\n' {
        after_start += 1;
    }
    let after = content[after_start..].to_string();

    ParsedGitignore {
        before,
        managed: Some(managed),
        after,
    }
}

/// Render the file content from its parts. `managed_body` is the body
/// (without markers) to place between BEGIN and END.
pub fn render(parsed: &ParsedGitignore, managed_body: &str) -> String {
    let mut out = String::new();
    out.push_str(&parsed.before);
    // Ensure blank line separation between user content and the marker.
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }

    out.push_str(BEGIN_MARKER);
    out.push('\n');
    out.push_str(managed_body);
    if !managed_body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(END_MARKER);
    out.push('\n');

    if !parsed.after.is_empty() {
        out.push('\n');
        out.push_str(&parsed.after);
    }

    out
}

/// High-level helper: take the existing file content (or empty string for
/// a brand-new file), the freshly synthesized managed body, and produce
/// the new file content.
pub fn merge_into_existing(existing: &str, managed_body: &str) -> String {
    let mut parsed = parse(existing);
    parsed.managed = Some(managed_body.to_string());
    render(&parsed, managed_body)
}

fn find_line(haystack: &str, needle: &str) -> Option<usize> {
    // Find `needle` as a complete line (newline-bounded or at start/end of file).
    let bytes = haystack.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        let line_end = bytes[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| start + i)
            .unwrap_or(bytes.len());
        let line = &haystack[start..line_end];
        if line == needle {
            return Some(start);
        }
        if line_end == bytes.len() {
            break;
        }
        start = line_end + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_file() {
        let p = parse("");
        assert_eq!(p.before, "");
        assert_eq!(p.managed, None);
        assert_eq!(p.after, "");
    }

    #[test]
    fn parse_user_only_no_markers() {
        let p = parse("user-line-1\nuser-line-2\n");
        assert_eq!(p.before, "user-line-1\nuser-line-2\n");
        assert_eq!(p.managed, None);
    }

    #[test]
    fn parse_with_full_marker_block() {
        let content = format!(
            "user-above\n\n{BEGIN_MARKER}\n# === OS: macOS ===\n.DS_Store\n{END_MARKER}\nuser-below\n",
        );
        let p = parse(&content);
        assert_eq!(p.before, "user-above\n");
        assert_eq!(p.managed.as_deref(), Some("# === OS: macOS ===\n.DS_Store"));
        assert_eq!(p.after, "user-below\n");
    }

    #[test]
    fn parse_with_only_begin_marker_treats_as_user_content() {
        let content = format!("user-above\n{BEGIN_MARKER}\n# === ... ===\n");
        let p = parse(&content);
        assert!(p.managed.is_none());
        assert_eq!(p.before, content);
    }

    #[test]
    fn render_appends_block_to_end_when_missing() {
        let merged = merge_into_existing("existing-line\n", "# === OS: Linux ===\n*.swp\n");
        assert!(merged.contains("existing-line"));
        assert!(merged.contains(BEGIN_MARKER));
        assert!(merged.contains("# === OS: Linux ==="));
        assert!(merged.contains(END_MARKER));
        // Marker block comes after user content.
        let user_idx = merged.find("existing-line").unwrap();
        let begin_idx = merged.find(BEGIN_MARKER).unwrap();
        assert!(user_idx < begin_idx);
    }

    #[test]
    fn render_replaces_existing_block_in_place() {
        let initial = format!("user-A\n\n{BEGIN_MARKER}\nold-managed\n{END_MARKER}\nuser-B\n",);
        let merged = merge_into_existing(&initial, "new-managed\n");
        assert!(merged.contains("new-managed"));
        assert!(!merged.contains("old-managed"));
        // User content preserved.
        assert!(merged.contains("user-A"));
        assert!(merged.contains("user-B"));
    }

    #[test]
    fn render_preserves_user_byte_for_byte_above_and_below() {
        let initial = format!(
            "trailing space   \n\t# tabbed comment\n\n{BEGIN_MARKER}\nbody\n{END_MARKER}\n# below\n   \n",
        );
        let merged = merge_into_existing(&initial, "body2\n");
        // Above
        assert!(merged.starts_with("trailing space   \n\t# tabbed comment\n"));
        // Below
        assert!(merged.contains("# below\n   \n"));
    }

    #[test]
    fn render_produces_block_for_empty_input() {
        let merged = merge_into_existing("", "# === OS: macOS ===\n.DS_Store\n");
        assert!(merged.starts_with(BEGIN_MARKER));
        assert!(merged.contains("# === OS: macOS ==="));
        assert!(merged.contains(END_MARKER));
    }

    #[test]
    fn marker_text_is_fixed_constant() {
        // Regression guard: changing these strings would break all
        // existing user `.gitignore` files. Bump the spec and add a
        // BREAKING change before touching them.
        assert_eq!(
            BEGIN_MARKER,
            "# >>> pm managed (do not edit; run `pm project gitignore` to refresh) >>>",
        );
        assert_eq!(END_MARKER, "# <<< pm managed <<<");
    }
}
