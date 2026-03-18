use crate::error::PmError;
use crate::restore::{can_prompt, prompt_yes_no, restore_project};
use crate::state::{find_project_mut, load_state, parse_target, project_path, save_state};
use anyhow::Result;
use chrono::Utc;

pub fn run(target: String) -> Result<()> {
    let (mut config, mut manifest) = load_state()?;
    let (workspace_name, project_name) = parse_target(target);

    if let Some(workspace_name) = workspace_name {
        if !manifest.workspaces.iter().any(|ws| ws.name == workspace_name) {
            return Err(PmError::WorkspaceNotFound(workspace_name).into());
        }
        config.current_workspace = workspace_name;
    }

    let project_snapshot = {
        let project = find_project_mut(&mut manifest, &project_name)?;
        project.last_accessed = Utc::now();
        project.access_count += 1;
        config.current_workspace = project.workspace.clone();
        config.current_project = Some(project.name.clone());
        project.clone()
    };
    let initial_path = project_path(&config, &manifest, &project_snapshot)?;

    let final_path = if initial_path.exists() {
        initial_path
    } else {
        let project = find_project_mut(&mut manifest, &project_name)?.clone();
        if project.remote.is_none() {
            return Err(PmError::ProjectMissing(project.name.clone()).into());
        }
        if !can_prompt() {
            return Err(PmError::NonInteractiveRestore(project.name.clone()).into());
        }

        let should_restore = prompt_yes_no(
            &format!(
                "Project '{}' is missing at {}. Restore it now?",
                project.name,
                initial_path.display()
            ),
            true,
        )?;
        if !should_restore {
            return Err(PmError::ProjectMissing(project.name.clone()).into());
        }

        restore_project(&config, &manifest, &project)?
    };

    save_state(&config, &manifest)?;
    println!("{}", final_path.display());
    Ok(())
}
