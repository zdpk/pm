//! Bundled `.gitignore` template synthesis.
//!
//! This module exposes the small public API used by `pm project init`,
//! `pm project sync`, and the new `pm project gitignore` subcommand:
//!
//! - [`Category`]: a known embedded gitignore source.
//! - [`lookup_category`]: case-insensitive name lookup.
//! - [`default_categories`]: the set selected automatically based on the
//!   project's `language` and `framework`.
//! - Synthesis helpers (in [`synthesize`](crate::templates::synthesize))
//!   that compose categories into a single `.gitignore` body.
//! - Marker-block helpers (in [`marker`](crate::templates::marker)) that
//!   isolate the pm-managed region from the user's hand-written content.
//!
//! The actual template strings come from `vendor/github-gitignore/` via
//! [`build.rs`] and are accessible as `&'static str` constants in
//! [`embedded`].
//!
//! [`build.rs`]: ../../../build.rs

#![allow(dead_code)] // public API consumed by Stage 2 (cmd_gitignore, synthesize, init/sync)

pub mod embedded;
pub mod marker;
pub mod synthesize;

use std::fmt;

/// One of the embedded `.gitignore` categories. New variants must keep
/// the enum, [`Category::ALL`], [`Category::content`], and [`Category::header`]
/// in sync. They must also appear in `build.rs`'s `CATEGORIES` table and
/// in the `bundled-templates` spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Macos,
    Linux,
    Windows,
    Vscode,
    Jetbrains,
    Rust,
    Node,
    Python,
    Dart,
    Go,
}

impl Category {
    /// Every embedded category, in arbitrary canonical order.
    pub const ALL: &'static [Category] = &[
        Category::Macos,
        Category::Linux,
        Category::Windows,
        Category::Vscode,
        Category::Jetbrains,
        Category::Rust,
        Category::Node,
        Category::Python,
        Category::Dart,
        Category::Go,
    ];

    /// Lowercase identifier used on the CLI (`--categories rust,macos`).
    pub fn key(self) -> &'static str {
        match self {
            Category::Macos => "macos",
            Category::Linux => "linux",
            Category::Windows => "windows",
            Category::Vscode => "vscode",
            Category::Jetbrains => "jetbrains",
            Category::Rust => "rust",
            Category::Node => "node",
            Category::Python => "python",
            Category::Dart => "dart",
            Category::Go => "go",
        }
    }

    /// Section header used inside the synthesized managed block. Mirrors
    /// the spec's `# === <Header> ===` convention.
    pub fn header(self) -> &'static str {
        match self {
            Category::Macos => "OS: macOS",
            Category::Linux => "OS: Linux",
            Category::Windows => "OS: Windows",
            Category::Vscode => "IDE: Visual Studio Code",
            Category::Jetbrains => "IDE: JetBrains",
            Category::Rust => "Language: Rust",
            Category::Node => "Language: Node",
            Category::Python => "Language: Python",
            Category::Dart => "Language: Dart",
            Category::Go => "Language: Go",
        }
    }

    /// Embedded contents from `vendor/github-gitignore/`. CC0-licensed.
    pub fn content(self) -> &'static str {
        match self {
            Category::Macos => embedded::MACOS,
            Category::Linux => embedded::LINUX,
            Category::Windows => embedded::WINDOWS,
            Category::Vscode => embedded::VSCODE,
            Category::Jetbrains => embedded::JETBRAINS,
            Category::Rust => embedded::RUST,
            Category::Node => embedded::NODE,
            Category::Python => embedded::PYTHON,
            Category::Dart => embedded::DART,
            Category::Go => embedded::GO,
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Resolve a category by its CLI key. Case-insensitive.
pub fn lookup_category(name: &str) -> Option<Category> {
    let lower = name.trim().to_ascii_lowercase();
    Category::ALL.iter().copied().find(|c| c.key() == lower)
}

/// The default set of categories for a project with the given language
/// and (optional) framework. Order matches the spec's required composition
/// order: OS → IDE → Language → (framework extras handled separately).
pub fn default_categories(language: &str, _framework: Option<&str>) -> Vec<Category> {
    let mut out = vec![
        Category::Macos,
        Category::Linux,
        Category::Windows,
        Category::Vscode,
        Category::Jetbrains,
    ];
    // Language-specific category. `ts` and `node` both map to Node.
    let lang_lower = language.to_ascii_lowercase();
    let lang_cat = match lang_lower.as_str() {
        "rust" => Some(Category::Rust),
        "ts" | "typescript" | "javascript" | "js" | "node" => Some(Category::Node),
        "python" | "py" => Some(Category::Python),
        "dart" => Some(Category::Dart),
        "go" | "golang" => Some(Category::Go),
        _ => None,
    };
    if let Some(c) = lang_cat {
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_category_case_insensitive() {
        assert_eq!(lookup_category("rust"), Some(Category::Rust));
        assert_eq!(lookup_category("RUST"), Some(Category::Rust));
        assert_eq!(lookup_category(" RuSt "), Some(Category::Rust));
    }

    #[test]
    fn lookup_unknown_category() {
        assert_eq!(lookup_category("perl"), None);
        assert_eq!(lookup_category(""), None);
    }

    #[test]
    fn every_category_has_non_empty_content() {
        for c in Category::ALL {
            assert!(
                !c.content().trim().is_empty(),
                "category {} has empty content",
                c.key()
            );
        }
    }

    #[test]
    fn default_categories_for_rust() {
        let cats = default_categories("rust", None);
        assert!(cats.contains(&Category::Rust));
        assert!(cats.contains(&Category::Macos));
        assert!(cats.contains(&Category::Vscode));
        assert!(!cats.contains(&Category::Node));
    }

    #[test]
    fn default_categories_for_ts() {
        let cats = default_categories("ts", Some("nextjs"));
        assert!(cats.contains(&Category::Node));
        assert!(!cats.contains(&Category::Rust));
    }

    #[test]
    fn default_categories_unknown_language_omits_language_section() {
        let cats = default_categories("crystal", None);
        // OS + IDE only, no language category
        assert_eq!(cats.len(), 5);
    }

    #[test]
    fn default_categories_includes_windows_for_team_safety() {
        let cats = default_categories("rust", None);
        assert!(
            cats.contains(&Category::Windows),
            "Windows is included by default per design D7"
        );
    }
}
