use crate::config::load_ports;
use crate::error::PmError;
use crate::models::{PortKind, Project};
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
    let entry = ports.projects.get(&project_key).ok_or_else(|| {
        anyhow!(
            "No ports allocated for '{}'. Run 'pm ports assign {}' first.",
            project.name,
            project.name
        )
    })?;

    let mut env = HashMap::new();
    env.insert("PM_WORKSPACE".to_string(), workspace.to_string());
    env.insert("PM_PROJECT".to_string(), project.name.clone());

    for service in entry.services.values() {
        env.insert(service.env.clone(), service.port.to_string());

        match service.kind {
            PortKind::Backend => {
                env.insert("APP_HOST".to_string(), "127.0.0.1".to_string());
            }
            PortKind::Database => {
                env.insert(
                    "DATABASE_URL".to_string(),
                    format!(
                        "postgres://postgres:postgres@127.0.0.1:{}/{}_local",
                        service.port,
                        local_database_name(&project.name)
                    ),
                );
            }
            PortKind::Redis => {
                env.insert(
                    "REDIS_URL".to_string(),
                    format!("redis://127.0.0.1:{}", service.port),
                );
            }
            PortKind::Frontend | PortKind::Infra => {}
        }
    }

    Ok(env)
}

fn local_database_name(project_name: &str) -> String {
    project_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
