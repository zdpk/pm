use crate::config::{load_projects, load_workspaces, save_projects, save_workspaces};
use crate::error::PmError;
use crate::git::{get_remote_url, is_git_repo};
use crate::models::Project;
use crate::path::{is_directory, normalize_path, path_exists};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;

pub fn run(
    path: String,
    name: Option<String>,
    tags: Option<String>,
    note: Option<String>,
    force: bool,
) -> Result<()> {
    // Validate path exists
    if !path_exists(&path) {
        return Err(PmError::PathNotFound(path).into());
    }

    if !is_directory(&path) {
        return Err(PmError::NotADirectory(path).into());
    }

    // Normalize path to ~/ format
    let normalized_path = normalize_path(&path)?;

    // Determine project name
    let project_name = match name {
        Some(n) => n,
        None => {
            // Extract directory name from path
            let expanded = crate::path::expand_path(&normalized_path);
            expanded
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("Could not determine project name from path"))?
        }
    };

    // Validate project name
    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&project_name) {
        return Err(PmError::InvalidProjectName(project_name).into());
    }

    // Check if git repository
    let is_git = is_git_repo(&normalized_path);
    if !is_git {
        println!(
            "{} Not a git repository, adding anyway...",
            "⚠".yellow()
        );
    }

    // Load existing data
    let mut projects_data = load_projects()?;
    let mut workspaces_data = load_workspaces()?;

    // Check for duplicates
    let name_exists = projects_data.projects.iter().any(|p| p.name == project_name);
    let path_exists = projects_data.projects.iter().any(|p| p.path == normalized_path);

    if (name_exists || path_exists) && !force {
        if name_exists {
            return Err(PmError::ProjectExists(project_name).into());
        } else {
            return Err(anyhow::anyhow!(
                "Path already registered. Use --force to overwrite."
            ));
        }
    }

    // Remove existing if force
    if force {
        projects_data.projects.retain(|p| p.name != project_name && p.path != normalized_path);
        for ws in &mut workspaces_data.workspaces {
            ws.projects.retain(|n| n != &project_name);
        }
    }

    // Create project
    let mut project = Project::new(project_name.clone(), normalized_path.clone());

    // Set remote URL if available
    if is_git {
        project.remote = get_remote_url(&normalized_path);
    }

    // Set tags
    if let Some(tags_str) = tags {
        project.tags = tags_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    // Set note
    project.note = note;

    // Add to projects
    projects_data.projects.push(project);

    // Add to current workspace
    let current_ws = &workspaces_data.current;
    if let Some(ws) = workspaces_data
        .workspaces
        .iter_mut()
        .find(|w| &w.name == current_ws)
    {
        ws.projects.push(project_name.clone());
    }

    // Save
    save_projects(&projects_data)?;
    save_workspaces(&workspaces_data)?;

    println!(
        "{} Added '{}' to workspace '{}'",
        "✓".green(),
        project_name.cyan(),
        current_ws.cyan()
    );

    Ok(())
}
