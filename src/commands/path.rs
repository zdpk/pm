use crate::config::{load_projects, load_workspaces, save_workspaces};
use crate::error::PmError;
use crate::path::expand_path;
use anyhow::Result;

pub fn run(target: String) -> Result<()> {
    let projects_data = load_projects()?;
    let mut workspaces_data = load_workspaces()?;

    // Parse @workspace/project syntax
    let (workspace_name, project_name) = if target.starts_with('@') {
        let parts: Vec<&str> = target[1..].splitn(2, '/').collect();
        if parts.len() == 2 {
            (Some(parts[0].to_string()), parts[1].to_string())
        } else {
            (None, target)
        }
    } else {
        (None, target)
    };

    // Switch workspace if specified
    if let Some(ref ws_name) = workspace_name {
        if !workspaces_data.workspaces.iter().any(|w| &w.name == ws_name) {
            return Err(PmError::WorkspaceNotFound(ws_name.clone()).into());
        }
        workspaces_data.current = ws_name.clone();
        save_workspaces(&workspaces_data)?;
    }

    // Find project
    let project = projects_data
        .projects
        .iter()
        .find(|p| p.name == project_name)
        .ok_or_else(|| PmError::ProjectNotFound(project_name.clone()))?;

    // Print path for shell integration
    let path = expand_path(&project.path);
    println!("{}", path.display());

    Ok(())
}
