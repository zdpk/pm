//! `pm run` — entry point for both legacy and orchestrator modes.
//!
//! ## Grammar
//!
//! v0.3.0 grammar (preserved):
//! ```text
//! pm run [project] -- <cmd> [args...]
//! ```
//!
//! v0.4.0 orchestrator grammar (activated when the resolved project has a
//! `.proj.yaml` with a non-empty `services:` section AND the user did NOT
//! pass a `--` separator):
//! ```text
//! pm run                  # all services in current project
//! pm run <service>        # one service in current project
//! pm run <service> <proj> # one service in a specific project
//! pm run <project>        # all services in a specific project
//! ```
//!
//! Disambiguation rule: if `--` was passed, run legacy mode unconditionally.
//! Otherwise consult the project's `.proj.yaml`. If the first positional
//! matches a service key in `.proj.yaml.services`, treat it as a service;
//! else treat it as a project name.

use crate::config::load_ports;
use crate::error::PmError;
use crate::models::Project;
use crate::project as proj;
use crate::state::{detect_current_project, load_state, parse_target, project_path};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub fn run(positional: Vec<String>, command: Vec<String>) -> Result<()> {
    // Legacy mode: presence of `--` (i.e. `command` is non-empty) takes
    // precedence over orchestrator semantics. This guarantees v0.3.0
    // grammar continues to work unchanged.
    if !command.is_empty() {
        return run_legacy(positional.into_iter().next(), command);
    }

    // No `--`. Inspect the first positional and the current project's
    // .proj.yaml to decide service-mode vs all-services vs legacy fallback.
    let (workspace, project, project_dir) = resolve_project_from_positional(&positional)?;

    let proj_config = proj::load_proj_config(&project_dir).ok();
    let services = proj_config.as_ref().map(|c| &c.services);

    let has_services = services.map(|s| !s.is_empty()).unwrap_or(false);
    if !has_services {
        return Err(anyhow!(
            "No `--` command provided and project '{}' has no services in .proj.yaml. \
             Did you mean `pm run -- <command>` or `pm proj init` to add services?",
            project.name
        ));
    }

    // Decide which services to start.
    let services_map = services.unwrap();
    let target_service = first_service_token(&positional, services_map);

    #[cfg(unix)]
    {
        crate::commands::orchestrator::start(
            &workspace,
            &project,
            &project_dir,
            proj_config.as_ref().unwrap(),
            target_service.as_deref(),
        )
    }

    #[cfg(not(unix))]
    {
        let _ = (workspace, project, project_dir, target_service);
        Err(anyhow!(
            "Orchestrator mode requires Unix (macOS/Linux) in v0.4.0. \
             Use `pm run -- <command>` for stateless mode on this platform."
        ))
    }
}

// ── Legacy v0.3.0 mode ──

fn run_legacy(project_name: Option<String>, command: Vec<String>) -> Result<()> {
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

// ── Project resolution ──

/// Resolve project for orchestrator mode given the raw positional args.
///
/// Tries each interpretation in order:
/// 1. Empty positionals → current project
/// 2. First positional looks like a project (workspace/project or @-prefixed
///    or NOT a service key in current project) → use it as project, optional
///    second positional is the service identifier
/// 3. First positional is a service key in current project → current project,
///    optional second positional may override project
fn resolve_project_from_positional(positional: &[String]) -> Result<(String, Project, PathBuf)> {
    if positional.is_empty() {
        return resolve_project(None);
    }

    // Try interpreting first positional as a project name first; if that
    // fails, fall back to current project (the token may then be a service
    // identifier, which the caller resolves).
    //
    // For two-positional invocations like `pm run back api`, the second arg
    // is the project. We always consult the second slot when present.
    if let Some(second) = positional.get(1) {
        return resolve_project(Some(second.clone()));
    }

    // Single positional. We don't know yet whether it's a service or a
    // project. Try project resolution; if the project exists, use it (and
    // start ALL services). If not, fall back to current project (and treat
    // the token as a service hint).
    match resolve_project(Some(positional[0].clone())) {
        Ok(t) => Ok(t),
        Err(_) => resolve_project(None),
    }
}

/// Among the positional args, identify which (if any) is a service key in
/// the given services map. Service identifiers take precedence over project
/// identifiers when ambiguous, per the spec.
fn first_service_token(
    positional: &[String],
    services: &HashMap<String, proj::ServiceDef>,
) -> Option<String> {
    positional
        .iter()
        .find(|tok| services.contains_key(*tok))
        .cloned()
}

pub fn resolve_project(project_name: Option<String>) -> Result<(String, Project, PathBuf)> {
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

// ── Environment variable construction (shared with orchestrator) ──

pub fn build_port_env(workspace: &str, project: &Project) -> Result<HashMap<String, String>> {
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
        let name = local_database_name("work", "api");
        assert!(!name.ends_with("_local"));
        assert_eq!(name, "work_api");
    }

    #[test]
    fn first_service_token_finds_match() {
        let mut services = HashMap::new();
        services.insert("front".to_string(), proj::ServiceDef::default());
        services.insert("back".to_string(), proj::ServiceDef::default());

        let tokens = vec!["api".to_string(), "back".to_string()];
        let found = first_service_token(&tokens, &services);
        assert_eq!(found, Some("back".to_string()));
    }

    #[test]
    fn first_service_token_returns_none_when_absent() {
        let mut services = HashMap::new();
        services.insert("front".to_string(), proj::ServiceDef::default());

        let tokens = vec!["unknown-token".to_string()];
        let found = first_service_token(&tokens, &services);
        assert_eq!(found, None);
    }
}
