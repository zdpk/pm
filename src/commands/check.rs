use crate::git::{is_git_repo, remote_matches};
use crate::state::{load_state, project_path, project_path_display};
use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let (config, manifest) = load_state()?;
    let mut issues = 0;

    for project in &manifest.projects {
        let path = project_path(&config, &manifest, project)?;
        let path_display = project_path_display(&config, &manifest, project)?;

        if !path.exists() {
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

        if !is_git_repo(&path.display().to_string()) {
            println!(
                "{} {:<16} {} ({})",
                "✗".red(),
                project.name,
                path_display,
                "path-conflict".red()
            );
            issues += 1;
            continue;
        }

        if let Some(remote) = &project.remote {
            if !remote_matches(&path, remote) {
                println!(
                    "{} {:<16} {} ({})",
                    "✗".red(),
                    project.name,
                    path_display,
                    "remote-mismatch".red()
                );
                issues += 1;
                continue;
            }
        }

        println!("{} {:<16} {}", "✓".green(), project.name, path_display);
    }

    if issues > 0 {
        println!("\n{} issues found.", issues.to_string().red());
    } else {
        println!("\nAll projects are healthy.");
    }

    Ok(())
}
