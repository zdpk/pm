use crate::cli::{RepoCommand, RepoSpecCommand};
use crate::config::{list_repo_specs, load_repo_spec};
use crate::models::RepoSpecMetadata;
use crate::state::{detect_current_project, find_project, find_project_mut, load_state, save_state};
use anyhow::{Result, anyhow};
use chrono::Utc;
use colored::Colorize;

pub fn run(cmd: RepoCommand) -> Result<()> {
    match cmd {
        RepoCommand::Spec(spec_cmd) => match spec_cmd {
            RepoSpecCommand::List => list_specs(),
            RepoSpecCommand::Show { id } => show_spec(id),
        },
        RepoCommand::Track {
            project,
            spec,
            version,
        } => track(project, spec, version),
        RepoCommand::Status { project } => status(project),
    }
}

fn list_specs() -> Result<()> {
    let specs = list_repo_specs()?;

    if specs.is_empty() {
        println!("{}", "(no repo specs registered)".dimmed());
        return Ok(());
    }

    println!("  {:<28} {:<10} NAME", "ID", "VERSION");
    for spec in specs {
        println!("  {:<28} {:<10} {}", spec.id, spec.version, spec.name);
    }

    Ok(())
}

fn show_spec(id: String) -> Result<()> {
    let spec = load_repo_spec(&id)?;

    println!("{} {}", "ID:".dimmed(), spec.id.cyan());
    println!("{} {}", "Version:".dimmed(), spec.version);
    println!("{} {}", "Name:".dimmed(), spec.name);
    println!("{} {}", "Description:".dimmed(), spec.description);
    println!("{} {}", "Source:".dimmed(), spec.source);

    Ok(())
}

fn track(project_name: String, spec_id: String, version: Option<String>) -> Result<()> {
    let spec = load_repo_spec(&spec_id)?;
    let tracked_version = version.unwrap_or_else(|| spec.version.clone());

    let (config, mut manifest) = load_state()?;
    let project = find_project_mut(&mut manifest, &project_name)?;

    project.repo_spec = Some(RepoSpecMetadata {
        id: spec.id.clone(),
        version: tracked_version.clone(),
        applied_at: Utc::now(),
    });

    save_state(&config, &manifest)?;

    println!(
        "{} Tracked '{}' with repo spec '{}@{}'",
        "✓".green(),
        project_name.cyan(),
        spec.id.cyan(),
        tracked_version
    );

    Ok(())
}

fn status(project_name: Option<String>) -> Result<()> {
    let (config, manifest) = load_state()?;
    let project = match project_name {
        Some(name) => find_project(&manifest, &name)?,
        None => detect_current_project(&config, &manifest)
            .map(|(project, _)| project)
            .ok_or_else(|| {
                anyhow!(
                    "No project specified and current directory is not inside a registered project"
                )
            })?,
    };

    println!("{} {}", "Project:".dimmed(), project.name.cyan());

    match &project.repo_spec {
        Some(metadata) => {
            println!("{} {}", "Spec:".dimmed(), metadata.id);
            println!("{} {}", "Version:".dimmed(), metadata.version);
            println!("{} {}", "Applied:".dimmed(), metadata.applied_at);

            if let Ok(spec) = load_repo_spec(&metadata.id) {
                if spec.version != metadata.version {
                    println!("{} {}", "Current spec:".dimmed(), spec.version.yellow());
                }
            }
        }
        None => {
            println!("{}", "Repo spec: untracked".dimmed());
        }
    }

    Ok(())
}
