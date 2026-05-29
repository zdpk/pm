use crate::config::{load_history, save_history};
use crate::error::PmError;
use crate::git::clone_repo;
use crate::reconcile::{adopt_one, classify, prune_one, scan_base_root, AdoptSkip, DiskEntry};
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

    // Surface conflicts (read-only signal). Sync never auto-resolves them.
    for conflict in &report.conflicts {
        if workspace
            .as_ref()
            .is_some_and(|name| &conflict.project.workspace != name)
        {
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
    let nothing_to_do = restores.is_empty() && adopts.is_empty() && prunes.is_empty();

    if dry_run {
        println!("{} dry-run; no changes will be made\n", "ℹ".cyan());
        print_plan(&restores, &adopts, &prunes);
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

fn print_plan(restores: &[RestoreTask], adopts: &[(DiskEntry, AdoptCandidate)], prunes: &[String]) {
    if restores.is_empty() && adopts.is_empty() && prunes.is_empty() {
        println!("Nothing to reconcile.");
        return;
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
}
