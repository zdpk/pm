use crate::cli::WorkspaceCommand;
use crate::error::PmError;
use crate::git::set_git_config;
use crate::models::Workspace;
use crate::state::{
    find_project, find_workspace, find_workspace_mut, load_state, project_path, save_state,
    workspace_dir,
};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::io::{self, Write};

pub fn run(cmd: WorkspaceCommand) -> Result<()> {
    match cmd {
        WorkspaceCommand::List => list(),
        WorkspaceCommand::New { name } => new(name),
        WorkspaceCommand::Remove {
            name,
            force,
            recursive,
        } => remove(name, force, recursive),
        WorkspaceCommand::Move {
            projects,
            workspace,
        } => move_projects(projects, workspace),
        WorkspaceCommand::Config {
            workspace,
            key,
            value,
            list,
            unset,
        } => config(workspace, key, value, list, unset),
        WorkspaceCommand::ApplyGit { workspace } => apply_git(workspace),
    }
}

fn list() -> Result<()> {
    let (config, manifest) = load_state()?;
    println!("  {:<12} {:<8} {}", "NAME", "PROJECTS", "PATH");

    for ws in &manifest.workspaces {
        if ws.is_system() {
            continue;
        }

        let marker = if ws.name == config.current_workspace {
            "*".green()
        } else {
            " ".normal()
        };
        let count = manifest
            .projects
            .iter()
            .filter(|p| p.workspace == ws.name)
            .count();
        let path = workspace_dir(&config.base_root, &ws.name);
        println!(
            "{} {:<12} {:<8} {}",
            marker,
            ws.name,
            count,
            path.display().to_string().dimmed()
        );
    }
    Ok(())
}

fn new(name: String) -> Result<()> {
    let (mut config, mut manifest) = load_state()?;
    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&name) {
        return Err(PmError::InvalidWorkspaceName(name).into());
    }
    if manifest.workspaces.iter().any(|ws| ws.name == name) {
        return Err(PmError::WorkspaceExists(name).into());
    }

    // Create the workspace directory by convention.
    let ws_path = workspace_dir(&config.base_root, &name);
    fs::create_dir_all(&ws_path)?;

    manifest.workspaces.push(Workspace::new(name.clone()));
    config.current_workspace = name.clone();
    save_state(&config, &manifest)?;

    println!(
        "{} Created workspace '{}' at {}",
        "✓".green(),
        name.cyan(),
        ws_path.display().to_string().dimmed()
    );
    println!("{} Switched to '{}'", "✓".green(), name.cyan());
    Ok(())
}

fn remove(name: String, force: bool, recursive: bool) -> Result<()> {
    let (mut config, mut manifest) = load_state()?;
    if name == "default" {
        return Err(PmError::CannotRemoveDefault.into());
    }
    if name.starts_with('.') {
        return Err(PmError::CannotRemoveSystem(name).into());
    }

    let _ = find_workspace(&manifest, &name)?;
    let project_names: Vec<String> = manifest
        .projects
        .iter()
        .filter(|project| project.workspace == name)
        .map(|project| project.name.clone())
        .collect();

    if recursive {
        if !force {
            return Err(anyhow::anyhow!("-r requires -f flag"));
        }
        if !project_names.is_empty() {
            println!(
                "{} This will permanently delete workspace '{}' and all tracked directories:",
                "⚠".yellow(),
                name
            );
            for project_name in &project_names {
                let project = find_project(&manifest, project_name)?;
                let path = project_path(&config, &manifest, project)?;
                println!("  - {} ({})", project_name, path.display());
            }

            print!("\nType workspace name to confirm: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if input.trim() != name {
                println!("Aborted.");
                return Ok(());
            }

            for project_name in &project_names {
                let project = find_project(&manifest, project_name)?;
                let path = project_path(&config, &manifest, project)?;
                if path.exists() {
                    fs::remove_dir_all(path)?;
                }
            }
        }
        manifest
            .projects
            .retain(|project| project.workspace != name);
    } else if force {
        manifest
            .projects
            .retain(|project| project.workspace != name);
    } else {
        for project in &mut manifest.projects {
            if project.workspace == name {
                project.workspace = ".trash".to_string();
            }
        }
    }

    manifest.workspaces.retain(|ws| ws.name != name);
    if config.current_workspace == name {
        config.current_workspace = "default".to_string();
        config.current_project = None;
    }
    save_state(&config, &manifest)?;
    println!("{} Removed workspace '{}'", "✓".green(), name.cyan());
    Ok(())
}

fn move_projects(projects: Vec<String>, workspace: String) -> Result<()> {
    let (config, mut manifest) = load_state()?;
    let _ = find_workspace(&manifest, &workspace)?;

    // Plan + validate all moves before applying any. Under the strict path
    // convention, moving a project's workspace also moves its directory
    // (D9). We compute every source/destination up front so a precondition
    // failure aborts the whole batch with the manifest untouched.
    struct Plan {
        name: String,
        src: std::path::PathBuf,
        dst: std::path::PathBuf,
    }

    let mut plans: Vec<Plan> = Vec::with_capacity(projects.len());
    let dst_ws_dir = workspace_dir(&config.base_root, &workspace);

    for project_name in &projects {
        let project = manifest
            .projects
            .iter()
            .find(|p| p.name == *project_name)
            .ok_or_else(|| PmError::ProjectNotFound(project_name.clone()))?;

        if project.workspace == workspace {
            return Err(anyhow::anyhow!(
                "Project '{}' is already in workspace '{}'",
                project_name,
                workspace
            ));
        }

        let src = project_path(&config, &manifest, project)?;
        let dst = dst_ws_dir.join(&project.dir);

        if dst.exists() {
            return Err(anyhow::anyhow!(
                "Destination '{}' already exists; refusing to move '{}'",
                dst.display(),
                project_name
            ));
        }

        plans.push(Plan {
            name: project_name.clone(),
            src,
            dst,
        });
    }

    fs::create_dir_all(&dst_ws_dir)?;

    // Apply: physically move first, then update manifest. On any move
    // failure, roll back already-moved directories so the on-disk state
    // matches the unchanged manifest.
    let mut moved: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for plan in &plans {
        // Detect the source node including a (possibly broken) symlink: a
        // linked project's convention path is a symlink, and `exists()` would
        // miss a dangling one. `fs::rename` on a symlink relocates the link
        // node itself, never the external target — exactly what we want.
        let src_present = plan.src.exists()
            || plan
                .src
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
        if src_present {
            if let Err(e) = fs::rename(&plan.src, &plan.dst) {
                // Roll back.
                for (src, dst) in moved.iter().rev() {
                    let _ = fs::rename(dst, src);
                }
                return Err(anyhow::anyhow!(
                    "Failed to move '{}' from {} to {}: {}. Manifest left unchanged.",
                    plan.name,
                    plan.src.display(),
                    plan.dst.display(),
                    e
                ));
            }
            moved.push((plan.src.clone(), plan.dst.clone()));
        }
        // If src is missing on disk, we still update the manifest below; the
        // user can `pm sync` to materialize it under the new convention path.
    }

    for project_name in &projects {
        if let Some(project) = manifest
            .projects
            .iter_mut()
            .find(|p| p.name == *project_name)
        {
            project.workspace = workspace.clone();
        }
    }

    save_state(&config, &manifest)?;
    println!(
        "{} Moved {} project(s) to '{}'",
        "✓".green(),
        projects.len(),
        workspace.cyan()
    );
    Ok(())
}

fn config(
    workspace: String,
    key: Option<String>,
    value: Option<String>,
    list: bool,
    unset: Option<String>,
) -> Result<()> {
    let (config, mut manifest) = load_state()?;
    let ws = find_workspace_mut(&mut manifest, &workspace)?;

    if list {
        if ws.git.is_empty() {
            println!("No git config set for workspace '{}'", workspace);
        } else {
            for (key, value) in &ws.git {
                println!("{} = {}", key, value);
            }
        }
        return Ok(());
    }

    if let Some(key) = unset {
        ws.git.remove(&key);
        save_state(&config, &manifest)?;
        println!(
            "{} Unset {} for workspace '{}'",
            "✓".green(),
            key,
            workspace.cyan()
        );
        return Ok(());
    }

    if let (Some(key), Some(value)) = (key, value) {
        ws.git.insert(key.clone(), value.clone());
        save_state(&config, &manifest)?;
        println!(
            "{} Set {} for workspace '{}'",
            "✓".green(),
            key,
            workspace.cyan()
        );
        return Ok(());
    }

    println!("Usage: pm ws config <workspace> <key> <value>");
    println!("       pm ws config <workspace> --list");
    println!("       pm ws config <workspace> --unset <key>");
    Ok(())
}

fn apply_git(workspace: String) -> Result<()> {
    let (config, manifest) = load_state()?;
    let ws = find_workspace(&manifest, &workspace)?;
    if ws.git.is_empty() {
        println!("No git config set for workspace '{}'", workspace);
        return Ok(());
    }

    let mut applied_count = 0;
    for project in manifest
        .projects
        .iter()
        .filter(|p| p.workspace == workspace)
    {
        let path = project_path(&config, &manifest, project)?;
        if !path.exists() {
            println!(
                "{} Skipped '{}' because {} is missing",
                "⚠".yellow(),
                project.name,
                path.display()
            );
            continue;
        }

        for (key, value) in &ws.git {
            if let Err(err) = set_git_config(&path.display().to_string(), key, value) {
                println!(
                    "{} Failed to apply config to '{}': {}",
                    "✗".red(),
                    project.name,
                    err
                );
            }
        }
        applied_count += 1;
        println!("{} Applied git config to '{}'", "✓".green(), project.name);
    }

    println!("\nApplied to {} projects.", applied_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_manifest;
    use crate::models::Project;
    use crate::state::save_state;
    use crate::test_support::TestEnv;

    /// Seed a workspace + project into the test env: create the directory,
    /// add the workspace + project to the manifest, save.
    fn seed_project(env: &TestEnv, workspace: &str, project: &str) {
        let (config, mut manifest) = load_state().expect("load state");
        if !manifest.workspaces.iter().any(|w| w.name == workspace) {
            manifest
                .workspaces
                .push(Workspace::new(workspace.to_string()));
        }
        std::fs::create_dir_all(env.project_dir(workspace, project)).expect("create project dir");
        std::fs::write(env.project_dir(workspace, project).join("README.md"), "x").unwrap();

        let proj = Project::new(
            project.to_string(),
            workspace.to_string(),
            project.to_string(),
            project.to_string(),
        );
        manifest.projects.push(proj);
        save_state(&config, &manifest).expect("save state");
    }

    #[test]
    fn mv_relocates_directory_and_updates_manifest() {
        let env = TestEnv::new();
        // Ensure target workspace exists and on disk.
        std::fs::create_dir_all(env.workspace_dir("target")).unwrap();
        let (config, mut manifest) = load_state().unwrap();
        manifest
            .workspaces
            .push(Workspace::new("target".to_string()));
        save_state(&config, &manifest).unwrap();

        seed_project(&env, "default", "web");

        move_projects(vec!["web".to_string()], "target".to_string()).expect("move should succeed");

        assert!(env.project_dir("target", "web").exists());
        assert!(!env.project_dir("default", "web").exists());

        let manifest = load_manifest().unwrap();
        let project = manifest.projects.iter().find(|p| p.name == "web").unwrap();
        assert_eq!(project.workspace, "target");
    }

    #[test]
    fn mv_aborts_when_destination_exists() {
        let env = TestEnv::new();
        // Pre-populate workspace + colliding destination.
        std::fs::create_dir_all(env.workspace_dir("target")).unwrap();
        let (config, mut manifest) = load_state().unwrap();
        manifest
            .workspaces
            .push(Workspace::new("target".to_string()));
        save_state(&config, &manifest).unwrap();

        seed_project(&env, "default", "web");
        std::fs::create_dir_all(env.project_dir("target", "web")).unwrap();

        let err = move_projects(vec!["web".to_string()], "target".to_string()).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "error should mention existing destination: {err}"
        );

        // Nothing moved on disk; manifest still records 'default'.
        assert!(env.project_dir("default", "web").exists());
        let manifest = load_manifest().unwrap();
        assert_eq!(
            manifest
                .projects
                .iter()
                .find(|p| p.name == "web")
                .unwrap()
                .workspace,
            "default"
        );
    }

    #[cfg(unix)]
    #[test]
    fn mv_linked_project_relocates_symlink_and_preserves_target() {
        let env = TestEnv::new();
        // Declare + materialize the target workspace.
        std::fs::create_dir_all(env.workspace_dir("target")).unwrap();
        let (config, mut manifest) = load_state().unwrap();
        manifest
            .workspaces
            .push(Workspace::new("target".to_string()));
        save_state(&config, &manifest).unwrap();

        // Create a linked project in `default`.
        let ext = env.tempdir_path().join("external").join("widgets");
        crate::test_support::init_git_repo(&ext);
        crate::commands::link::run(ext.to_string_lossy().into_owned(), None, None, None, None)
            .expect("link should succeed");

        move_projects(vec!["widgets".to_string()], "target".to_string())
            .expect("move should succeed");

        let old = env.project_dir("default", "widgets");
        let new = env.project_dir("target", "widgets");
        assert!(
            std::fs::symlink_metadata(&old).is_err(),
            "old symlink should be gone"
        );
        assert!(
            std::fs::symlink_metadata(&new)
                .unwrap()
                .file_type()
                .is_symlink(),
            "new convention path should be a symlink"
        );
        let canonical_ext = std::fs::canonicalize(&ext).unwrap();
        assert_eq!(
            std::fs::canonicalize(&new).unwrap(),
            canonical_ext,
            "moved symlink must still resolve to the external target"
        );
        assert!(
            ext.join(".git").exists(),
            "external target must not be moved"
        );

        let manifest = load_manifest().unwrap();
        let p = manifest
            .projects
            .iter()
            .find(|p| p.name == "widgets")
            .unwrap();
        assert_eq!(p.workspace, "target");
        assert_eq!(
            p.link.as_deref(),
            Some(canonical_ext.display().to_string().as_str()),
            "link target must be preserved across mv"
        );
    }
}
