use crate::config::{load_projects, load_workspaces, save_projects, save_workspaces};
use crate::error::PmError;
use crate::path::expand_path;
use anyhow::Result;
use chrono::Utc;

pub fn run(target: String) -> Result<()> {
    let mut projects_data = load_projects()?;
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

    // Find project and get path
    let project_path = {
        let project = projects_data
            .projects
            .iter_mut()
            .find(|p| p.name == project_name)
            .ok_or_else(|| PmError::ProjectNotFound(project_name.clone()))?;

        // Update access time and count
        project.last_accessed = Utc::now();
        project.access_count += 1;

        project.path.clone()
    };

    // Set current project
    workspaces_data.current_project = Some(project_name);
    save_workspaces(&workspaces_data)?;
    save_projects(&projects_data)?;

    // Print path for shell integration
    let path = expand_path(&project_path);
    println!("{}", path.display());

    Ok(())
}
