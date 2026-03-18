use crate::cli::{WorkspaceCommand, WorkspaceRootCommand};
use crate::error::PmError;
use crate::git::set_git_config;
use crate::models::Workspace;
use crate::state::{
    find_project, find_workspace, find_workspace_mut, load_state, normalized_workspace_root,
    project_path, save_state,
};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::io::{self, Write};

pub fn run(cmd: WorkspaceCommand) -> Result<()> {
    match cmd {
        WorkspaceCommand::List => list(),
        WorkspaceCommand::New { name, root } => new(name, root),
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
        WorkspaceCommand::Root { command } => root(command),
    }
}

fn list() -> Result<()> {
    let (config, manifest) = load_state()?;
    println!("  {:<12} {:<8} {}", "NAME", "PROJECTS", "ROOT");

    for ws in &manifest.workspaces {
        if ws.is_system() {
            continue;
        }

        let marker = if ws.name == config.current_workspace {
            "*".green()
        } else {
            " ".normal()
        };
        let count = manifest.projects.iter().filter(|p| p.workspace == ws.name).count();
        let root = ws.root.clone().unwrap_or_else(|| format!("{}/{}", config.base_root, ws.name));
        println!("{} {:<12} {:<8} {}", marker, ws.name, count, root.dimmed());
    }
    Ok(())
}

fn new(name: String, root: Option<String>) -> Result<()> {
    let (mut config, mut manifest) = load_state()?;
    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&name) {
        return Err(PmError::InvalidWorkspaceName(name).into());
    }
    if manifest.workspaces.iter().any(|ws| ws.name == name) {
        return Err(PmError::WorkspaceExists(name).into());
    }

    let root = match root {
        Some(root) => Some(normalized_workspace_root(&root)?),
        None => None,
    };

    manifest.workspaces.push(Workspace::new(name.clone(), root));
    config.current_workspace = name.clone();
    save_state(&config, &manifest)?;

    println!("{} Created workspace '{}'", "✓".green(), name.cyan());
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
        manifest.projects.retain(|project| project.workspace != name);
    } else if force {
        manifest.projects.retain(|project| project.workspace != name);
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

    for project_name in &projects {
        let project = manifest
            .projects
            .iter_mut()
            .find(|project| project.name == *project_name)
            .ok_or_else(|| PmError::ProjectNotFound(project_name.clone()))?;
        project.workspace = workspace.clone();
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
        println!("{} Unset {} for workspace '{}'", "✓".green(), key, workspace.cyan());
        return Ok(());
    }

    if let (Some(key), Some(value)) = (key, value) {
        ws.git.insert(key.clone(), value.clone());
        save_state(&config, &manifest)?;
        println!("{} Set {} for workspace '{}'", "✓".green(), key, workspace.cyan());
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
    for project in manifest.projects.iter().filter(|p| p.workspace == workspace) {
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

fn root(command: WorkspaceRootCommand) -> Result<()> {
    match command {
        WorkspaceRootCommand::Set { workspace, path } => set_root(workspace, path),
    }
}

fn set_root(workspace: String, path: String) -> Result<()> {
    let (config, mut manifest) = load_state()?;
    let ws = find_workspace_mut(&mut manifest, &workspace)?;
    ws.root = Some(normalized_workspace_root(&path)?);
    save_state(&config, &manifest)?;
    println!("{} Set root for workspace '{}'", "✓".green(), workspace.cyan());
    Ok(())
}
