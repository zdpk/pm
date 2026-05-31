//! Drift detection between the manifest (desired state) and the filesystem
//! (observed state) under the strict path convention.
//!
//! Strict v4 paths (`<base_root>/<workspace>/<project.dir>`) make the universe
//! of "where a project should live" enumerable: a two-level scan of
//! `base_root` returns every candidate. This module is the read-only engine —
//! it scans, classifies, and helps callers (`pm check`, `pm sync`) apply
//! repairs. It never mutates state on its own.
//!
//! Source-of-truth split (D8 of strict-path-convention):
//! - the manifest declares **what** `pm` manages,
//! - the filesystem owns **whether** a directory physically exists,
//! - and drift between the two is surfaced here.

use crate::error::PmError;
use crate::git::{get_remote_url, is_git_repo, remote_matches, repo_slug_from_remote};
use crate::models::{
    Config, HistoryAction, HistoryData, HistoryEntry, HistoryProjectSnapshot, Manifest, Project,
};
use crate::path::{collapse_path, expand_path, validate_project_dir};
use chrono::Utc;
use regex::Regex;
use std::path::PathBuf;

/// A directory (or project-level symlink) observed under
/// `<base_root>/<workspace>/<dir>`.
#[derive(Debug, Clone)]
pub struct DiskEntry {
    pub workspace: String,
    pub dir: String,
    pub path: PathBuf,
    pub is_git: bool,
    pub remote: Option<String>,
    /// When the convention path is a symlink, the canonicalized target it
    /// resolves to (`None` if the target is missing/dangling). `None` together
    /// with `is_symlink == false` means a plain directory.
    pub link_target: Option<PathBuf>,
    /// Whether the convention-path entry itself is a symlink node.
    pub is_symlink: bool,
}

/// A manifest project whose physical directory exists but disagrees with the
/// manifest in some way (non-git, remote mismatch).
#[derive(Debug, Clone)]
pub struct ConflictEntry<'a> {
    pub project: &'a Project,
    pub path: PathBuf,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// The convention path exists but is not a git repository.
    NonGitAtExpectedPath,
    /// The convention path is a git repo but `origin` doesn't match the
    /// manifest's recorded remote.
    RemoteMismatch,
    /// A linked project whose symlink is missing or whose target no longer
    /// exists (also the "unavailable on this machine" case).
    BrokenLink,
    /// A linked project whose symlink resolves to a path other than the
    /// recorded target.
    LinkTargetDrift,
    /// A linked project whose convention path is now a real directory instead
    /// of a symlink.
    LinkReplacedByDir,
    /// A managed (non-linked) project whose convention path is a symlink.
    UnexpectedSymlink,
}

/// Classification of drift between the manifest and the disk scan.
#[derive(Debug, Default)]
pub struct ReconcileReport<'a> {
    /// Projects in the manifest whose directory does not exist on disk.
    pub orphans: Vec<&'a Project>,
    /// Git directories under `base_root` with no matching manifest entry.
    pub untracked_git: Vec<DiskEntry>,
    /// Non-git directories under `base_root` with no matching manifest entry.
    /// Informational; never adopt-eligible.
    pub untracked_non_git: Vec<DiskEntry>,
    /// Manifest entries whose on-disk state disagrees with the manifest.
    pub conflicts: Vec<ConflictEntry<'a>>,
}

/// Walk two levels of `base_root` and return every candidate project entry.
/// Workspace-level (first-level) symlinks are skipped (avoid recursion and
/// cross-fs surprises); project-level (second-level) symlinks ARE recorded so
/// linked projects can be reconciled. Non-directory files and system
/// workspaces (`.`-prefix) — whose contents are tracked-but-trashed, not
/// "untracked" — are skipped.
pub fn scan_base_root(config: &Config) -> Vec<DiskEntry> {
    let base = expand_path(&config.base_root);
    let mut entries = Vec::new();

    let Ok(ws_iter) = std::fs::read_dir(&base) else {
        return entries;
    };

    for ws_entry in ws_iter.flatten() {
        let Ok(ws_meta) = ws_entry.metadata() else {
            continue;
        };
        if ws_meta.file_type().is_symlink() || !ws_meta.is_dir() {
            continue;
        }

        let Some(ws_name) = ws_entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        // System workspaces (.trash etc.) are scanned by the rest of pm but
        // their contents are not "untracked" — they're tracked-but-trashed.
        if ws_name.starts_with('.') {
            continue;
        }

        let Ok(project_iter) = std::fs::read_dir(ws_entry.path()) else {
            continue;
        };

        for project_entry in project_iter.flatten() {
            // `DirEntry::metadata` does not traverse symlinks, so this detects
            // the link node itself rather than its target.
            let Ok(pe_meta) = project_entry.metadata() else {
                continue;
            };
            let is_symlink = pe_meta.file_type().is_symlink();
            // Plain files (neither a directory nor a symlink) are not projects.
            if !is_symlink && !pe_meta.is_dir() {
                continue;
            }
            let Some(dir_name) = project_entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };

            let path = project_entry.path();

            // Resolve a project-level symlink to its canonical target. A
            // dangling link resolves to `None`. Git-ness/remote are computed
            // from the resolved location (the target for a symlink, the
            // directory itself otherwise).
            let (link_target, resolved) = if is_symlink {
                match std::fs::canonicalize(&path) {
                    Ok(target) => (Some(target.clone()), Some(target)),
                    Err(_) => (None, None),
                }
            } else {
                (None, Some(path.clone()))
            };

            let (is_git, remote) = match resolved {
                Some(ref real) if real.is_dir() => {
                    let real_str = real.display().to_string();
                    if is_git_repo(&real_str) {
                        (true, get_remote_url(&real_str))
                    } else {
                        (false, None)
                    }
                }
                _ => (false, None),
            };

            entries.push(DiskEntry {
                workspace: ws_name.clone(),
                dir: dir_name,
                path,
                is_git,
                remote,
                link_target,
                is_symlink,
            });
        }
    }

    entries
}

/// Classify the disk scan against the manifest into orphan / untracked /
/// conflict buckets. Pure function — no I/O beyond what callers already did
/// to build `disk`.
pub fn classify<'a>(
    config: &Config,
    manifest: &'a Manifest,
    disk: &[DiskEntry],
) -> ReconcileReport<'a> {
    let mut report = ReconcileReport::default();

    // Index disk entries by (workspace, dir) for O(1) lookup.
    use std::collections::HashMap;
    let disk_index: HashMap<(&str, &str), &DiskEntry> = disk
        .iter()
        .map(|d| ((d.workspace.as_str(), d.dir.as_str()), d))
        .collect();

    // Manifest-driven pass: orphans + conflicts. Linked projects (those with
    // a recorded link target) follow a separate set of rules — they are never
    // orphans.
    for project in &manifest.projects {
        let observed = disk_index.get(&(project.workspace.as_str(), project.dir.as_str()));

        if let Some(target) = project.link.as_deref() {
            classify_linked(&mut report, config, project, observed.copied(), target);
            continue;
        }

        match observed {
            None => {
                // No disk entry at the convention path. Either the dir was
                // deleted out-of-band, or the project is in a system
                // workspace whose contents we don't scan (e.g. .trash).
                // Only flag as orphan when the workspace is non-system:
                // trashed projects intentionally live there.
                if !project.workspace.starts_with('.') {
                    report.orphans.push(project);
                }
            }
            Some(entry) => {
                if entry.is_symlink {
                    // A managed project should be a real directory; a symlink
                    // here is unexpected drift.
                    report.conflicts.push(ConflictEntry {
                        project,
                        path: entry.path.clone(),
                        kind: ConflictKind::UnexpectedSymlink,
                    });
                } else if !entry.is_git {
                    report.conflicts.push(ConflictEntry {
                        project,
                        path: entry.path.clone(),
                        kind: ConflictKind::NonGitAtExpectedPath,
                    });
                } else if let Some(expected) = project.remote.as_deref() {
                    if !remote_matches(&entry.path, expected) {
                        report.conflicts.push(ConflictEntry {
                            project,
                            path: entry.path.clone(),
                            kind: ConflictKind::RemoteMismatch,
                        });
                    }
                }
            }
        }
    }

    // Disk-driven pass: untracked entries.
    let manifest_index: std::collections::HashSet<(&str, &str)> = manifest
        .projects
        .iter()
        .map(|p| (p.workspace.as_str(), p.dir.as_str()))
        .collect();

    for entry in disk {
        if manifest_index.contains(&(entry.workspace.as_str(), entry.dir.as_str())) {
            continue;
        }
        // An untracked symlink has no manifest entry — pm didn't create it and
        // it is never adopt-eligible. Leave it alone rather than treating it as
        // an adoptable git directory.
        if entry.is_symlink {
            continue;
        }
        if entry.is_git {
            report.untracked_git.push(entry.clone());
        } else {
            report.untracked_non_git.push(entry.clone());
        }
    }

    report
}

/// Classify a single linked project (one with a recorded link target) against
/// its observed convention-path entry. A linked project is never an orphan;
/// its outcomes are healthy (no report) or one of the link-specific conflicts.
fn classify_linked<'a>(
    report: &mut ReconcileReport<'a>,
    config: &Config,
    project: &'a Project,
    observed: Option<&DiskEntry>,
    target: &str,
) {
    let push = |report: &mut ReconcileReport<'a>, path: PathBuf, kind: ConflictKind| {
        report.conflicts.push(ConflictEntry {
            project,
            path,
            kind,
        });
    };

    match observed {
        // Nothing at the convention path: the symlink is gone, or the target
        // (and link) simply don't exist on this machine. Either way it's a
        // broken link, never an orphan.
        None => {
            let path = expand_path(&config.base_root)
                .join(&project.workspace)
                .join(&project.dir);
            push(report, path, ConflictKind::BrokenLink);
        }
        Some(entry) => {
            if !entry.is_symlink {
                // A real directory (or file) sits where the link should be.
                push(report, entry.path.clone(), ConflictKind::LinkReplacedByDir);
            } else {
                match entry.link_target.as_deref() {
                    // Dangling symlink — target missing.
                    None => push(report, entry.path.clone(), ConflictKind::BrokenLink),
                    Some(resolved) if resolved == std::path::Path::new(target) => {
                        // Healthy: symlink resolves to the recorded target.
                    }
                    Some(_) => push(report, entry.path.clone(), ConflictKind::LinkTargetDrift),
                }
            }
        }
    }
}

/// Reason an `adopt_one` call refused to act. The caller decides how to
/// surface it (warning + skip vs. fatal).
#[derive(Debug)]
pub enum AdoptSkip {
    /// Inferred name (the directory name) doesn't satisfy the project-name regex.
    InvalidName { dir: String },
    /// Inferred name passed the regex but failed `validate_project_dir`.
    InvalidDir { dir: String, rule: &'static str },
    /// The parent workspace isn't declared in the manifest. Adopt doesn't
    /// create workspaces; the user must `pm ws new` first.
    UnknownWorkspace { workspace: String },
    /// A project with the same name already exists in the manifest.
    NameCollision { existing_workspace: String },
}

impl std::fmt::Display for AdoptSkip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName { dir } => write!(
                f,
                "directory name '{dir}' is not a valid project name (must match [a-zA-Z][a-zA-Z0-9_-]*)"
            ),
            Self::InvalidDir { dir, rule } => write!(f, "dir '{dir}' invalid: {rule}"),
            Self::UnknownWorkspace { workspace } => write!(
                f,
                "workspace '{workspace}' is not declared in the manifest; run `pm ws new {workspace}` first"
            ),
            Self::NameCollision { existing_workspace } => write!(
                f,
                "a project with this name already exists in workspace '{existing_workspace}'"
            ),
        }
    }
}

/// Validate an untracked git directory against the path convention and the
/// project-name regex, and build a `Project` ready to be pushed into the
/// manifest. Pure — does not mutate the manifest.
pub fn adopt_one(manifest: &Manifest, entry: &DiskEntry) -> Result<Project, AdoptSkip> {
    // Name rules: same regex pm add applies to user-supplied names.
    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").expect("regex compiles");
    if !name_regex.is_match(&entry.dir) {
        return Err(AdoptSkip::InvalidName {
            dir: entry.dir.clone(),
        });
    }
    if let Err(e) = validate_project_dir(&entry.dir) {
        // PmError::InvalidProjectDir carries the rule string; surface it.
        if let PmError::InvalidProjectDir { rule, .. } = e {
            return Err(AdoptSkip::InvalidDir {
                dir: entry.dir.clone(),
                rule,
            });
        }
    }

    // Workspace must be declared in the manifest. Adopt doesn't auto-create.
    if !manifest
        .workspaces
        .iter()
        .any(|w| w.name == entry.workspace)
    {
        return Err(AdoptSkip::UnknownWorkspace {
            workspace: entry.workspace.clone(),
        });
    }

    // Name collision check across the whole manifest (project names are global).
    if let Some(existing) = manifest.projects.iter().find(|p| p.name == entry.dir) {
        return Err(AdoptSkip::NameCollision {
            existing_workspace: existing.workspace.clone(),
        });
    }

    let repo_slug = entry
        .remote
        .as_deref()
        .and_then(repo_slug_from_remote)
        .unwrap_or_else(|| entry.dir.clone());

    let mut project = Project::new(
        entry.dir.clone(),
        entry.workspace.clone(),
        repo_slug,
        entry.dir.clone(),
    );
    project.remote = entry.remote.clone();
    Ok(project)
}

/// Remove a project from the manifest and append a snapshot to history.
/// Caller is responsible for confirmation and for the final `save_state` /
/// `save_history`.
pub fn prune_one(
    config: &Config,
    manifest: &mut Manifest,
    history: &mut HistoryData,
    project_name: &str,
) -> Result<(), PmError> {
    let idx = manifest
        .projects
        .iter()
        .position(|p| p.name == project_name)
        .ok_or_else(|| PmError::ProjectNotFound(project_name.to_string()))?;
    let project = manifest.projects.remove(idx);

    // Compute the convention path snapshot. We can't reuse
    // state::project_path here because we already removed the project from
    // the manifest; reconstruct the path manually.
    let path = collapse_path(
        &expand_path(&config.base_root)
            .join(&project.workspace)
            .join(&project.dir),
    );

    history.entries.push(HistoryEntry {
        timestamp: Utc::now(),
        action: HistoryAction::Unregistered,
        project: HistoryProjectSnapshot {
            name: project.name,
            workspace: project.workspace,
            repo_slug: project.repo_slug,
            dir: project.dir,
            remote: project.remote,
            path,
        },
    });
    Ok(())
}

/// The repairability predicate for a linked project: returns the recorded
/// canonical target iff the project is linked AND that target currently exists
/// on disk. A broken/drifted link is repairable exactly when its target is
/// present (recreating the symlink would then resolve); a missing target (the
/// cross-machine case) is not repairable.
pub fn repairable_target(project: &Project) -> Option<PathBuf> {
    let target = PathBuf::from(project.link.as_deref()?);
    target.exists().then_some(target)
}

/// Recreate (or correct) a linked project's symlink so `convention` resolves
/// to `target`. Touches only the pointer:
/// - if a real (non-symlink) entry occupies `convention`, refuse — removing it
///   could destroy data (`LinkReplacedByDir` stays manual);
/// - if a stale symlink node is present (drift), remove just that node;
/// - then create the symlink.
///
/// Caller is responsible for confirmation and for ensuring `target` exists
/// (see [`repairable_target`]).
pub fn repair_one(target: &std::path::Path, convention: &std::path::Path) -> anyhow::Result<()> {
    match convention.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Stale/drifted symlink node — remove just the pointer.
            std::fs::remove_file(convention)?;
        }
        Ok(_) => {
            // A real directory or file occupies the path; never delete it.
            return Err(PmError::LinkPathOccupied(convention.display().to_string()).into());
        }
        Err(_) => { /* nothing there — create fresh below */ }
    }

    crate::path::create_symlink(target, convention)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Project, Workspace};
    use chrono::Utc;
    use std::collections::HashMap;

    fn proj(name: &str, workspace: &str, remote: Option<&str>) -> Project {
        let mut p = Project::new(
            name.to_string(),
            workspace.to_string(),
            name.to_string(),
            name.to_string(),
        );
        p.remote = remote.map(str::to_string);
        p
    }

    fn linked(name: &str, workspace: &str, target: &str) -> Project {
        let mut p = proj(name, workspace, None);
        p.link = Some(target.to_string());
        p
    }

    fn workspace(name: &str) -> Workspace {
        Workspace {
            name: name.to_string(),
            created_at: Utc::now(),
            git: HashMap::new(),
        }
    }

    fn config_with_base(base: &str) -> Config {
        Config {
            base_root: base.to_string(),
            ..Config::default()
        }
    }

    fn disk(workspace: &str, dir: &str, is_git: bool, remote: Option<&str>) -> DiskEntry {
        DiskEntry {
            workspace: workspace.to_string(),
            dir: dir.to_string(),
            path: PathBuf::from(format!("/fake/{workspace}/{dir}")),
            is_git,
            remote: remote.map(str::to_string),
            link_target: None,
            is_symlink: false,
        }
    }

    /// A symlink disk entry resolving to `target` (or dangling when `None`).
    fn disk_link(workspace: &str, dir: &str, target: Option<&str>) -> DiskEntry {
        DiskEntry {
            workspace: workspace.to_string(),
            dir: dir.to_string(),
            path: PathBuf::from(format!("/fake/{workspace}/{dir}")),
            is_git: target.is_some(),
            remote: None,
            link_target: target.map(PathBuf::from),
            is_symlink: true,
        }
    }

    #[test]
    fn classify_finds_orphans_for_manifest_entries_missing_on_disk() {
        let manifest = Manifest {
            version: 4,
            workspaces: vec![workspace("default")],
            projects: vec![proj("ghost", "default", Some("git@x:y/ghost.git"))],
        };
        let report = classify(&config_with_base("/projects"), &manifest, &[]);
        assert_eq!(report.orphans.len(), 1);
        assert_eq!(report.orphans[0].name, "ghost");
        assert!(report.untracked_git.is_empty());
    }

    #[test]
    fn classify_skips_orphans_in_system_workspaces() {
        let manifest = Manifest {
            version: 4,
            workspaces: vec![workspace(".trash")],
            projects: vec![proj("old", ".trash", None)],
        };
        let report = classify(&config_with_base("/projects"), &manifest, &[]);
        assert!(
            report.orphans.is_empty(),
            "trash projects are not orphans even when their dir is not scanned"
        );
    }

    #[test]
    fn classify_finds_untracked_git_dirs() {
        let manifest = Manifest {
            version: 4,
            workspaces: vec![workspace("acme")],
            projects: vec![],
        };
        let disk = vec![disk("acme", "web", true, Some("git@x:y/web.git"))];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert_eq!(report.untracked_git.len(), 1);
        assert_eq!(report.untracked_git[0].dir, "web");
        assert!(report.untracked_non_git.is_empty());
    }

    #[test]
    fn classify_separates_untracked_non_git() {
        let manifest = Manifest {
            version: 4,
            workspaces: vec![workspace("acme")],
            projects: vec![],
        };
        let disk = vec![disk("acme", "notes", false, None)];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert!(report.untracked_git.is_empty());
        assert_eq!(report.untracked_non_git.len(), 1);
    }

    #[test]
    fn classify_finds_non_git_conflict() {
        let manifest = Manifest {
            version: 4,
            workspaces: vec![workspace("acme")],
            projects: vec![proj("web", "acme", None)],
        };
        let disk = vec![disk("acme", "web", false, None)];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, ConflictKind::NonGitAtExpectedPath);
        // The project is matched on disk (even if non-git) so it's not an orphan.
        assert!(report.orphans.is_empty());
    }

    fn manifest_with(projects: Vec<Project>) -> Manifest {
        Manifest {
            version: 4,
            workspaces: vec![workspace("acme")],
            projects,
        }
    }

    #[test]
    fn classify_healthy_link_is_not_reported() {
        let manifest = manifest_with(vec![linked("widgets", "acme", "/ext/widgets")]);
        let disk = vec![disk_link("acme", "widgets", Some("/ext/widgets"))];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert!(report.orphans.is_empty());
        assert!(
            report.conflicts.is_empty(),
            "a link resolving to its recorded target is healthy"
        );
    }

    #[test]
    fn classify_missing_link_is_broken_not_orphan() {
        let manifest = manifest_with(vec![linked("widgets", "acme", "/ext/widgets")]);
        // Nothing on disk (no symlink, target absent — also the cross-machine case).
        let report = classify(&config_with_base("/projects"), &manifest, &[]);
        assert!(
            report.orphans.is_empty(),
            "a linked project is never an orphan"
        );
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, ConflictKind::BrokenLink);
    }

    #[test]
    fn classify_dangling_symlink_is_broken_link() {
        let manifest = manifest_with(vec![linked("widgets", "acme", "/ext/widgets")]);
        let disk = vec![disk_link("acme", "widgets", None)];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, ConflictKind::BrokenLink);
    }

    #[test]
    fn classify_link_target_drift() {
        let manifest = manifest_with(vec![linked("widgets", "acme", "/ext/widgets")]);
        let disk = vec![disk_link("acme", "widgets", Some("/other/widgets"))];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, ConflictKind::LinkTargetDrift);
    }

    #[test]
    fn classify_link_replaced_by_dir() {
        let manifest = manifest_with(vec![linked("widgets", "acme", "/ext/widgets")]);
        // A real (non-symlink) directory sits at the convention path.
        let disk = vec![disk("acme", "widgets", true, None)];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, ConflictKind::LinkReplacedByDir);
    }

    #[test]
    fn classify_unexpected_symlink_for_managed_project() {
        let manifest = manifest_with(vec![proj("web", "acme", None)]);
        let disk = vec![disk_link("acme", "web", Some("/somewhere/web"))];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, ConflictKind::UnexpectedSymlink);
    }

    #[test]
    fn repairable_target_requires_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("widgets");
        std::fs::create_dir_all(&existing).unwrap();

        let p = linked("widgets", "acme", existing.to_str().unwrap());
        assert_eq!(
            repairable_target(&p).as_deref(),
            Some(existing.as_path()),
            "an existing recorded target is repairable"
        );

        let missing = linked("widgets", "acme", dir.path().join("gone").to_str().unwrap());
        assert!(
            repairable_target(&missing).is_none(),
            "a missing target is not repairable (cross-machine case)"
        );

        let unlinked = proj("web", "acme", None);
        assert!(
            repairable_target(&unlinked).is_none(),
            "a non-linked project is never repairable"
        );
    }

    #[test]
    fn classify_untracked_symlink_is_never_adopt_eligible() {
        let manifest = manifest_with(vec![]);
        // A symlink (to a git repo) with no matching manifest entry.
        let disk = vec![disk_link("acme", "extra", Some("/ext/extra"))];
        let report = classify(&config_with_base("/projects"), &manifest, &disk);
        assert!(
            report.untracked_git.is_empty(),
            "an untracked symlink must never be adopt-eligible"
        );
        assert!(report.untracked_non_git.is_empty());
        assert!(report.conflicts.is_empty());
    }
}
