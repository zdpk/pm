use crate::config::{
    config_dir, config_path, is_initialized, projects_path, workspaces_path,
};
use crate::error::PmError;
use crate::models::{Config, ProjectsData, WorkspacesData};
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};

pub fn run(force: bool) -> Result<()> {
    if is_initialized() && !force {
        return Err(PmError::AlreadyInitialized.into());
    }

    if is_initialized() && force {
        print!(
            "{} Existing configuration will be overwritten.\nContinue? [y/N] ",
            "⚠".yellow()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Create config directory
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    println!("{} Created {}", "✓".green(), dir.display());

    // Create config.json
    let config = Config::default();
    let config_content = serde_json::to_string_pretty(&config)?;
    fs::write(config_path(), config_content)?;
    println!("{} Created config.json", "✓".green());

    // Create projects.json
    let projects = ProjectsData::default();
    let projects_content = serde_json::to_string_pretty(&projects)?;
    fs::write(projects_path(), projects_content)?;
    println!("{} Created projects.json", "✓".green());

    // Create workspaces.json
    let workspaces = WorkspacesData::default();
    let workspaces_content = serde_json::to_string_pretty(&workspaces)?;
    fs::write(workspaces_path(), workspaces_content)?;
    println!("{} Created workspaces.json", "✓".green());

    println!("\n{}", "PM initialized successfully!".green().bold());

    Ok(())
}
