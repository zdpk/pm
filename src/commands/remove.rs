use crate::history::record_project_event;
use crate::models::HistoryAction;
use crate::state::{find_project, load_state, project_path, save_state};
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};

pub fn run(project: String, yes: bool, force: bool, recursive: bool) -> Result<()> {
    if recursive && !force {
        return Err(anyhow::anyhow!("-r requires -f flag"));
    }

    let (mut config, mut manifest) = load_state()?;
    let project_data = find_project(&manifest, &project)?.clone();
    let path = project_path(&config, &manifest, &project_data)?;

    if !confirm_removal(&project, yes, force, recursive, &path)? {
        return Ok(());
    }

    if recursive {
        if !force {
            return Err(anyhow::anyhow!("-r requires -f flag"));
        }

        record_project_event(&project_data, &path, HistoryAction::Deleted)?;
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        manifest
            .projects
            .retain(|candidate| candidate.name != project);
        clear_current_project(&mut config, &project);
        save_state(&config, &manifest)?;
        println!("{} Deleted '{}' and its files", "✓".green(), project.cyan());
    } else if force {
        record_project_event(&project_data, &path, HistoryAction::Trashed)?;
        if let Some(project_data) = manifest
            .projects
            .iter_mut()
            .find(|candidate| candidate.name == project)
        {
            project_data.workspace = ".trash".to_string();
        }
        clear_current_project(&mut config, &project);
        save_state(&config, &manifest)?;
        println!("{} Moved '{}' to trash", "✓".green(), project.cyan());
    } else {
        record_project_event(&project_data, &path, HistoryAction::Unregistered)?;
        manifest
            .projects
            .retain(|candidate| candidate.name != project);
        clear_current_project(&mut config, &project);
        save_state(&config, &manifest)?;
        println!(
            "{} Unregistered '{}' (files kept at {})",
            "✓".green(),
            project.cyan(),
            path.display()
        );
    }

    Ok(())
}

fn confirm_removal(
    project: &str,
    yes: bool,
    force: bool,
    recursive: bool,
    path: &std::path::Path,
) -> Result<bool> {
    if recursive {
        println!("{} This will permanently delete:", "⚠".yellow());
        println!("  {}", path.display());

        if path.exists() {
            let file_count = walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_file())
                .count();
            println!("  ({} files)", file_count);
        }
    } else if force {
        println!(
            "{} '{}' will be moved to trash",
            "⚠".yellow(),
            project.cyan()
        );
        println!("  {}", path.display());
    } else {
        println!(
            "{} '{}' will be unregistered (files kept)",
            "⚠".yellow(),
            project.cyan()
        );
        println!("  {}", path.display());
    }

    if !yes {
        print!("\nType 'y' to continue: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim() != "y" {
            println!("Aborted.");
            return Ok(false);
        }
    }

    Ok(true)
}

fn clear_current_project(config: &mut crate::models::Config, project: &str) {
    if config.current_project.as_deref() == Some(project) {
        config.current_project = None;
    }
}
