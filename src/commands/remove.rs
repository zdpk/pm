use crate::config::{load_projects, load_workspaces, save_projects, save_workspaces};
use crate::error::PmError;
use crate::path::expand_path;
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};

pub fn run(project: String, force: bool, recursive: bool) -> Result<()> {
    let mut projects_data = load_projects()?;
    let mut workspaces_data = load_workspaces()?;

    // Find project
    let project_idx = projects_data
        .projects
        .iter()
        .position(|p| p.name == project)
        .ok_or_else(|| PmError::ProjectNotFound(project.clone()))?;

    let project_data = &projects_data.projects[project_idx];
    let path = project_data.path.clone();

    if recursive {
        // -rf: Delete files too
        if !force {
            return Err(anyhow::anyhow!("-r requires -f flag"));
        }

        let expanded = expand_path(&path);

        // Show warning and confirm
        println!(
            "{} This will permanently delete:",
            "⚠".yellow()
        );
        println!("  {}", expanded.display());

        if expanded.exists() {
            // Count files
            let file_count = walkdir::WalkDir::new(&expanded)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .count();
            println!("  ({} files)", file_count);
        }

        print!("\nType project name to confirm: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != project {
            println!("Aborted.");
            return Ok(());
        }

        // Delete files
        if expanded.exists() {
            fs::remove_dir_all(&expanded)?;
        }

        // Remove from projects
        projects_data.projects.remove(project_idx);

        // Remove from all workspaces
        for ws in &mut workspaces_data.workspaces {
            ws.projects.retain(|n| n != &project);
        }

        save_projects(&projects_data)?;
        save_workspaces(&workspaces_data)?;

        println!(
            "{} Deleted '{}' and its files",
            "✓".green(),
            project.cyan()
        );
    } else if force {
        // -f: Move to trash
        // Remove from current workspace
        for ws in &mut workspaces_data.workspaces {
            ws.projects.retain(|n| n != &project);
        }

        // Add to .trash
        if let Some(trash) = workspaces_data
            .workspaces
            .iter_mut()
            .find(|w| w.name == ".trash")
        {
            if !trash.projects.contains(&project) {
                trash.projects.push(project.clone());
            }
        }

        save_workspaces(&workspaces_data)?;

        println!("{} Moved '{}' to trash", "✓".green(), project.cyan());
    } else {
        // No flags: Just unregister
        // Remove from projects
        projects_data.projects.remove(project_idx);

        // Remove from all workspaces
        for ws in &mut workspaces_data.workspaces {
            ws.projects.retain(|n| n != &project);
        }

        save_projects(&projects_data)?;
        save_workspaces(&workspaces_data)?;

        println!(
            "{} Unregistered '{}' (files kept at {})",
            "✓".green(),
            project.cyan(),
            path
        );
    }

    Ok(())
}
