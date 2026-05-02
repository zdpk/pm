//! `.gitignore` content synthesis.
//!
//! Builds the body that will live inside the pm-managed marker block by
//! concatenating selected category contents, prefixed with section
//! headers and source attribution. Pattern lines that appear in more than
//! one category are deduplicated (first wins); comments and blank lines
//! are preserved verbatim.

use crate::templates::Category;

/// Section header for framework-specific extras layered on top of the
/// last category. `framework_name` is shown verbatim.
const FRAMEWORK_HEADER_PREFIX: &str = "Framework: ";

/// `# Source: ...` attribution applied to every github/gitignore-derived
/// section. Framework extras get a different note (managed in this repo).
const ATTRIBUTION_GITHUB: &str = "# Source: https://github.com/github/gitignore (CC0)";
const ATTRIBUTION_PM: &str = "# Source: pm bundled framework extras";

/// Build the synthesized body that will be placed inside the pm-managed
/// marker block. Does NOT include the markers themselves — those are added
/// by [`super::marker::merge_into_existing`].
///
/// `framework_extra` is the verbatim contents of
/// `configs/<lang>/<fw>/.gitignore.extra` (if present), and
/// `framework_name` is the corresponding framework identifier (e.g. `"nextjs"`).
pub fn synthesize_managed_body(
    categories: &[Category],
    framework_name: Option<&str>,
    framework_extra: Option<&str>,
) -> String {
    let mut out = String::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, cat) in categories.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_section_header(&mut out, cat.header());
        out.push_str(ATTRIBUTION_GITHUB);
        out.push('\n');
        push_deduped(&mut out, cat.content(), &mut seen);
    }

    if let (Some(name), Some(extra)) = (framework_name, framework_extra)
        && !extra.trim().is_empty()
    {
        if !out.is_empty() {
            out.push('\n');
        }
        let header = format!("{FRAMEWORK_HEADER_PREFIX}{name}");
        write_section_header(&mut out, &header);
        out.push_str(ATTRIBUTION_PM);
        out.push('\n');
        push_deduped(&mut out, extra, &mut seen);
    }

    out
}

fn write_section_header(out: &mut String, label: &str) {
    out.push_str("# === ");
    out.push_str(label);
    out.push_str(" ===\n");
}

/// Append `content` to `out`, dropping pattern-bearing lines that have
/// already been seen. Comments and blank lines are emitted unchanged.
fn push_deduped(out: &mut String, content: &str, seen: &mut std::collections::HashSet<String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            // Preserve comments and whitespace as-is — they are part of
            // the bundled template's narrative.
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Pattern lines are de-duplicated by their trimmed text.
        if seen.insert(trimmed.to_string()) {
            out.push_str(line);
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_emits_headers_and_attribution() {
        let body = synthesize_managed_body(&[Category::Macos], None, None);
        assert!(body.contains("# === OS: macOS ==="));
        assert!(body.contains(ATTRIBUTION_GITHUB));
        assert!(body.contains(".DS_Store"));
    }

    #[test]
    fn synthesize_dedups_pattern_across_categories() {
        // Inject a fake duplicate by chaining the same category twice.
        let body = synthesize_managed_body(&[Category::Macos, Category::Macos], None, None);
        let count = body.matches(".DS_Store").count();
        assert_eq!(
            count, 1,
            "duplicate pattern across categories should appear once"
        );
    }

    #[test]
    fn synthesize_preserves_comments_in_each_section() {
        // Both Linux and Windows have comment lines; comments should not
        // be deduped even when they happen to match.
        let body = synthesize_managed_body(&[Category::Linux, Category::Windows], None, None);
        // Both sections must be present (full headers).
        assert!(body.contains("# === OS: Linux ==="));
        assert!(body.contains("# === OS: Windows ==="));
    }

    #[test]
    fn synthesize_appends_framework_extra_with_header() {
        let extra = "\n.next/\nout/\n.vercel\n";
        let body = synthesize_managed_body(&[Category::Node], Some("nextjs"), Some(extra));
        assert!(body.contains("# === Framework: nextjs ==="));
        assert!(body.contains(ATTRIBUTION_PM));
        assert!(body.contains(".next/"));
        assert!(body.contains(".vercel"));
    }

    #[test]
    fn synthesize_skips_empty_framework_extra() {
        let body = synthesize_managed_body(&[Category::Node], Some("nextjs"), Some("   \n\n"));
        assert!(!body.contains("Framework: nextjs"));
    }

    #[test]
    fn synthesize_dedups_framework_pattern_against_categories() {
        // Node already contains `node_modules/`; framework extra repeats it.
        let extra = "node_modules/\n.next/\n";
        let body = synthesize_managed_body(&[Category::Node], Some("nextjs"), Some(extra));
        let count = body.matches("node_modules/").count();
        assert_eq!(count, 1);
        assert!(body.contains(".next/"));
    }

    #[test]
    fn synthesize_empty_categories_returns_empty_or_framework_only() {
        assert_eq!(synthesize_managed_body(&[], None, None), "");
        let body = synthesize_managed_body(&[], Some("nextjs"), Some(".next/\n"));
        assert!(body.contains("Framework: nextjs"));
        assert!(body.contains(".next/"));
    }
}
