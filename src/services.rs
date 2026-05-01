//! Running-services registry: `~/.config/pm/services.json`.
//!
//! Each spawned service from `pm run` is tracked here with its PID, the
//! port it was allocated, and the absolute path to its log file. The
//! file is consulted by:
//! - `pm logs <service>` to find the log path
//! - `pm stop` to find PIDs to signal
//! - `pm proxy status` (indirectly, via `routes.json`)
//!
//! Schema is keyed by `<workspace>/<project>` → service identifier →
//! [`ServiceState`]. Processes are tracked across pm invocations because
//! services are spawned detached (`setsid`) and survive the CLI exit.

use crate::config::services_state_path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub pid: u32,
    pub port: u16,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub log_path: PathBuf,
    /// The dev_cmd that was spawned (recorded for `pm logs` headers).
    pub dev_cmd: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServicesData {
    /// `{workspace}/{project}` → service-key → state
    #[serde(default)]
    pub projects: HashMap<String, HashMap<String, ServiceState>>,
}

pub fn load() -> Result<ServicesData> {
    let path = services_state_path();
    if !path.exists() {
        return Ok(ServicesData::default());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let data: ServicesData = serde_json::from_str(&content)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(data)
}

pub fn save(data: &ServicesData) -> Result<()> {
    let path = services_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(data)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Project key as stored in the file.
pub fn project_key(workspace: &str, project: &str) -> String {
    format!("{workspace}/{project}")
}

/// Insert or replace the state of a single service. Called after spawn.
pub fn upsert(
    workspace: &str,
    project: &str,
    service: &str,
    state: ServiceState,
) -> Result<()> {
    let mut data = load()?;
    let entry = data.projects.entry(project_key(workspace, project)).or_default();
    entry.insert(service.to_string(), state);
    save(&data)
}

#[allow(dead_code)] // wired in by Group 10 (pm stop / pm logs)
/// Remove a single service. Called by `pm stop <svc>`.
pub fn remove(workspace: &str, project: &str, service: &str) -> Result<Option<ServiceState>> {
    let mut data = load()?;
    let key = project_key(workspace, project);
    let removed = match data.projects.get_mut(&key) {
        Some(map) => map.remove(service),
        None => None,
    };
    if let Some(map) = data.projects.get(&key)
        && map.is_empty()
    {
        data.projects.remove(&key);
    }
    if removed.is_some() {
        save(&data)?;
    }
    Ok(removed)
}

#[allow(dead_code)] // wired in by Group 10
/// Remove and return all services for a project. Called by `pm stop` (no args).
pub fn remove_project(workspace: &str, project: &str) -> Result<HashMap<String, ServiceState>> {
    let mut data = load()?;
    let removed = data
        .projects
        .remove(&project_key(workspace, project))
        .unwrap_or_default();
    if !removed.is_empty() {
        save(&data)?;
    }
    Ok(removed)
}

/// Look up a single service.
pub fn get(workspace: &str, project: &str, service: &str) -> Result<Option<ServiceState>> {
    let data = load()?;
    Ok(data
        .projects
        .get(&project_key(workspace, project))
        .and_then(|m| m.get(service))
        .cloned())
}

#[allow(dead_code)] // wired in by Group 10
/// Look up all services of a project.
pub fn list_project(workspace: &str, project: &str) -> Result<HashMap<String, ServiceState>> {
    let data = load()?;
    Ok(data
        .projects
        .get(&project_key(workspace, project))
        .cloned()
        .unwrap_or_default())
}

#[allow(dead_code)] // wired in by Group 10 (pm stop / pm proxy status)
/// Filter the in-memory state to only services whose PID is still alive.
/// Used to clean up stale entries left by ungraceful exits.
#[cfg(unix)]
pub fn prune_dead(data: &mut ServicesData) -> usize {
    let mut removed = 0;
    data.projects.retain(|_proj, services| {
        services.retain(|_svc, state| {
            let alive = pid_alive(state.pid);
            if !alive {
                removed += 1;
            }
            alive
        });
        !services.is_empty()
    });
    removed
}

#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(not(unix))]
pub fn pid_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_state() -> ServiceState {
        ServiceState {
            pid: 12345,
            port: 26918,
            started_at: Utc::now(),
            log_path: PathBuf::from("/tmp/x.log"),
            dev_cmd: "cargo run".into(),
        }
    }

    #[test]
    fn project_key_format() {
        assert_eq!(project_key("work", "api"), "work/api");
    }

    #[test]
    fn services_data_default_is_empty() {
        let d = ServicesData::default();
        assert!(d.projects.is_empty());
    }

    #[test]
    fn services_data_round_trips_through_json() {
        let mut data = ServicesData::default();
        data.projects
            .entry("work/api".into())
            .or_default()
            .insert("back".into(), sample_state());
        let json = serde_json::to_string(&data).unwrap();
        let back: ServicesData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.projects.len(), 1);
        assert_eq!(back.projects["work/api"]["back"].port, 26918);
    }
}
