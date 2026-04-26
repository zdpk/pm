use crate::error::PmError;
use crate::git;
use crate::path::expand_path;
use anyhow::Result;
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── .project.yaml schema ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjConfig {
    pub language: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,

    pub config_version: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<String>,
}

// ── Config repo manifest schema ──

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalManifest {
    #[allow(dead_code)]
    pub meta: ManifestMeta,
    pub languages: Vec<LanguageEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestMeta {
    #[allow(dead_code)]
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageEntry {
    pub id: String,
    pub name: String,
    pub markers: Vec<String>,
    #[serde(default)]
    pub frameworks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigManifest {
    pub files: Vec<ConfigFileEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFileEntry {
    pub path: String,
    pub strategy: FileStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStrategy {
    Managed,
    Merged,
    Template,
}

impl FileStrategy {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Merged => "merged",
            Self::Template => "template",
        }
    }
}

// ── .project.yaml I/O ──

pub fn load_proj_config(project_dir: &Path) -> Result<ProjConfig> {
    let path = project_dir.join(".project.yaml");
    if !path.exists() {
        return Err(PmError::ProjNotInitialized.into());
    }
    let content = fs::read_to_string(&path)?;
    let config: ProjConfig = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse .project.yaml: {e}"))?;
    Ok(config)
}

pub fn save_proj_config(project_dir: &Path, config: &ProjConfig) -> Result<()> {
    let path = project_dir.join(".project.yaml");
    let content = serde_yaml::to_string(config)
        .map_err(|e| anyhow::anyhow!("Failed to serialize .project.yaml: {e}"))?;
    fs::write(&path, content)?;
    Ok(())
}

// ── Config repo management ──

pub fn ensure_config_repo(url: &str, cache_dir: &str) -> Result<PathBuf> {
    let expanded = expand_path(cache_dir);
    if expanded.join(".git").exists() {
        return Ok(expanded);
    }
    if let Some(parent) = expanded.parent() {
        fs::create_dir_all(parent)?;
    }
    git::clone_repo(url, &expanded)
        .map_err(|_| PmError::ConfigRepoUpdateFailed("git clone failed".to_string()))?;
    Ok(expanded)
}

pub fn update_config_repo(url: &str, cache_dir: &str) -> Result<PathBuf> {
    let expanded = expand_path(cache_dir);
    if !expanded.join(".git").exists() {
        return ensure_config_repo(url, cache_dir);
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(&expanded)
        .arg("pull")
        .arg("--ff-only")
        .status()?;
    if !status.success() {
        return Err(PmError::ConfigRepoUpdateFailed("git pull failed".to_string()).into());
    }
    Ok(expanded)
}

pub fn config_repo_head(repo_path: &Path) -> Result<String> {
    let repo = Repository::open(repo_path)?;
    let head = repo.head()?;
    let oid = head
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD has no target"))?;
    Ok(format!("{}", oid)[..7].to_string())
}

// ── Language / framework detection ──

pub fn load_global_manifest(repo_path: &Path) -> Result<GlobalManifest> {
    let manifest_path = repo_path.join("manifest.yaml");
    if !manifest_path.exists() {
        return Err(anyhow::anyhow!("Config repo missing manifest.yaml"));
    }
    let content = fs::read_to_string(&manifest_path)?;
    let manifest: GlobalManifest = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse manifest.yaml: {e}"))?;
    Ok(manifest)
}

pub fn detect_language(project_dir: &Path, manifest: &GlobalManifest) -> Option<String> {
    for lang in &manifest.languages {
        for marker in &lang.markers {
            if marker.contains('*') {
                // Glob pattern like "*.c"
                if let Some(ext) = marker.strip_prefix("*.") {
                    if has_files_with_extension(project_dir, ext) {
                        return Some(lang.id.clone());
                    }
                }
            } else if project_dir.join(marker).exists() {
                return Some(lang.id.clone());
            }
        }
    }
    None
}

fn has_files_with_extension(dir: &Path, ext: &str) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(|entry| entry.ok()).any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|file_ext| file_ext == ext)
    })
}

pub fn detect_framework(project_dir: &Path, language: &str) -> Option<String> {
    match language {
        "rust" => detect_rust_framework(project_dir),
        "ts" => detect_ts_framework(project_dir),
        "python" => detect_python_framework(project_dir),
        "dart" => detect_dart_framework(project_dir),
        _ => None,
    }
}

fn detect_rust_framework(dir: &Path) -> Option<String> {
    let cargo_toml = dir.join("Cargo.toml");
    let content = fs::read_to_string(cargo_toml).ok()?;
    if content.contains("axum") {
        Some("axum".to_string())
    } else if content.contains("clap") {
        Some("clap".to_string())
    } else {
        None
    }
}

fn detect_ts_framework(dir: &Path) -> Option<String> {
    // Check for next.config.*
    for ext in &["js", "mjs", "ts"] {
        if dir.join(format!("next.config.{ext}")).exists() {
            return Some("nextjs".to_string());
        }
    }
    if dir.join("nest-cli.json").exists() {
        return Some("nestjs".to_string());
    }
    None
}

fn detect_python_framework(dir: &Path) -> Option<String> {
    let pyproject = dir.join("pyproject.toml");
    if let Ok(content) = fs::read_to_string(pyproject) {
        if content.contains("fastapi") {
            return Some("fastapi".to_string());
        }
    }
    None
}

fn detect_dart_framework(dir: &Path) -> Option<String> {
    let pubspec = dir.join("pubspec.yaml");
    if let Ok(content) = fs::read_to_string(pubspec) {
        if content.contains("flutter") {
            return Some("flutter".to_string());
        }
    }
    None
}

// ── Config manifest loading ──

pub fn load_config_manifest(
    repo_path: &Path,
    lang: &str,
    fw: Option<&str>,
) -> Result<Vec<(PathBuf, ConfigFileEntry)>> {
    let mut files = Vec::new();

    // 1. common/ manifest
    let common_dir = repo_path.join(lang).join("common");
    if let Ok(entries) = load_dir_manifest(&common_dir) {
        for entry in entries {
            let source = common_dir.join(&entry.path);
            if source.exists() {
                files.push((source, entry));
            }
        }
    }

    // 2. framework/ manifest
    if let Some(fw) = fw {
        let fw_dir = repo_path.join(lang).join(fw);
        if let Ok(entries) = load_dir_manifest(&fw_dir) {
            for entry in entries {
                let source = fw_dir.join(&entry.path);
                if source.exists() {
                    files.push((source, entry));
                }
            }
        }
    }

    Ok(files)
}

/// Collect shared/ files based on includes (ci, docker, hooks)
pub fn collect_shared_files(
    repo_path: &Path,
    includes: &[String],
) -> Result<Vec<(PathBuf, ConfigFileEntry)>> {
    let mut files = Vec::new();
    let shared_dir = repo_path.join("shared");

    // Always include root shared files (.editorconfig, .gitignore)
    if let Ok(entries) = load_dir_manifest(&shared_dir) {
        for entry in entries {
            let source = shared_dir.join(&entry.path);
            if source.exists() {
                files.push((source, entry));
            }
        }
    }

    // Include subdirectories based on includes
    for include in includes {
        let sub_dir = shared_dir.join(include);
        if sub_dir.is_dir() {
            collect_dir_recursive(&sub_dir, &sub_dir, &mut files)?;
        }
    }

    Ok(files)
}

/// Collect all source files for a project: common + framework + shared
pub fn collect_all_source_files(
    repo_path: &Path,
    config: &ProjConfig,
) -> Result<Vec<(PathBuf, ConfigFileEntry)>> {
    let mut files = load_config_manifest(repo_path, &config.language, config.framework.as_deref())?;
    let shared = collect_shared_files(repo_path, &config.includes)?;
    files.extend(shared);
    Ok(files)
}

fn collect_dir_recursive(
    base: &Path,
    dir: &Path,
    files: &mut Vec<(PathBuf, ConfigFileEntry)>,
) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_dir_recursive(base, &path, files)?;
        } else if path.file_name().is_some_and(|n| n != "manifest.yaml") {
            let rel = path
                .strip_prefix(base)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            if !rel.is_empty() {
                files.push((
                    path.clone(),
                    ConfigFileEntry {
                        path: rel,
                        strategy: FileStrategy::Managed,
                    },
                ));
            }
        }
    }
    Ok(())
}

fn load_dir_manifest(dir: &Path) -> Result<Vec<ConfigFileEntry>> {
    let manifest_path = dir.join("manifest.yaml");
    if !manifest_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&manifest_path)?;
    let manifest: ConfigManifest = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", manifest_path.display()))?;
    Ok(manifest.files)
}

// ── File strategy execution ──

/// Apply a managed file: overwrite completely. Returns true if file changed.
pub fn apply_managed(source: &Path, target: &Path) -> Result<bool> {
    let source_content = fs::read(source)?;
    if target.exists() {
        let target_content = fs::read(target)?;
        if source_content == target_content {
            return Ok(false);
        }
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, source_content)?;
    Ok(true)
}

/// Apply a merged file: append missing lines from source. Returns true if file changed.
pub fn apply_merged(source: &Path, target: &Path) -> Result<bool> {
    let source_content = fs::read_to_string(source)?;

    if !target.exists() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, &source_content)?;
        return Ok(true);
    }

    let target_content = fs::read_to_string(target)?;
    let target_lines: HashSet<&str> = target_content.lines().collect();

    let new_lines: Vec<&str> = source_content
        .lines()
        .filter(|line| !line.trim().is_empty() && !target_lines.contains(line))
        .collect();

    if new_lines.is_empty() {
        return Ok(false);
    }

    let mut content = target_content;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str("\n# --- project-managed ---\n");
    for line in &new_lines {
        content.push_str(line);
        content.push('\n');
    }
    fs::write(target, content)?;
    Ok(true)
}

/// Apply a template file: copy only if target does not exist. Returns true if created.
pub fn apply_template(source: &Path, target: &Path) -> Result<bool> {
    if target.exists() {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, target)?;
    Ok(true)
}

/// Apply a file with the given strategy
pub fn apply_file(source: &Path, target: &Path, strategy: FileStrategy) -> Result<bool> {
    match strategy {
        FileStrategy::Managed => apply_managed(source, target),
        FileStrategy::Merged => apply_merged(source, target),
        FileStrategy::Template => apply_template(source, target),
    }
}

// ── Diff ──

pub fn diff_file(source: &Path, target: &Path) -> Option<String> {
    let source_content = fs::read_to_string(source).ok()?;
    let target_content = if target.exists() {
        fs::read_to_string(target).ok()?
    } else {
        String::new()
    };

    if source_content == target_content {
        return None;
    }

    let diff = similar::TextDiff::from_lines(&target_content, &source_content);
    let mut output = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{sign}{change}"));
    }
    Some(output)
}

/// Check if a file is outdated (content differs from source)
pub fn is_file_outdated(source: &Path, target: &Path, strategy: FileStrategy) -> bool {
    match strategy {
        FileStrategy::Template => false, // templates are never synced
        FileStrategy::Managed => {
            let Ok(src) = fs::read(source) else {
                return false;
            };
            let Ok(tgt) = fs::read(target) else {
                return true;
            };
            src != tgt
        }
        FileStrategy::Merged => {
            let Ok(src) = fs::read_to_string(source) else {
                return false;
            };
            let Ok(tgt) = fs::read_to_string(target) else {
                return true;
            };
            let target_lines: HashSet<&str> = tgt.lines().collect();
            src.lines()
                .any(|line| !line.trim().is_empty() && !target_lines.contains(line))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── ProjConfig I/O ──

    #[test]
    fn test_save_and_load_proj_config() {
        let dir = TempDir::new().unwrap();
        let config = ProjConfig {
            language: "rust".to_string(),
            framework: Some("axum".to_string()),
            config_version: "abc1234".to_string(),
            includes: vec!["ci".to_string(), "docker".to_string()],
        };
        save_proj_config(dir.path(), &config).unwrap();

        let loaded = load_proj_config(dir.path()).unwrap();
        assert_eq!(loaded.language, "rust");
        assert_eq!(loaded.framework.as_deref(), Some("axum"));
        assert_eq!(loaded.config_version, "abc1234");
        assert_eq!(loaded.includes, vec!["ci", "docker"]);
    }

    #[test]
    fn test_save_proj_config_no_framework() {
        let dir = TempDir::new().unwrap();
        let config = ProjConfig {
            language: "c".to_string(),
            framework: None,
            config_version: "def5678".to_string(),
            includes: Vec::new(),
        };
        save_proj_config(dir.path(), &config).unwrap();

        let loaded = load_proj_config(dir.path()).unwrap();
        assert_eq!(loaded.language, "c");
        assert!(loaded.framework.is_none());
        assert!(loaded.includes.is_empty());
    }

    #[test]
    fn test_load_proj_config_missing_file() {
        let dir = TempDir::new().unwrap();
        let result = load_proj_config(dir.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(".project.yaml"));
    }

    #[test]
    fn test_load_proj_config_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".project.yaml"), ":::invalid yaml[[[").unwrap();
        let result = load_proj_config(dir.path());
        assert!(result.is_err());
    }

    // ── FileStrategy ──

    #[test]
    fn test_file_strategy_label() {
        assert_eq!(FileStrategy::Managed.label(), "managed");
        assert_eq!(FileStrategy::Merged.label(), "merged");
        assert_eq!(FileStrategy::Template.label(), "template");
    }

    #[test]
    fn test_file_strategy_deserialize() {
        let yaml = "files:\n  - path: test.txt\n    strategy: managed\n";
        let manifest: ConfigManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.files[0].strategy, FileStrategy::Managed);

        let yaml = "files:\n  - path: test.txt\n    strategy: merged\n";
        let manifest: ConfigManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.files[0].strategy, FileStrategy::Merged);

        let yaml = "files:\n  - path: test.txt\n    strategy: template\n";
        let manifest: ConfigManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.files[0].strategy, FileStrategy::Template);
    }

    // ── apply_managed ──

    #[test]
    fn test_apply_managed_creates_new_file() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "hello").unwrap();

        let changed = apply_managed(&source, &target).unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn test_apply_managed_overwrites_different_content() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "new content").unwrap();
        fs::write(&target, "old content").unwrap();

        let changed = apply_managed(&source, &target).unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), "new content");
    }

    #[test]
    fn test_apply_managed_skips_identical() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "same").unwrap();
        fs::write(&target, "same").unwrap();

        let changed = apply_managed(&source, &target).unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_apply_managed_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("sub/dir/target.txt");
        fs::write(&source, "deep").unwrap();

        let changed = apply_managed(&source, &target).unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), "deep");
    }

    // ── apply_template ──

    #[test]
    fn test_apply_template_creates_new_file() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "template content").unwrap();

        let changed = apply_template(&source, &target).unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), "template content");
    }

    #[test]
    fn test_apply_template_skips_existing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "new template").unwrap();
        fs::write(&target, "existing content").unwrap();

        let changed = apply_template(&source, &target).unwrap();
        assert!(!changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), "existing content");
    }

    // ── apply_merged ──

    #[test]
    fn test_apply_merged_creates_new_file() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "line1\nline2\n").unwrap();

        let changed = apply_merged(&source, &target).unwrap();
        assert!(changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), "line1\nline2\n");
    }

    #[test]
    fn test_apply_merged_appends_missing_lines() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "line1\nline2\nline3\n").unwrap();
        fs::write(&target, "line1\ncustom\n").unwrap();

        let changed = apply_merged(&source, &target).unwrap();
        assert!(changed);
        let content = fs::read_to_string(&target).unwrap();
        assert!(content.contains("custom")); // preserved
        assert!(content.contains("line2")); // added
        assert!(content.contains("line3")); // added
        assert!(content.contains("# --- project-managed ---")); // marker
    }

    #[test]
    fn test_apply_merged_skips_when_all_present() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "line1\nline2\n").unwrap();
        fs::write(&target, "line1\nline2\nextra\n").unwrap();

        let changed = apply_merged(&source, &target).unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_apply_merged_idempotent() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "line1\nline2\n").unwrap();
        fs::write(&target, "line1\n").unwrap();

        apply_merged(&source, &target).unwrap();
        let content_after_first = fs::read_to_string(&target).unwrap();

        // Second merge should be a no-op
        let changed = apply_merged(&source, &target).unwrap();
        assert!(!changed);
        assert_eq!(fs::read_to_string(&target).unwrap(), content_after_first);
    }

    // ── apply_file dispatch ──

    #[test]
    fn test_apply_file_dispatches_correctly() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("src.txt");
        fs::write(&source, "content").unwrap();

        let t1 = dir.path().join("managed.txt");
        assert!(apply_file(&source, &t1, FileStrategy::Managed).unwrap());

        let t2 = dir.path().join("template.txt");
        assert!(apply_file(&source, &t2, FileStrategy::Template).unwrap());

        let t3 = dir.path().join("merged.txt");
        assert!(apply_file(&source, &t3, FileStrategy::Merged).unwrap());
    }

    // ── is_file_outdated ──

    #[test]
    fn test_is_file_outdated_managed_identical() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "same").unwrap();
        fs::write(&target, "same").unwrap();

        assert!(!is_file_outdated(&source, &target, FileStrategy::Managed));
    }

    #[test]
    fn test_is_file_outdated_managed_different() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "new").unwrap();
        fs::write(&target, "old").unwrap();

        assert!(is_file_outdated(&source, &target, FileStrategy::Managed));
    }

    #[test]
    fn test_is_file_outdated_managed_target_missing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "content").unwrap();

        assert!(is_file_outdated(&source, &target, FileStrategy::Managed));
    }

    #[test]
    fn test_is_file_outdated_template_always_false() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "new").unwrap();
        fs::write(&target, "old").unwrap();

        assert!(!is_file_outdated(&source, &target, FileStrategy::Template));
    }

    #[test]
    fn test_is_file_outdated_merged_missing_lines() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "a\nb\nc\n").unwrap();
        fs::write(&target, "a\n").unwrap();

        assert!(is_file_outdated(&source, &target, FileStrategy::Merged));
    }

    #[test]
    fn test_is_file_outdated_merged_all_present() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "a\nb\n").unwrap();
        fs::write(&target, "a\nb\nextra\n").unwrap();

        assert!(!is_file_outdated(&source, &target, FileStrategy::Merged));
    }

    // ── diff_file ──

    #[test]
    fn test_diff_file_identical() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "same\n").unwrap();
        fs::write(&target, "same\n").unwrap();

        assert!(diff_file(&source, &target).is_none());
    }

    #[test]
    fn test_diff_file_different() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "new line\n").unwrap();
        fs::write(&target, "old line\n").unwrap();

        let diff = diff_file(&source, &target).unwrap();
        assert!(diff.contains("-old line"));
        assert!(diff.contains("+new line"));
    }

    #[test]
    fn test_diff_file_target_missing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "content\n").unwrap();

        let diff = diff_file(&source, &target).unwrap();
        assert!(diff.contains("+content"));
    }

    // ── Language detection ──

    fn make_global_manifest() -> GlobalManifest {
        GlobalManifest {
            meta: ManifestMeta { version: 1 },
            languages: vec![
                LanguageEntry {
                    id: "rust".to_string(),
                    name: "Rust".to_string(),
                    markers: vec!["Cargo.toml".to_string()],
                    frameworks: vec!["axum".to_string(), "clap".to_string()],
                },
                LanguageEntry {
                    id: "ts".to_string(),
                    name: "TypeScript".to_string(),
                    markers: vec!["package.json".to_string()],
                    frameworks: vec!["nextjs".to_string(), "nestjs".to_string()],
                },
                LanguageEntry {
                    id: "python".to_string(),
                    name: "Python".to_string(),
                    markers: vec!["pyproject.toml".to_string(), "requirements.txt".to_string()],
                    frameworks: vec!["fastapi".to_string()],
                },
                LanguageEntry {
                    id: "dart".to_string(),
                    name: "Dart".to_string(),
                    markers: vec!["pubspec.yaml".to_string()],
                    frameworks: vec!["flutter".to_string()],
                },
                LanguageEntry {
                    id: "c".to_string(),
                    name: "C".to_string(),
                    markers: vec!["Makefile".to_string(), "*.c".to_string()],
                    frameworks: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn test_detect_language_rust() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let manifest = make_global_manifest();
        assert_eq!(
            detect_language(dir.path(), &manifest),
            Some("rust".to_string())
        );
    }

    #[test]
    fn test_detect_language_typescript() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let manifest = make_global_manifest();
        assert_eq!(
            detect_language(dir.path(), &manifest),
            Some("ts".to_string())
        );
    }

    #[test]
    fn test_detect_language_python_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "[tool]").unwrap();

        let manifest = make_global_manifest();
        assert_eq!(
            detect_language(dir.path(), &manifest),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_language_python_requirements() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("requirements.txt"), "flask").unwrap();

        let manifest = make_global_manifest();
        assert_eq!(
            detect_language(dir.path(), &manifest),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_language_c_glob() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.c"), "int main(){}").unwrap();

        let manifest = make_global_manifest();
        assert_eq!(
            detect_language(dir.path(), &manifest),
            Some("c".to_string())
        );
    }

    #[test]
    fn test_detect_language_none() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("readme.md"), "# Hello").unwrap();

        let manifest = make_global_manifest();
        assert_eq!(detect_language(dir.path(), &manifest), None);
    }

    // ── Framework detection ──

    #[test]
    fn test_detect_framework_rust_axum() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[dependencies]\naxum = \"0.7\"\ntokio = \"1\"\n",
        )
        .unwrap();

        assert_eq!(
            detect_framework(dir.path(), "rust"),
            Some("axum".to_string())
        );
    }

    #[test]
    fn test_detect_framework_rust_clap() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[dependencies]\nclap = { version = \"4\", features = [\"derive\"] }\n",
        )
        .unwrap();

        assert_eq!(
            detect_framework(dir.path(), "rust"),
            Some("clap".to_string())
        );
    }

    #[test]
    fn test_detect_framework_rust_none() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();

        assert_eq!(detect_framework(dir.path(), "rust"), None);
    }

    #[test]
    fn test_detect_framework_ts_nextjs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("next.config.js"), "module.exports = {}").unwrap();

        assert_eq!(
            detect_framework(dir.path(), "ts"),
            Some("nextjs".to_string())
        );
    }

    #[test]
    fn test_detect_framework_ts_nextjs_mjs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("next.config.mjs"), "export default {}").unwrap();

        assert_eq!(
            detect_framework(dir.path(), "ts"),
            Some("nextjs".to_string())
        );
    }

    #[test]
    fn test_detect_framework_ts_nestjs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("nest-cli.json"), "{}").unwrap();

        assert_eq!(
            detect_framework(dir.path(), "ts"),
            Some("nestjs".to_string())
        );
    }

    #[test]
    fn test_detect_framework_python_fastapi() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\ndependencies = [\"fastapi\"]\n",
        )
        .unwrap();

        assert_eq!(
            detect_framework(dir.path(), "python"),
            Some("fastapi".to_string())
        );
    }

    #[test]
    fn test_detect_framework_dart_flutter() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pubspec.yaml"),
            "dependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();

        assert_eq!(
            detect_framework(dir.path(), "dart"),
            Some("flutter".to_string())
        );
    }

    #[test]
    fn test_detect_framework_unknown_language() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_framework(dir.path(), "go"), None);
    }

    // ── Config manifest loading ──

    fn setup_config_repo(dir: &Path) {
        // rust/common/
        let common = dir.join("rust").join("common");
        fs::create_dir_all(&common).unwrap();
        fs::write(
            common.join("manifest.yaml"),
            "files:\n  - path: rustfmt.toml\n    strategy: managed\n  - path: .gitignore\n    strategy: merged\n",
        ).unwrap();
        fs::write(common.join("rustfmt.toml"), "edition = \"2021\"").unwrap();
        fs::write(common.join(".gitignore"), "/target\n").unwrap();

        // rust/axum/
        let axum = dir.join("rust").join("axum");
        fs::create_dir_all(&axum).unwrap();
        fs::write(
            axum.join("manifest.yaml"),
            "files:\n  - path: Dockerfile\n    strategy: template\n",
        )
        .unwrap();
        fs::write(axum.join("Dockerfile"), "FROM rust:1.78").unwrap();

        // shared/
        let shared = dir.join("shared");
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            shared.join("manifest.yaml"),
            "files:\n  - path: .editorconfig\n    strategy: managed\n",
        )
        .unwrap();
        fs::write(shared.join(".editorconfig"), "root = true").unwrap();

        // shared/ci/
        let ci = shared.join("ci");
        fs::create_dir_all(&ci).unwrap();
        fs::write(ci.join("ci.yml"), "name: CI").unwrap();
    }

    #[test]
    fn test_load_config_manifest_common_only() {
        let dir = TempDir::new().unwrap();
        setup_config_repo(dir.path());

        let files = load_config_manifest(dir.path(), "rust", None).unwrap();
        assert_eq!(files.len(), 2); // rustfmt.toml + .gitignore
        assert!(
            files
                .iter()
                .any(|(_, e)| e.path == "rustfmt.toml" && e.strategy == FileStrategy::Managed)
        );
        assert!(
            files
                .iter()
                .any(|(_, e)| e.path == ".gitignore" && e.strategy == FileStrategy::Merged)
        );
    }

    #[test]
    fn test_load_config_manifest_with_framework() {
        let dir = TempDir::new().unwrap();
        setup_config_repo(dir.path());

        let files = load_config_manifest(dir.path(), "rust", Some("axum")).unwrap();
        assert_eq!(files.len(), 3); // common(2) + axum(1)
        assert!(
            files
                .iter()
                .any(|(_, e)| e.path == "Dockerfile" && e.strategy == FileStrategy::Template)
        );
    }

    #[test]
    fn test_load_config_manifest_nonexistent_language() {
        let dir = TempDir::new().unwrap();
        setup_config_repo(dir.path());

        let files = load_config_manifest(dir.path(), "go", None).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_shared_files_no_includes() {
        let dir = TempDir::new().unwrap();
        setup_config_repo(dir.path());

        let files = collect_shared_files(dir.path(), &[]).unwrap();
        assert_eq!(files.len(), 1); // .editorconfig only (from manifest)
    }

    #[test]
    fn test_collect_shared_files_with_ci() {
        let dir = TempDir::new().unwrap();
        setup_config_repo(dir.path());

        let includes = vec!["ci".to_string()];
        let files = collect_shared_files(dir.path(), &includes).unwrap();
        assert!(files.len() >= 2); // .editorconfig + ci.yml
        assert!(files.iter().any(|(_, e)| e.path.contains("ci.yml")));
    }

    #[test]
    fn test_collect_all_source_files() {
        let dir = TempDir::new().unwrap();
        setup_config_repo(dir.path());

        let config = ProjConfig {
            language: "rust".to_string(),
            framework: Some("axum".to_string()),
            config_version: String::new(),
            includes: vec!["ci".to_string()],
        };
        let files = collect_all_source_files(dir.path(), &config).unwrap();
        // common(2) + axum(1) + shared(1) + ci(1)
        assert!(files.len() >= 4);
    }

    // ── Global manifest ──

    #[test]
    fn test_load_global_manifest() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("manifest.yaml"),
            "meta:\n  version: 1\nlanguages:\n  - id: rust\n    name: Rust\n    markers: [Cargo.toml]\n    frameworks: [axum]\n",
        ).unwrap();

        let manifest = load_global_manifest(dir.path()).unwrap();
        assert_eq!(manifest.languages.len(), 1);
        assert_eq!(manifest.languages[0].id, "rust");
        assert_eq!(manifest.languages[0].frameworks, vec!["axum"]);
    }

    #[test]
    fn test_load_global_manifest_missing() {
        let dir = TempDir::new().unwrap();
        let result = load_global_manifest(dir.path());
        assert!(result.is_err());
    }

    // ── End-to-end: init-like flow ──

    #[test]
    fn test_init_flow_applies_files_correctly() {
        let repo = TempDir::new().unwrap();
        setup_config_repo(repo.path());

        let project = TempDir::new().unwrap();
        fs::write(project.path().join(".gitignore"), "*.log\n").unwrap();

        let config = ProjConfig {
            language: "rust".to_string(),
            framework: Some("axum".to_string()),
            config_version: String::new(),
            includes: Vec::new(),
        };
        let files = collect_all_source_files(repo.path(), &config).unwrap();

        for (source, entry) in &files {
            let target = project.path().join(&entry.path);
            apply_file(source, &target, entry.strategy).unwrap();
        }

        // Managed: created
        assert_eq!(
            fs::read_to_string(project.path().join("rustfmt.toml")).unwrap(),
            "edition = \"2021\""
        );
        // Merged: existing content preserved + new content added
        let gitignore = fs::read_to_string(project.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("*.log")); // original preserved
        assert!(gitignore.contains("/target")); // merged from source
        // Template: created
        assert_eq!(
            fs::read_to_string(project.path().join("Dockerfile")).unwrap(),
            "FROM rust:1.78"
        );
        // Shared: created
        assert_eq!(
            fs::read_to_string(project.path().join(".editorconfig")).unwrap(),
            "root = true"
        );
    }

    #[test]
    fn test_sync_flow_skips_templates_updates_managed() {
        let repo = TempDir::new().unwrap();
        setup_config_repo(repo.path());

        let project = TempDir::new().unwrap();

        // Simulate initial init
        let config = ProjConfig {
            language: "rust".to_string(),
            framework: Some("axum".to_string()),
            config_version: String::new(),
            includes: Vec::new(),
        };
        let files = collect_all_source_files(repo.path(), &config).unwrap();
        for (source, entry) in &files {
            apply_file(source, &project.path().join(&entry.path), entry.strategy).unwrap();
        }

        // User modifies template file
        fs::write(project.path().join("Dockerfile"), "FROM rust:1.80-custom").unwrap();
        // Upstream updates managed file
        fs::write(
            repo.path().join("rust/common/rustfmt.toml"),
            "edition = \"2024\"",
        )
        .unwrap();

        // Sync: only managed/merged should update, template should be left alone
        let files = collect_all_source_files(repo.path(), &config).unwrap();
        for (source, entry) in &files {
            if entry.strategy == FileStrategy::Template {
                assert!(!is_file_outdated(
                    source,
                    &project.path().join(&entry.path),
                    entry.strategy
                ));
                continue;
            }
            let target = project.path().join(&entry.path);
            if is_file_outdated(source, &target, entry.strategy) {
                apply_file(source, &target, entry.strategy).unwrap();
            }
        }

        // Template untouched
        assert_eq!(
            fs::read_to_string(project.path().join("Dockerfile")).unwrap(),
            "FROM rust:1.80-custom"
        );
        // Managed updated
        assert_eq!(
            fs::read_to_string(project.path().join("rustfmt.toml")).unwrap(),
            "edition = \"2024\""
        );
    }

    // ── has_files_with_extension ──

    #[test]
    fn test_has_files_with_extension_found() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.c"), "").unwrap();
        fs::write(dir.path().join("readme.md"), "").unwrap();

        assert!(has_files_with_extension(dir.path(), "c"));
    }

    #[test]
    fn test_has_files_with_extension_not_found() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("readme.md"), "").unwrap();

        assert!(!has_files_with_extension(dir.path(), "c"));
    }

    #[test]
    fn test_has_files_with_extension_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert!(!has_files_with_extension(dir.path(), "c"));
    }
}
