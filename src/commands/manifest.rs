use crate::cli::ManifestCommand;
use crate::config::migrate_legacy_data;
use anyhow::Result;
use colored::Colorize;

pub fn run(cmd: ManifestCommand) -> Result<()> {
    match cmd {
        ManifestCommand::Migrate => migrate(),
    }
}

fn migrate() -> Result<()> {
    let manifest = migrate_legacy_data()?;
    println!(
        "{} Wrote manifest.json ({} workspaces, {} projects)",
        "✓".green(),
        manifest.workspaces.len(),
        manifest.projects.len()
    );
    Ok(())
}
