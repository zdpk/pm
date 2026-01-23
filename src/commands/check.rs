use crate::config::load_projects;
use crate::path::path_exists;
use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let projects_data = load_projects()?;

    let mut invalid_count = 0;

    for project in &projects_data.projects {
        if path_exists(&project.path) {
            println!(
                "{} {:<16} {}",
                "✓".green(),
                project.name,
                project.path
            );
        } else {
            println!(
                "{} {:<16} {} (not found)",
                "✗".red(),
                project.name,
                project.path
            );
            invalid_count += 1;
        }
    }

    if invalid_count > 0 {
        println!(
            "\n{} projects have invalid paths.",
            invalid_count.to_string().red()
        );
    } else {
        println!("\nAll projects have valid paths.");
    }

    Ok(())
}
