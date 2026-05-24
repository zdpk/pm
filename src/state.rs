use crate::config::{load_config, load_manifest, save_all};
use crate::error::PmError;
use crate::models::{Config, Manifest, Project, Workspace};
use crate::path::{collapse_path, expand_path};
use anyhow::Result;
use std::path::PathBuf;

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

/// Compute a workspace's absolute directory under the strict path convention:
/// `<base_root>/<workspace_name>/`. Workspaces no longer carry their own root
/// override.
pub fn workspace_dir(base_root: &str, workspace_name: &str) -> PathBuf {
    expand_path(base_root).join(workspace_name)
}

/// Compute a project's absolute filesystem path: `<base_root>/<ws>/<dir>`.
///
/// This consults the manifest only to verify that the workspace name exists;
/// the path itself is fully determined by the convention.
pub fn project_path(config: &Config, manifest: &Manifest, project: &Project) -> Result<PathBuf> {
    let _ = find_workspace(manifest, &project.workspace)?;
    Ok(workspace_dir(&config.base_root, &project.workspace).join(&project.dir))
}

pub fn project_path_display(
    config: &Config,
    manifest: &Manifest,
    project: &Project,
) -> Result<String> {
    Ok(collapse_path(&project_path(config, manifest, project)?))
}

/// Detect which project the current working directory belongs to.
///
/// Under the convention, projects live at `<base_root>/<workspace>/<dir>`, so
/// we only need to check whether the cwd is rooted at `base_root` and look up
/// the next two path components directly in the manifest.
pub fn detect_current_project<'a>(
    config: &Config,
    manifest: &'a Manifest,
) -> Option<(&'a Project, PathBuf)> {
    let cwd = std::env::current_dir().ok()?;
    let base = expand_path(&config.base_root);

    let relative = cwd.strip_prefix(&base).ok()?;
    let mut components = relative.components();
    let ws_component = components.next()?.as_os_str().to_str()?;
    let project_component = components.next()?.as_os_str().to_str()?;

    let project = manifest
        .projects
        .iter()
        .find(|p| p.workspace == ws_component && p.dir == project_component)?;

    let project_root = base.join(ws_component).join(project_component);
    Some((project, project_root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_manifest() -> Manifest {
        Manifest {
            version: 4,
            workspaces: vec![Workspace {
                name: "acme".to_string(),
                created_at: Utc::now(),
                git: HashMap::new(),
            }],
            projects: vec![Project {
                name: "web".to_string(),
                workspace: "acme".to_string(),
                repo_slug: "web".to_string(),
                dir: "web".to_string(),
                remote: None,
                tags: vec![],
                note: None,
                added_at: Utc::now(),
                last_accessed: Utc::now(),
                access_count: 0,
                proj: None,
                repo_spec: None,
            }],
        }
    }

    fn config_with_base_root(base: &str) -> Config {
        Config {
            base_root: base.to_string(),
            ..Config::default()
        }
    }

    #[test]
    fn workspace_dir_is_name_composition() {
        let p = workspace_dir("/projects", "acme");
        assert_eq!(p, PathBuf::from("/projects/acme"));
    }

    #[test]
    fn project_path_is_deterministic_from_names() {
        let config = config_with_base_root("/projects");
        let manifest = make_manifest();
        let project = &manifest.projects[0];
        let path = project_path(&config, &manifest, project).unwrap();
        assert_eq!(path, PathBuf::from("/projects/acme/web"));
    }

    #[test]
    fn project_path_fails_when_workspace_missing() {
        let config = config_with_base_root("/projects");
        let manifest = make_manifest();
        let mut orphan = manifest.projects[0].clone();
        orphan.workspace = "ghost".to_string();
        assert!(project_path(&config, &manifest, &orphan).is_err());
    }
}
