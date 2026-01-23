use crate::config::{load_workspaces, save_workspaces};
use crate::error::PmError;
use anyhow::Result;
use colored::Colorize;

pub fn run(workspace: String) -> Result<()> {
    let mut workspaces_data = load_workspaces()?;

    // Check workspace exists
    if !workspaces_data.workspaces.iter().any(|w| w.name == workspace) {
        return Err(PmError::WorkspaceNotFound(workspace).into());
    }

    // Update current workspace
    workspaces_data.current = workspace.clone();
    save_workspaces(&workspaces_data)?;

    println!(
        "{} Switched to workspace '{}'",
        "✓".green(),
        workspace.cyan()
    );

    Ok(())
}
