use crate::config::load_ports;
use crate::error::PmError;
use crate::models::Project;
use crate::state::{detect_current_project, load_state, parse_target, project_path};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub fn run(project_name: Option<String>, command: Vec<String>) -> Result<()> {
    let (workspace, project, project_dir) = resolve_project(project_name)?;
    let env = build_port_env(&workspace, &project)?;

    if command.is_empty() {
        return Err(anyhow!("No command provided"));
    }

    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(project_dir)
        .envs(env)
        .status()?;

    std::process::exit(status.code().unwrap_or(1));
}

fn resolve_project(project_name: Option<String>) -> Result<(String, Project, PathBuf)> {
    let (config, manifest) = load_state()?;

    let project = match project_name {
        Some(target) => {
            let (workspace_name, project_name) = parse_target(target);
            if let Some(workspace_name) = &workspace_name {
                if !manifest
                    .workspaces
                    .iter()
                    .any(|workspace| &workspace.name == workspace_name)
                {
                    return Err(PmError::WorkspaceNotFound(workspace_name.clone()).into());
                }
            }

            manifest
                .projects
                .iter()
                .find(|project| {
                    project.name == project_name
                        && match &workspace_name {
                            Some(workspace) => project.workspace == *workspace,
                            None => true,
                        }
                })
                .cloned()
                .ok_or_else(|| PmError::ProjectNotFound(project_name.clone()))?
        }
        None => detect_current_project(&config, &manifest)
            .map(|(project, _)| project.clone())
            .ok_or_else(|| {
                anyhow!(
                    "No project specified and current directory is not inside a registered project"
                )
            })?,
    };

    let project_dir = project_path(&config, &manifest, &project)?;
    Ok((project.workspace.clone(), project, project_dir))
}

fn build_port_env(workspace: &str, project: &Project) -> Result<HashMap<String, String>> {
    let ports = load_ports()?;
    let project_key = format!("{workspace}/{}", project.name);

    let mut env = HashMap::new();
    env.insert("PM_WORKSPACE".to_string(), workspace.to_string());
    env.insert("PM_PROJECT".to_string(), project.name.clone());

    let postgres_port = ports.shared.postgres_port;
    let redis_port = ports.shared.redis_port;
    let db_name = local_database_name(workspace, &project.name);

    env.insert("LOCAL_POSTGRES_PORT".to_string(), postgres_port.to_string());
    env.insert(
        "DATABASE_URL".to_string(),
        format!("postgres://postgres:postgres@127.0.0.1:{postgres_port}/{db_name}"),
    );
    env.insert("LOCAL_REDIS_PORT".to_string(), redis_port.to_string());
    env.insert(
        "REDIS_URL".to_string(),
        format!("redis://127.0.0.1:{redis_port}"),
    );
    env.insert(
        "REDIS_KEY_PREFIX".to_string(),
        format!("{workspace}:{}", project.name),
    );

    if let Some(entry) = ports.projects.get(&project_key) {
        for service in entry.services.values() {
            if service.kind.is_shared() {
                continue;
            }
            env.insert(service.env.clone(), service.port.to_string());

            if matches!(service.kind, crate::models::PortKind::Backend) {
                env.insert("APP_HOST".to_string(), "127.0.0.1".to_string());
            }
        }
    }

    Ok(env)
}

/// Build the local Postgres database name for a project, in the form
/// `{workspace}_{project}`. Non-`[a-z0-9_]` characters are replaced with
/// `_`, and the entire result is lowercased.
///
/// **BREAKING (v0.4.0):** the trailing `_local` suffix used in v0.3.0 has
/// been removed. v0.3.0 users with existing `<ws>_<proj>_local` databases
/// receive a stderr migration notice on first orchestrator-mode `pm run`.
pub fn local_database_name(workspace: &str, project: &str) -> String {
    let raw = format!("{workspace}_{project}");
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_name_replaces_hyphens() {
        assert_eq!(local_database_name("work", "my-app"), "work_my_app");
    }

    #[test]
    fn db_name_lowercases() {
        assert_eq!(local_database_name("Work", "MyApp"), "work_myapp");
    }

    #[test]
    fn db_name_workspace_separates_collisions() {
        assert_ne!(
            local_database_name("a", "api"),
            local_database_name("b", "api"),
        );
        assert_eq!(local_database_name("a", "api"), "a_api");
        assert_eq!(local_database_name("b", "api"), "b_api");
    }

    #[test]
    fn db_name_replaces_non_alnum() {
        assert_eq!(local_database_name("ws.1", "p@y"), "ws_1_p_y");
    }

    #[test]
    fn db_name_no_local_suffix() {
        // BREAKING change in v0.4.0: ensure no `_local` is appended.
        let name = local_database_name("work", "api");
        assert!(!name.ends_with("_local"));
        assert_eq!(name, "work_api");
    }
}
