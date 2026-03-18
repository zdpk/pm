use crate::error::PmError;
use crate::state::{load_state, save_state};
use anyhow::Result;
use colored::Colorize;

pub fn run(workspace: String) -> Result<()> {
    let (mut config, manifest) = load_state()?;

    if !manifest.workspaces.iter().any(|ws| ws.name == workspace) {
        return Err(PmError::WorkspaceNotFound(workspace).into());
    }

    config.current_workspace = workspace.clone();
    save_state(&config, &manifest)?;

    println!(
        "{} Switched to workspace '{}'",
        "✓".green(),
        workspace.cyan()
    );
    Ok(())
}
