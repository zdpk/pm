use crate::config::{load_config, load_manifest, save_all};
use crate::error::PmError;
use crate::models::{Config, Manifest, Project, Workspace};
use crate::path::{collapse_path, expand_path};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn load_state() -> Result<(Config, Manifest)> {
    Ok((load_config()?, load_manifest()?))
}

pub fn save_state(config: &Config, manifest: &Manifest) -> Result<()> {
    save_all(config, manifest)
}

pub fn parse_target(target: String) -> (Option<String>, String) {
    if target.starts_with('@') {
        let parts: Vec<&str> = target[1..].splitn(2, '/').collect();
        if parts.len() == 2 {
            return (Some(parts[0].to_string()), parts[1].to_string());
        }
    }
    (None, target)
}

pub fn find_workspace<'a>(manifest: &'a Manifest, name: &str) -> Result<&'a Workspace> {
    manifest
        .workspaces
        .iter()
        .find(|ws| ws.name == name)
        .ok_or_else(|| PmError::WorkspaceNotFound(name.to_string()).into())
}

pub fn find_workspace_mut<'a>(manifest: &'a mut Manifest, name: &str) -> Result<&'a mut Workspace> {
    manifest
        .workspaces
        .iter_mut()
        .find(|ws| ws.name == name)
        .ok_or_else(|| PmError::WorkspaceNotFound(name.to_string()).into())
}

pub fn find_project<'a>(manifest: &'a Manifest, name: &str) -> Result<&'a Project> {
    manifest
        .projects
        .iter()
        .find(|project| project.name == name)
        .ok_or_else(|| PmError::ProjectNotFound(name.to_string()).into())
}

pub fn find_project_mut<'a>(manifest: &'a mut Manifest, name: &str) -> Result<&'a mut Project> {
    manifest
        .projects
        .iter_mut()
        .find(|project| project.name == name)
        .ok_or_else(|| PmError::ProjectNotFound(name.to_string()).into())
}

pub fn workspace_root(config: &Config, workspace: &Workspace) -> PathBuf {
    workspace
        .root
        .as_deref()
        .map(expand_path)
        .unwrap_or_else(|| expand_path(&config.base_root).join(&workspace.name))
}

pub fn project_path(config: &Config, manifest: &Manifest, project: &Project) -> Result<PathBuf> {
    let workspace = find_workspace(manifest, &project.workspace)?;
    Ok(workspace_root(config, workspace).join(&project.dir))
}

pub fn project_path_display(
    config: &Config,
    manifest: &Manifest,
    project: &Project,
) -> Result<String> {
    Ok(collapse_path(&project_path(config, manifest, project)?))
}

pub fn detect_current_project<'a>(
    config: &Config,
    manifest: &'a Manifest,
) -> Option<(&'a Project, PathBuf)> {
    let cwd = std::env::current_dir().ok()?;
    let mut best_match: Option<(&Project, PathBuf)> = None;

    for project in &manifest.projects {
        let Ok(project_root) = project_path(config, manifest, project) else {
            continue;
        };
        if !cwd.starts_with(&project_root) {
            continue;
        }

        match &best_match {
            Some((_, current_root))
                if current_root.components().count() >= project_root.components().count() => {}
            _ => best_match = Some((project, project_root)),
        }
    }

    best_match
}

pub fn relative_dir(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        anyhow::anyhow!(
            "Path '{}' is outside workspace root '{}'",
            path.display(),
            root.display()
        )
    })?;

    let dir = relative.display().to_string();
    if dir.is_empty() || dir == "." {
        return Err(anyhow::anyhow!(
            "Project path cannot be the workspace root itself"
        ));
    }
    Ok(dir)
}

pub fn normalized_workspace_root(path: &str) -> Result<String> {
    let expanded = expand_path(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()?.join(expanded)
    };

    let normalized = if absolute.exists() {
        absolute.canonicalize()?
    } else {
        absolute
    };

    Ok(collapse_path(&normalized))
}
