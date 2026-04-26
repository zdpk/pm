use crate::error::PmError;
use crate::state::{find_project, load_state, parse_target, project_path};
use anyhow::Result;

pub fn run(target: String) -> Result<()> {
    let (mut config, manifest) = load_state()?;
    let (workspace_name, project_name) = parse_target(target);

    if let Some(workspace_name) = workspace_name {
        if !manifest
            .workspaces
            .iter()
            .any(|ws| ws.name == workspace_name)
        {
            return Err(PmError::WorkspaceNotFound(workspace_name).into());
        }
        config.current_workspace = workspace_name;
    }

    let project = find_project(&manifest, &project_name)?;
    let path = project_path(&config, &manifest, project)?;
    if !path.exists() {
        return Err(PmError::NonInteractiveRestore(project.name.clone()).into());
    }

    println!("{}", path.display());
    Ok(())
}
