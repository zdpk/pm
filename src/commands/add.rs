use crate::error::PmError;
use crate::git::{get_remote_url, is_git_repo, repo_slug_from_remote};
use crate::models::Project;
use crate::path::{expand_path, is_directory, normalize_path, path_exists};
use crate::state::{find_workspace, load_state, relative_dir, save_state, workspace_root};
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
    if !path_exists(&path) {
        return Err(PmError::PathNotFound(path).into());
    }
    if !is_directory(&path) {
        return Err(PmError::NotADirectory(path).into());
    }

    let normalized_path = normalize_path(&path)?;
    let expanded_path = expand_path(&normalized_path);

    let (mut config, mut manifest) = load_state()?;
    let current_workspace = config.current_workspace.clone();
    let workspace = find_workspace(&manifest, &current_workspace)?;
    let workspace_root = workspace_root(&config, workspace);
    let dir = relative_dir(&workspace_root, &expanded_path)?;

    let project_name = match name {
        Some(name) => name,
        None => expanded_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not determine project name from path"))?,
    };

    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&project_name) {
        return Err(PmError::InvalidProjectName(project_name).into());
    }

    let is_git = is_git_repo(&normalized_path);
    if !is_git {
        println!("{} Not a git repository, adding anyway...", "⚠".yellow());
    }

    let remote = if is_git {
        get_remote_url(&normalized_path)
    } else {
        None
    };

    let repo_slug = remote
        .as_deref()
        .and_then(repo_slug_from_remote)
        .unwrap_or_else(|| {
            expanded_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&project_name)
                .to_string()
        });

    let name_exists = manifest.projects.iter().any(|p| p.name == project_name);
    let dir_exists = manifest
        .projects
        .iter()
        .any(|p| p.workspace == current_workspace && p.dir == dir);

    if (name_exists || dir_exists) && !force {
        if name_exists {
            return Err(PmError::ProjectExists(project_name).into());
        }
        return Err(anyhow::anyhow!(
            "Directory '{}' is already registered in workspace '{}'. Use --force to overwrite.",
            dir,
            current_workspace
        ));
    }

    if force {
        manifest.projects.retain(|p| {
            !(p.name == project_name || (p.workspace == current_workspace && p.dir == dir))
        });
    }

    let mut project = Project::new(
        project_name.clone(),
        current_workspace.clone(),
        repo_slug,
        dir,
    );
    project.remote = remote;
    project.note = note;
    if let Some(tags_str) = tags {
        project.tags = tags_str
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }

    manifest.projects.push(project);
    config.current_project = Some(project_name.clone());
    save_state(&config, &manifest)?;

    println!(
        "{} Added '{}' to workspace '{}'",
        "✓".green(),
        project_name.cyan(),
        current_workspace.cyan()
    );

    Ok(())
}
