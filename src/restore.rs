use crate::error::PmError;
use crate::git::clone_repo;
use crate::models::{Config, Manifest, Project};
use crate::state::project_path;
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

pub fn can_prompt() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub fn prompt_yes_no(question: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{} {} ", question, suffix);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(default_yes);
    }
    Ok(trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes"))
}

pub fn restore_project(config: &Config, manifest: &Manifest, project: &Project) -> Result<PathBuf> {
    let remote = project
        .remote
        .as_deref()
        .ok_or_else(|| PmError::NoRemoteUrl(project.name.clone()))?;

    let target = project_path(config, manifest, project)?;
    if target.exists() {
        return Ok(target);
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    println!(
        "{} Cloning '{}' into {}",
        "✓".green(),
        remote,
        target.display()
    );
    clone_repo(remote, &target)?;
    Ok(target)
}
