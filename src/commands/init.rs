use crate::config::{
    config_dir, config_path, history_path, is_initialized, manifest_path, projects_path,
    workspaces_path,
};
use crate::error::PmError;
use crate::models::{Config, HistoryData, Manifest};
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

    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    println!("{} Created {}", "✓".green(), dir.display());

    fs::write(config_path(), serde_json::to_string_pretty(&Config::default())?)?;
    println!("{} Created config.json", "✓".green());

    fs::write(manifest_path(), serde_json::to_string_pretty(&Manifest::default())?)?;
    println!("{} Created manifest.json", "✓".green());

    fs::write(history_path(), serde_json::to_string_pretty(&HistoryData::default())?)?;
    println!("{} Created history.json", "✓".green());

    if force {
        let _ = fs::remove_file(projects_path());
        let _ = fs::remove_file(workspaces_path());
    }

    println!("\n{}", "PM initialized successfully!".green().bold());
    Ok(())
}
