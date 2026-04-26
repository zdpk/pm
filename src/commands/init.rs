use crate::config::{
    config_dir, config_path, history_path, is_initialized, manifest_path, projects_path,
    repo_specs_dir, workspaces_path,
};
use crate::error::PmError;
use crate::models::{Config, HistoryData, Manifest, RepoSpec};
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};

pub fn run(force: bool) -> Result<()> {
    if is_initialized() && !force {
        ensure_repo_specs_dir()?;
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

    fs::write(
        config_path(),
        serde_json::to_string_pretty(&Config::default())?,
    )?;
    println!("{} Created config.json", "✓".green());

    fs::write(
        manifest_path(),
        serde_json::to_string_pretty(&Manifest::default())?,
    )?;
    println!("{} Created manifest.json", "✓".green());

    fs::write(
        history_path(),
        serde_json::to_string_pretty(&HistoryData::default())?,
    )?;
    println!("{} Created history.json", "✓".green());

    ensure_repo_specs_dir()?;

    if force {
        let _ = fs::remove_file(projects_path());
        let _ = fs::remove_file(workspaces_path());
    }

    println!("\n{}", "PM initialized successfully!".green().bold());
    Ok(())
}

fn ensure_repo_specs_dir() -> Result<()> {
    let specs_dir = repo_specs_dir();
    fs::create_dir_all(&specs_dir)?;

    let default_spec_path = specs_dir.join("rust-axum-sqlx-backend.json");
    if !default_spec_path.exists() {
        let spec = RepoSpec {
            id: "rust-axum-sqlx-backend".to_string(),
            version: "0.1.0".to_string(),
            name: "Rust Axum SQLx Backend".to_string(),
            description: "Rust + Axum + SQLx + PostgreSQL backend template standard".to_string(),
            source: "BACKEND_TEMPLATE_STANDARD.md".to_string(),
        };
        fs::write(default_spec_path, serde_json::to_string_pretty(&spec)?)?;
        println!("{} Created repo-specs/rust-axum-sqlx-backend.json", "✓".green());
    }

    Ok(())
}
