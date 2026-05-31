use crate::reconcile::{classify, repairable_target, scan_base_root, ConflictKind};
use crate::state::{load_state, project_path, project_path_display};
use anyhow::Result;
use colored::Colorize;

/// Read-only diagnostic for drift between the manifest (desired state) and
/// the filesystem (observed state).
///
/// Reports four classes (see `reconcile::ReconcileReport`):
/// - **orphans** — manifest entries whose directory is missing on disk
///   (`missing-restorable` if a remote is recorded, else `missing-unrestorable`)
/// - **conflicts** — manifest entries whose on-disk state disagrees
///   (`path-conflict`, `remote-mismatch`)
/// - **untracked git** — directories at the convention path with no manifest entry
/// - **untracked non-git** — informational; never adopt-eligible
///
/// Makes zero filesystem or manifest changes — mutating actions belong on `pm sync`.
pub fn run() -> Result<()> {
    let (config, manifest) = load_state()?;
    let disk = scan_base_root(&config);
    let report = classify(&config, &manifest, &disk);

    let mut issues = 0;

    // Healthy + per-project diagnostics. We loop the manifest in declared
    // order so the output stays stable; classify() told us which entries
    // need attention.
    let orphan_names: std::collections::HashSet<&str> =
        report.orphans.iter().map(|p| p.name.as_str()).collect();
    let conflict_names: std::collections::HashMap<&str, ConflictKind> = report
        .conflicts
        .iter()
        .map(|c| (c.project.name.as_str(), c.kind))
        .collect();

    for project in &manifest.projects {
        let path_display = project_path_display(&config, &manifest, project)?;
        let name = project.name.as_str();

        if orphan_names.contains(name) {
            if project.remote.is_some() {
                println!(
                    "{} {:<16} {} ({})",
                    "✗".yellow(),
                    project.name,
                    path_display,
                    "missing-restorable".yellow()
                );
            } else {
                println!(
                    "{} {:<16} {} ({})",
                    "✗".red(),
                    project.name,
                    path_display,
                    "missing-unrestorable".red()
                );
            }
            issues += 1;
            continue;
        }

        if let Some(kind) = conflict_names.get(name).copied() {
            let label = match kind {
                ConflictKind::NonGitAtExpectedPath => "path-conflict",
                ConflictKind::RemoteMismatch => "remote-mismatch",
                ConflictKind::BrokenLink => "broken-link",
                ConflictKind::LinkTargetDrift => "link-drift",
                ConflictKind::LinkReplacedByDir => "link-replaced-by-dir",
                ConflictKind::UnexpectedSymlink => "unexpected-symlink",
            };
            println!(
                "{} {:<16} {} ({})",
                "✗".red(),
                project.name,
                path_display,
                label.red()
            );
            // Extra detail for link-specific diagnostics, including whether the
            // condition is repairable by `pm sync` (recorded target exists).
            let repairable = repairable_target(project).is_some();
            match kind {
                ConflictKind::BrokenLink => {
                    if let Some(target) = &project.link {
                        if repairable {
                            println!(
                                "    → target {} exists; repairable — run `pm sync`",
                                target.dimmed()
                            );
                        } else {
                            println!(
                                "    → links to {} (unavailable on this machine)",
                                target.dimmed()
                            );
                        }
                    }
                }
                ConflictKind::LinkTargetDrift => {
                    let expected = project.link.as_deref().unwrap_or("?").to_string();
                    let found = project_path(&config, &manifest, project)
                        .ok()
                        .and_then(|p| std::fs::read_link(p).ok())
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "?".to_string());
                    println!(
                        "    → expected {} but resolves to {}",
                        expected.dimmed(),
                        found.dimmed()
                    );
                    if repairable {
                        println!("      repairable — run `pm sync`");
                    }
                }
                _ => {}
            }
            issues += 1;
            continue;
        }

        // Project may live in a system workspace (e.g. .trash) we don't scan;
        // classify intentionally doesn't surface those as orphans. Mark them OK.
        println!("{} {:<16} {}", "✓".green(), project.name, path_display);
    }

    if !report.untracked_git.is_empty() {
        println!("\nUntracked git directories (not registered in manifest):");
        for entry in &report.untracked_git {
            println!(
                "{} {}/{:<16} {} ({})",
                "?".cyan(),
                entry.workspace,
                entry.dir,
                entry.path.display(),
                "untracked".cyan()
            );
            issues += 1;
        }
        println!("\n  Run `pm sync --adopt` to register them, or ignore.");
    }

    if !report.untracked_non_git.is_empty() {
        println!("\nUntracked non-git directories (informational):");
        for entry in &report.untracked_non_git {
            println!(
                "{} {}/{:<16} {} ({})",
                "·".dimmed(),
                entry.workspace,
                entry.dir,
                entry.path.display(),
                "untracked-non-git".dimmed()
            );
        }
    }

    if issues > 0 {
        println!("\n{} issues found.", issues.to_string().red());
    } else {
        println!("\nAll projects are healthy.");
    }

    Ok(())
}
