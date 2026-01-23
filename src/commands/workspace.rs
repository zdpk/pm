use crate::cli::WorkspaceCommand;
use crate::config::{load_projects, load_workspaces, save_projects, save_workspaces};
use crate::error::PmError;
use crate::git::set_git_config;
use crate::models::Workspace;
use crate::path::expand_path;
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
    let workspaces_data = load_workspaces()?;

    println!("  {:<12} {}", "NAME", "PROJECTS");

    for ws in &workspaces_data.workspaces {
        if ws.is_system() {
            continue; // Hide .trash
        }

        let marker = if ws.name == workspaces_data.current {
            "*".green()
        } else {
            " ".normal()
        };

        println!(
            "{} {:<12} {}",
            marker,
            ws.name,
            ws.projects.len()
        );
    }

    Ok(())
}

fn new(name: String) -> Result<()> {
    let mut workspaces_data = load_workspaces()?;

    // Validate name
    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&name) {
        return Err(PmError::InvalidWorkspaceName(name).into());
    }

    // Check if exists
    if workspaces_data.workspaces.iter().any(|w| w.name == name) {
        return Err(PmError::WorkspaceExists(name).into());
    }

    // Create workspace
    let workspace = Workspace::new(name.clone());
    workspaces_data.workspaces.push(workspace);

    // Switch to new workspace
    workspaces_data.current = name.clone();

    save_workspaces(&workspaces_data)?;

    println!("{} Created workspace '{}'", "✓".green(), name.cyan());
    println!("{} Switched to '{}'", "✓".green(), name.cyan());

    Ok(())
}

fn remove(name: String, force: bool, recursive: bool) -> Result<()> {
    let mut workspaces_data = load_workspaces()?;
    let mut projects_data = load_projects()?;

    // Cannot remove default
    if name == "default" {
        return Err(PmError::CannotRemoveDefault.into());
    }

    // Cannot remove system workspaces
    if name.starts_with('.') {
        return Err(PmError::CannotRemoveSystem(name).into());
    }

    // Find workspace
    let ws_idx = workspaces_data
        .workspaces
        .iter()
        .position(|w| w.name == name)
        .ok_or_else(|| PmError::WorkspaceNotFound(name.clone()))?;

    let ws = &workspaces_data.workspaces[ws_idx];
    let project_names: Vec<String> = ws.projects.clone();

    if recursive {
        // -rf: Delete project files too
        if !force {
            return Err(anyhow::anyhow!("-r requires -f flag"));
        }

        if !project_names.is_empty() {
            println!(
                "{} This will permanently delete workspace '{}' and all files:",
                "⚠".yellow(),
                name
            );

            for pname in &project_names {
                if let Some(p) = projects_data.projects.iter().find(|p| &p.name == pname) {
                    println!("  - {} ({})", pname, p.path);
                }
            }

            print!("\nType workspace name to confirm: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim() != name {
                println!("Aborted.");
                return Ok(());
            }

            // Delete files
            for pname in &project_names {
                if let Some(p) = projects_data.projects.iter().find(|p| &p.name == pname) {
                    let expanded = expand_path(&p.path);
                    if expanded.exists() {
                        fs::remove_dir_all(&expanded)?;
                    }
                }
            }

            // Remove projects from data
            projects_data.projects.retain(|p| !project_names.contains(&p.name));
        }

        // Remove workspace
        workspaces_data.workspaces.remove(ws_idx);

        println!(
            "{} Deleted workspace '{}' and all project files",
            "✓".green(),
            name.cyan()
        );
    } else if force {
        // -f: Unregister projects (keep files)
        if !project_names.is_empty() {
            print!(
                "{} {} projects will be unregistered (files kept).\nContinue? [y/N] ",
                "⚠".yellow(),
                project_names.len()
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }

            // Remove projects from data
            projects_data.projects.retain(|p| !project_names.contains(&p.name));
        }

        // Remove workspace
        workspaces_data.workspaces.remove(ws_idx);

        println!("{} Removed workspace '{}'", "✓".green(), name.cyan());
    } else {
        // No flags: Move projects to trash
        if !project_names.is_empty() {
            print!(
                "{} {} projects will be moved to trash.\nContinue? [y/N] ",
                "⚠".yellow(),
                project_names.len()
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }

            // Move projects to .trash
            if let Some(trash) = workspaces_data
                .workspaces
                .iter_mut()
                .find(|w| w.name == ".trash")
            {
                for pname in &project_names {
                    if !trash.projects.contains(pname) {
                        trash.projects.push(pname.clone());
                    }
                }
            }
        }

        // Remove workspace
        workspaces_data.workspaces.remove(ws_idx);

        println!("{} Removed workspace '{}'", "✓".green(), name.cyan());
    }

    // Switch to default if current was removed
    if workspaces_data.current == name {
        workspaces_data.current = "default".to_string();
    }

    save_workspaces(&workspaces_data)?;
    save_projects(&projects_data)?;

    Ok(())
}

fn move_projects(projects: Vec<String>, workspace: String) -> Result<()> {
    let mut workspaces_data = load_workspaces()?;

    // Check target workspace exists
    if !workspaces_data.workspaces.iter().any(|w| w.name == workspace) {
        return Err(PmError::WorkspaceNotFound(workspace).into());
    }

    // Remove from current workspaces and add to target
    for ws in &mut workspaces_data.workspaces {
        for project in &projects {
            ws.projects.retain(|n| n != project);
        }
    }

    if let Some(target_ws) = workspaces_data
        .workspaces
        .iter_mut()
        .find(|w| w.name == workspace)
    {
        for project in &projects {
            if !target_ws.projects.contains(project) {
                target_ws.projects.push(project.clone());
            }
        }
    }

    save_workspaces(&workspaces_data)?;

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
    let mut workspaces_data = load_workspaces()?;

    let ws = workspaces_data
        .workspaces
        .iter_mut()
        .find(|w| w.name == workspace)
        .ok_or_else(|| PmError::WorkspaceNotFound(workspace.clone()))?;

    if list {
        // List all config
        if ws.git.is_empty() {
            println!("No git config set for workspace '{}'", workspace);
        } else {
            for (k, v) in &ws.git {
                println!("{} = {}", k, v);
            }
        }
    } else if let Some(key_to_unset) = unset {
        // Unset a key
        if ws.git.remove(&key_to_unset).is_some() {
            save_workspaces(&workspaces_data)?;
            println!(
                "{} Unset {} for workspace '{}'",
                "✓".green(),
                key_to_unset,
                workspace.cyan()
            );
        } else {
            println!("Key '{}' not found", key_to_unset);
        }
    } else if let (Some(k), Some(v)) = (key, value) {
        // Set a key
        ws.git.insert(k.clone(), v);
        save_workspaces(&workspaces_data)?;
        println!(
            "{} Set {} for workspace '{}'",
            "✓".green(),
            k,
            workspace.cyan()
        );
    } else {
        println!("Usage: pm ws config <workspace> <key> <value>");
        println!("       pm ws config <workspace> --list");
        println!("       pm ws config <workspace> --unset <key>");
    }

    Ok(())
}

fn apply_git(workspace: String) -> Result<()> {
    let workspaces_data = load_workspaces()?;
    let projects_data = load_projects()?;

    let ws = workspaces_data
        .workspaces
        .iter()
        .find(|w| w.name == workspace)
        .ok_or_else(|| PmError::WorkspaceNotFound(workspace.clone()))?;

    if ws.git.is_empty() {
        println!("No git config set for workspace '{}'", workspace);
        return Ok(());
    }

    let mut applied_count = 0;

    for project_name in &ws.projects {
        if let Some(project) = projects_data.projects.iter().find(|p| &p.name == project_name) {
            for (key, value) in &ws.git {
                if let Err(e) = set_git_config(&project.path, key, value) {
                    println!(
                        "{} Failed to apply config to '{}': {}",
                        "✗".red(),
                        project_name,
                        e
                    );
                    continue;
                }
            }
            println!("{} Applied git config to '{}'", "✓".green(), project_name);
            applied_count += 1;
        }
    }

    println!("\nApplied to {} projects.", applied_count);

    Ok(())
}
