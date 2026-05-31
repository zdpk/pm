use crate::config::{load_history, save_history};
use crate::error::PmError;
use crate::git::clone_repo;
use crate::reconcile::{
    adopt_one, classify, prune_one, repair_one, repairable_target, scan_base_root, AdoptSkip,
    ConflictKind, DiskEntry,
};
use crate::restore::{can_prompt, prompt_yes_no};
use crate::state::{find_workspace, load_state, project_path, save_state};
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
struct RestoreTask {
    name: String,
    remote: String,
    path: PathBuf,
}

#[derive(Clone)]
struct RepairTask {
    name: String,
    target: PathBuf,
    convention: PathBuf,
    /// `true` = correct a drifted symlink, `false` = recreate a missing one.
    drift: bool,
}

/// Reconcile the manifest (desired state) with the filesystem (observed state).
///
/// - default: restore manifest entries whose directory is missing AND have a
///   recorded remote (v0.5.0 behavior).
/// - `--adopt`: register untracked git directories under `base_root` (D8 says
///   manifest is the membership SoT — adoption is always opt-in and confirmed).
/// - `--prune`: remove manifest entries whose directory is missing and append
///   `HistoryAction::Unregistered` to history.
/// - `--dry-run`: print the plan, make no changes.
/// - non-interactive runs without `--yes` refuse to apply adopt/prune.
pub fn run(
    workspace: Option<String>,
    yes: bool,
    jobs: usize,
    adopt: bool,
    prune: bool,
    dry_run: bool,
) -> Result<()> {
    let (config, mut manifest) = load_state()?;

    if let Some(ref workspace_name) = workspace {
        let _ = find_workspace(&manifest, workspace_name)?;
    }

    let disk = scan_base_root(&config);
    let report = classify(&config, &manifest, &disk);

    // Build the restore plan (existing behavior — only orphans with a remote).
    // Scope by --workspace if provided.
    let mut restores: Vec<RestoreTask> = Vec::new();
    for project in &report.orphans {
        if workspace
            .as_ref()
            .is_some_and(|name| &project.workspace != name)
        {
            continue;
        }
        // Linked projects are machine-local: their bytes live only at the
        // external target, so there is nothing to clone. classify never marks
        // a linked project as an orphan, but guard explicitly so a stray remote
        // can never trigger a clone over a link.
        if project.is_linked() {
            continue;
        }
        if let Some(remote) = &project.remote {
            // path computed mechanically from convention.
            let path = project_path(&config, &manifest, project)?;
            restores.push(RestoreTask {
                name: project.name.clone(),
                remote: remote.clone(),
                path,
            });
        }
    }

    // Adopt plan: untracked git dirs, filtered by --workspace, validated up front.
    let mut adopts: Vec<(DiskEntry, AdoptCandidate)> = Vec::new();
    if adopt {
        for entry in &report.untracked_git {
            if workspace
                .as_ref()
                .is_some_and(|name| &entry.workspace != name)
            {
                continue;
            }
            match adopt_one(&manifest, entry) {
                Ok(project) => adopts.push((entry.clone(), AdoptCandidate::Ready(project))),
                Err(skip) => adopts.push((entry.clone(), AdoptCandidate::Skip(skip))),
            }
        }
    }

    // Prune plan: orphans (all of them, restorable or not) scoped by --workspace.
    let mut prunes: Vec<String> = Vec::new();
    if prune {
        for project in &report.orphans {
            if workspace
                .as_ref()
                .is_some_and(|name| &project.workspace != name)
            {
                continue;
            }
            prunes.push(project.name.clone());
        }
        // Restoring an entry we're about to prune would be wasted work (and
        // would clone into a directory we're declaring unmanaged). Drop any
        // pruned name from the restore plan; prune wins.
        let pruning: std::collections::HashSet<&str> = prunes.iter().map(String::as_str).collect();
        restores.retain(|task| !pruning.contains(task.name.as_str()));
    }

    // Repair plan: linked projects whose symlink is missing (broken-link) or
    // drifted, AND whose recorded target still exists. Touches only the
    // pointer; never clones. Unrecoverable linked conflicts (target missing,
    // link-replaced-by-dir) fall through to the read-only conflict report.
    let mut repairs: Vec<RepairTask> = Vec::new();
    for conflict in &report.conflicts {
        if workspace
            .as_ref()
            .is_some_and(|name| &conflict.project.workspace != name)
        {
            continue;
        }
        let drift = match conflict.kind {
            ConflictKind::BrokenLink => false,
            ConflictKind::LinkTargetDrift => true,
            _ => continue,
        };
        if let Some(target) = repairable_target(conflict.project) {
            repairs.push(RepairTask {
                name: conflict.project.name.clone(),
                target,
                convention: conflict.path.clone(),
                drift,
            });
        }
    }
    let repairing: std::collections::HashSet<&str> =
        repairs.iter().map(|t| t.name.as_str()).collect();

    // Also report orphans with no remote when the user didn't ask for prune —
    // this preserves the v0.5.0 informational message.
    if !prune {
        for project in &report.orphans {
            if workspace
                .as_ref()
                .is_some_and(|name| &project.workspace != name)
            {
                continue;
            }
            if project.remote.is_none() {
                println!(
                    "{} '{}' is missing and cannot be restored automatically (no remote)",
                    "✗".red(),
                    project.name
                );
            }
        }
    }

    // Surface remaining conflicts (read-only signal). Conflicts we're about to
    // repair are shown in the plan instead, so skip them here.
    for conflict in &report.conflicts {
        if workspace
            .as_ref()
            .is_some_and(|name| &conflict.project.workspace != name)
        {
            continue;
        }
        if repairing.contains(conflict.project.name.as_str()) {
            continue;
        }
        println!(
            "{} '{}' conflicts at {} (run `pm check` for details)",
            "✗".red(),
            conflict.project.name,
            conflict.path.display()
        );
    }

    // --- Print plan (also the only output in dry-run) ---
    let nothing_to_do =
        restores.is_empty() && adopts.is_empty() && prunes.is_empty() && repairs.is_empty();

    if dry_run {
        println!("{} dry-run; no changes will be made\n", "ℹ".cyan());
        print_plan(&restores, &adopts, &prunes, &repairs);
        return Ok(());
    }

    if nothing_to_do {
        println!("Nothing to reconcile.");
        return Ok(());
    }

    // --- Guard non-interactive runs for any mutating mode ---
    let needs_prompts = !yes;
    if needs_prompts && !can_prompt() {
        // Pick the first thing we'd touch for the error message context.
        let name = restores
            .first()
            .map(|t| t.name.clone())
            .or_else(|| {
                adopts.iter().find_map(|(_, c)| match c {
                    AdoptCandidate::Ready(p) => Some(p.name.clone()),
                    _ => None,
                })
            })
            .or_else(|| prunes.first().cloned())
            .or_else(|| repairs.first().map(|t| t.name.clone()))
            .unwrap_or_else(|| "<reconcile>".to_string());
        return Err(PmError::NonInteractiveRestore(name).into());
    }

    // --- Apply adopts (each item confirmed independently) ---
    let mut adopted_count = 0usize;
    for (entry, candidate) in adopts {
        match candidate {
            AdoptCandidate::Skip(skip) => {
                println!(
                    "{} skipping adopt {}/{}: {}",
                    "⚠".yellow(),
                    entry.workspace,
                    entry.dir,
                    skip
                );
                continue;
            }
            AdoptCandidate::Ready(project) => {
                let approve = yes
                    || prompt_yes_no(
                        &format!(
                            "Adopt {}/{} ({}) into manifest?",
                            entry.workspace,
                            entry.dir,
                            entry.path.display()
                        ),
                        true,
                    )?;
                if !approve {
                    println!("  · skipped");
                    continue;
                }
                manifest.projects.push(project);
                adopted_count += 1;
                println!("{} Adopted {}/{}", "✓".green(), entry.workspace, entry.dir);
            }
        }
    }

    // --- Apply prunes (each item confirmed independently) ---
    let mut pruned_count = 0usize;
    if !prunes.is_empty() {
        let mut history = load_history()?;
        for name in prunes {
            let approve = yes
                || prompt_yes_no(
                    &format!(
                        "Prune manifest entry '{}' (directory missing on disk)?",
                        name
                    ),
                    false,
                )?;
            if !approve {
                println!("  · skipped");
                continue;
            }
            prune_one(&config, &mut manifest, &mut history, &name)?;
            pruned_count += 1;
            println!("{} Pruned '{}'", "✓".green(), name);
        }
        save_history(&history)?;
    }

    // Persist adopt + prune manifest changes in one shot.
    if adopted_count > 0 || pruned_count > 0 {
        save_state(&config, &manifest)?;
    }

    // --- Apply repairs (recreate / correct linked-project symlinks) ---
    if !repairs.is_empty() {
        let approve = !needs_prompts
            || prompt_yes_no(
                &format!(
                    "Repair {} link(s) by recreating/correcting the symlink? Continue?",
                    repairs.len()
                ),
                true,
            )?;
        if !approve {
            println!("Skipped link repair.");
        } else {
            for task in &repairs {
                match repair_one(&task.target, &task.convention) {
                    Ok(()) => println!(
                        "{} {} link '{}' → {}",
                        "✓".green(),
                        if task.drift { "Corrected" } else { "Restored" },
                        task.name,
                        task.target.display()
                    ),
                    Err(err) => println!("{} '{}' repair failed: {}", "✗".red(), task.name, err),
                }
            }
        }
    }

    // --- Apply restores (existing parallel clone) ---
    if !restores.is_empty() {
        if needs_prompts {
            let approve = prompt_yes_no(
                &format!(
                    "Found {} missing project(s) to restore from remote. Continue?",
                    restores.len()
                ),
                true,
            )?;
            if !approve {
                println!("Skipped restore.");
            } else {
                run_restores(restores, jobs)?;
            }
        } else {
            run_restores(restores, jobs)?;
        }
    }

    Ok(())
}

enum AdoptCandidate {
    Ready(crate::models::Project),
    Skip(AdoptSkip),
}

fn print_plan(
    restores: &[RestoreTask],
    adopts: &[(DiskEntry, AdoptCandidate)],
    prunes: &[String],
    repairs: &[RepairTask],
) {
    if restores.is_empty() && adopts.is_empty() && prunes.is_empty() && repairs.is_empty() {
        println!("Nothing to reconcile.");
        return;
    }
    if !repairs.is_empty() {
        println!("Link repairs ({}):", repairs.len());
        for task in repairs {
            println!(
                "  ~ {} {} → {}",
                if task.drift { "relink" } else { "create" },
                task.convention.display(),
                task.target.display()
            );
        }
    }
    if !restores.is_empty() {
        println!("Restores ({}):", restores.len());
        for task in restores {
            println!("  + clone {} → {}", task.remote, task.path.display());
        }
    }
    if !adopts.is_empty() {
        println!("Adopts ({}):", adopts.len());
        for (entry, candidate) in adopts {
            match candidate {
                AdoptCandidate::Ready(_) => println!(
                    "  + register {}/{} ({})",
                    entry.workspace,
                    entry.dir,
                    entry.path.display()
                ),
                AdoptCandidate::Skip(s) => {
                    println!("  ~ skip {}/{}: {}", entry.workspace, entry.dir, s)
                }
            }
        }
    }
    if !prunes.is_empty() {
        println!("Prunes ({}):", prunes.len());
        for name in prunes {
            println!(
                "  - drop manifest entry '{}' (history records snapshot)",
                name
            );
        }
    }
}

fn run_restores(restores: Vec<RestoreTask>, jobs: usize) -> Result<()> {
    let queue = Arc::new(Mutex::new(restores));
    let worker_count = jobs.max(1);
    let mut handles = Vec::new();

    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        handles.push(thread::spawn(move || -> Vec<String> {
            let mut messages = Vec::new();
            loop {
                let task = {
                    let mut queue = queue.lock().expect("queue lock poisoned");
                    queue.pop()
                };

                let Some(task) = task else {
                    break;
                };

                if let Some(parent) = task.path.parent() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        messages.push(format!("✗ '{}' failed: {}", task.name, err));
                        continue;
                    }
                }

                match clone_repo(&task.remote, &task.path) {
                    Ok(()) => messages.push(format!(
                        "✓ Restored '{}' to {}",
                        task.name,
                        task.path.display()
                    )),
                    Err(err) => messages.push(format!("✗ '{}' failed: {}", task.name, err)),
                }
            }
            messages
        }));
    }

    for handle in handles {
        for message in handle
            .join()
            .map_err(|_| anyhow::anyhow!("sync worker panicked"))?
        {
            println!("{}", message);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{load_history, load_manifest};
    use crate::models::{HistoryAction, Project, Workspace};
    use crate::state::save_state;
    use crate::test_support::{init_git_repo, TestEnv};

    fn seed_workspace(env: &TestEnv, name: &str) {
        let (config, mut manifest) = load_state().unwrap();
        if !manifest.workspaces.iter().any(|w| w.name == name) {
            manifest.workspaces.push(Workspace::new(name.to_string()));
        }
        std::fs::create_dir_all(env.workspace_dir(name)).unwrap();
        save_state(&config, &manifest).unwrap();
    }

    fn seed_project(env: &TestEnv, workspace: &str, project: &str, remote: Option<&str>) {
        seed_workspace(env, workspace);
        let (config, mut manifest) = load_state().unwrap();
        std::fs::create_dir_all(env.project_dir(workspace, project)).unwrap();
        let mut p = Project::new(
            project.to_string(),
            workspace.to_string(),
            project.to_string(),
            project.to_string(),
        );
        p.remote = remote.map(str::to_string);
        manifest.projects.push(p);
        save_state(&config, &manifest).unwrap();
    }

    #[test]
    fn sync_adopt_yes_registers_untracked_git_dir() {
        let env = TestEnv::new();
        // Drop an untracked git repo at base_root/default/discovered (default ws exists).
        let dir = env.project_dir("default", "discovered");
        init_git_repo(&dir);

        run(None, true, 1, true, false, false).expect("adopt should succeed");

        let manifest = load_manifest().unwrap();
        let project = manifest
            .projects
            .iter()
            .find(|p| p.name == "discovered")
            .expect("manifest should record adopted project");
        assert_eq!(project.workspace, "default");
        assert_eq!(project.dir, "discovered");
    }

    #[test]
    fn sync_adopt_skips_collision_and_unknown_workspace() {
        let env = TestEnv::new();
        // Existing project named "web" in default.
        seed_project(&env, "default", "web", None);

        // Untracked git dir with the same name in a workspace that isn't declared.
        let dir = env.workspace_dir("strangers").join("web");
        init_git_repo(&dir);

        run(None, true, 1, true, false, false).expect("adopt should not error overall");

        let manifest = load_manifest().unwrap();
        // Only one "web" remains — the seeded one. No adoption happened.
        let webs: Vec<_> = manifest
            .projects
            .iter()
            .filter(|p| p.name == "web")
            .collect();
        assert_eq!(webs.len(), 1, "collision should have been skipped");
        assert_eq!(webs[0].workspace, "default");
    }

    #[test]
    fn sync_prune_yes_drops_orphan_and_records_history() {
        let env = TestEnv::new();
        seed_workspace(&env, "default");
        // Manifest project whose directory does not exist on disk.
        let (config, mut manifest) = load_state().unwrap();
        let mut p = Project::new(
            "ghost".to_string(),
            "default".to_string(),
            "ghost".to_string(),
            "ghost".to_string(),
        );
        p.remote = Some("git@x:y/ghost.git".to_string());
        manifest.projects.push(p);
        save_state(&config, &manifest).unwrap();
        // No directory at env.project_dir("default", "ghost").
        assert!(!env.project_dir("default", "ghost").exists());

        run(None, true, 1, false, true, false).expect("prune should succeed");

        let manifest = load_manifest().unwrap();
        assert!(
            !manifest.projects.iter().any(|p| p.name == "ghost"),
            "manifest entry should be pruned"
        );
        let history = load_history().unwrap();
        let recorded = history
            .entries
            .iter()
            .find(|e| matches!(e.action, HistoryAction::Unregistered) && e.project.name == "ghost")
            .expect("history should record the prune");
        assert_eq!(recorded.project.workspace, "default");
    }

    #[test]
    fn sync_dry_run_makes_no_changes() {
        let env = TestEnv::new();
        // One untracked git dir (adopt candidate) and one orphan (prune candidate).
        let dir = env.project_dir("default", "to_adopt");
        init_git_repo(&dir);

        seed_workspace(&env, "default");
        let (config, mut manifest) = load_state().unwrap();
        manifest.projects.push(Project::new(
            "to_prune".to_string(),
            "default".to_string(),
            "to_prune".to_string(),
            "to_prune".to_string(),
        ));
        save_state(&config, &manifest).unwrap();

        // Snapshot manifest + history before.
        let before_manifest = load_manifest().unwrap();
        let before_history = load_history().unwrap();

        run(None, true, 1, true, true, true).expect("dry-run should succeed");

        let after_manifest = load_manifest().unwrap();
        let after_history = load_history().unwrap();
        assert_eq!(
            before_manifest.projects.len(),
            after_manifest.projects.len(),
            "dry-run must not change manifest"
        );
        assert_eq!(
            before_history.entries.len(),
            after_history.entries.len(),
            "dry-run must not change history"
        );
    }

    #[test]
    fn sync_adopt_without_yes_errors_in_non_tty() {
        let env = TestEnv::new();
        let dir = env.project_dir("default", "discovered");
        init_git_repo(&dir);

        // cargo test runs without TTY; can_prompt() returns false.
        let err = run(None, false, 1, true, false, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Non-interactive")
                || msg.contains("--yes")
                || msg.contains("'discovered'"),
            "expected non-interactive error, got: {msg}"
        );

        let manifest = load_manifest().unwrap();
        assert!(
            !manifest.projects.iter().any(|p| p.name == "discovered"),
            "manifest must be unchanged when sync refuses to run"
        );
    }

    #[test]
    fn sync_default_invocation_neither_adopts_nor_prunes() {
        let env = TestEnv::new();
        // Untracked git dir present (would be adopted under --adopt).
        let dir = env.project_dir("default", "discovered");
        init_git_repo(&dir);
        // Orphan manifest entry present (would be pruned under --prune).
        seed_workspace(&env, "default");
        let (config, mut manifest) = load_state().unwrap();
        manifest.projects.push(Project::new(
            "ghost".to_string(),
            "default".to_string(),
            "ghost".to_string(),
            "ghost".to_string(),
        ));
        save_state(&config, &manifest).unwrap();

        // No adopt, no prune, no restore (no remote on ghost) — nothing happens.
        run(None, true, 1, false, false, false).expect("plain sync should be a no-op here");

        let manifest = load_manifest().unwrap();
        assert!(
            !manifest.projects.iter().any(|p| p.name == "discovered"),
            "default sync must not adopt"
        );
        assert!(
            manifest.projects.iter().any(|p| p.name == "ghost"),
            "default sync must not prune"
        );
    }

    #[test]
    fn sync_never_clones_a_linked_project() {
        let env = TestEnv::new();
        seed_workspace(&env, "default");

        // A remote that WOULD clone successfully if the link guard were broken.
        let origin = env.tempdir_path().join("origin");
        init_git_repo(&origin);

        // A linked project carrying that remote, with no symlink/target on disk
        // (a broken link — also the cross-machine case).
        let (config, mut manifest) = load_state().unwrap();
        let mut p = Project::new(
            "widgets".to_string(),
            "default".to_string(),
            "widgets".to_string(),
            "widgets".to_string(),
        );
        p.remote = Some(origin.to_string_lossy().into_owned());
        p.link = Some("/nonexistent/widgets".to_string());
        manifest.projects.push(p);
        save_state(&config, &manifest).unwrap();
        assert!(env
            .project_dir("default", "widgets")
            .symlink_metadata()
            .is_err());

        // Default sync must not clone (or prune) the linked project.
        run(None, true, 1, false, false, false).expect("sync should be a no-op for links");

        assert!(
            env.project_dir("default", "widgets")
                .symlink_metadata()
                .is_err(),
            "a linked project must never be cloned, even with a working remote"
        );
        let manifest = load_manifest().unwrap();
        assert!(
            manifest.projects.iter().any(|p| p.name == "widgets"),
            "default sync must not prune the linked entry"
        );
    }

    /// Link an external git repo as `widgets` in `default`; return its canonical
    /// target and the convention path (the symlink node).
    #[cfg(unix)]
    fn link_widgets(env: &TestEnv) -> (std::path::PathBuf, std::path::PathBuf) {
        let target = env.tempdir_path().join("external").join("widgets");
        init_git_repo(&target);
        crate::commands::link::run(
            target.to_string_lossy().into_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("link should succeed");
        (
            std::fs::canonicalize(&target).unwrap(),
            env.project_dir("default", "widgets"),
        )
    }

    #[cfg(unix)]
    #[test]
    fn sync_repairs_broken_link_when_target_exists() {
        let env = TestEnv::new();
        let (target, convention) = link_widgets(&env);
        // Break: remove the symlink, leave the target.
        std::fs::remove_file(&convention).unwrap();
        assert!(convention.symlink_metadata().is_err());

        run(None, true, 1, false, false, false).expect("sync should repair");

        assert!(
            std::fs::symlink_metadata(&convention)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the symlink should be recreated"
        );
        assert_eq!(std::fs::canonicalize(&convention).unwrap(), target);
        assert!(target.join(".git").exists(), "the target must be untouched");
    }

    #[cfg(unix)]
    #[test]
    fn sync_corrects_drifted_link() {
        let env = TestEnv::new();
        let (target, convention) = link_widgets(&env);
        // Drift: repoint the symlink at a different existing directory.
        let other = env.tempdir_path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::remove_file(&convention).unwrap();
        std::os::unix::fs::symlink(&other, &convention).unwrap();

        run(None, true, 1, false, false, false).expect("sync should correct drift");

        assert_eq!(
            std::fs::canonicalize(&convention).unwrap(),
            target,
            "the symlink should be repointed to the recorded target"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_does_not_repair_link_replaced_by_dir() {
        let env = TestEnv::new();
        let (_target, convention) = link_widgets(&env);
        // Replace the symlink with a real directory holding a file.
        std::fs::remove_file(&convention).unwrap();
        std::fs::create_dir_all(&convention).unwrap();
        std::fs::write(convention.join("keep.txt"), "x").unwrap();

        run(None, true, 1, false, false, false).expect("sync should be a no-op here");

        let meta = std::fs::symlink_metadata(&convention).unwrap();
        assert!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "a real directory at the convention path must be left intact"
        );
        assert!(convention.join("keep.txt").exists(), "its contents survive");
    }

    #[cfg(unix)]
    #[test]
    fn sync_dry_run_does_not_repair() {
        let env = TestEnv::new();
        let (_target, convention) = link_widgets(&env);
        std::fs::remove_file(&convention).unwrap();

        run(None, true, 1, false, false, true).expect("dry-run ok");

        assert!(
            convention.symlink_metadata().is_err(),
            "dry-run must not recreate the symlink"
        );
    }
}
