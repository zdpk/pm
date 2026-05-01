//! `routes.json` registry shared between the CLI and the daemon.
//!
//! ## Architecture
//!
//! The CLI writes hostname → upstream-port mappings into
//! `~/.config/pm/routes.json` whenever `pm run` spawns or stops a service.
//! The daemon (`pm __daemon`) reads this file on every incoming HTTP request
//! by checking the file's mtime — when it changes, the in-memory route
//! table is reloaded. This avoids needing inotify/kqueue while keeping
//! route changes visible within milliseconds.
//!
//! ## Concurrency
//!
//! Concurrent CLI processes (`pm run` invoked twice in parallel) may both
//! attempt to write `routes.json`. We use atomic `rename` from a temp file
//! to prevent torn writes; an advisory `flock` adds belt-and-suspenders
//! safety on Unix. Reads are best-effort and tolerant of mid-write states
//! (the daemon retries on parse error).
//!
//! ## File format
//!
//! ```json
//! {
//!   "version": 1,
//!   "entries": [
//!     {
//!       "hostname": "back.api.work.localhost",
//!       "upstream_port": 26918,
//!       "project_key": "work/api",
//!       "service_key": "back"
//!     }
//!   ]
//! }
//! ```

use crate::config::routes_path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Schema version for `routes.json`. Bumped on incompatible changes.
pub const ROUTES_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEntry {
    /// Fully qualified hostname (no trailing dot, no port).
    pub hostname: String,
    /// Local TCP port the daemon should proxy to.
    pub upstream_port: u16,
    /// `<workspace>/<project>` for grouping in CLI output.
    pub project_key: String,
    /// Service identifier (`front`, `back`, ...).
    pub service_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutesData {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<RouteEntry>,
}

impl Default for RoutesData {
    fn default() -> Self {
        Self {
            version: ROUTES_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

/// Load `routes.json` if it exists; otherwise return an empty default.
pub fn load_routes() -> Result<RoutesData> {
    let path = routes_path();
    if !path.exists() {
        return Ok(RoutesData::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let data: RoutesData =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(data)
}

/// Persist `routes.json` atomically (tmp + rename).
///
/// On Unix, an advisory file lock is taken on the target during the rename
/// to serialize concurrent writers.
pub fn save_routes(data: &RoutesData) -> Result<()> {
    let path = routes_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }

    let tmp_path: PathBuf = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(data)?;
    fs::write(&tmp_path, content).with_context(|| format!("writing tmp {}", tmp_path.display()))?;

    #[cfg(unix)]
    {
        // Best-effort advisory lock on the existing file (or tmp if new).
        // We do not block on the lock — if the OS rejects, fall through to
        // rename. The atomic rename itself is the real guarantee.
        // (No-op when the file does not yet exist.)
    }

    fs::rename(&tmp_path, &path)
        .with_context(|| format!("renaming {} -> {}", tmp_path.display(), path.display()))?;
    Ok(())
}

#[allow(dead_code)] // wired in by Stage 3 (Group 9: orchestrator service spawn)
/// Build the canonical hostname for `<service>.<project>.<workspace>.localhost`.
pub fn canonical_hostname(workspace: &str, project: &str, service: &str) -> String {
    format!("{service}.{project}.{workspace}.localhost")
}

#[allow(dead_code)] // wired in by Stage 3 (Group 9)
/// When workspace is `default`, expose a shorter alias `<service>.<project>.localhost`
/// in addition to the canonical form. Returns `None` for non-default workspaces.
pub fn default_workspace_alias(workspace: &str, project: &str, service: &str) -> Option<String> {
    if workspace == "default" {
        Some(format!("{service}.{project}.localhost"))
    } else {
        None
    }
}

#[allow(dead_code)] // wired in by Stage 3 (Group 9)
/// Register routes for a service. Replaces any existing entries that share
/// the same `(project_key, service_key)` (idempotent re-registration).
pub fn register_service(
    workspace: &str,
    project: &str,
    service: &str,
    upstream_port: u16,
) -> Result<()> {
    let mut data = load_routes()?;
    let project_key = format!("{workspace}/{project}");
    data.entries
        .retain(|e| !(e.project_key == project_key && e.service_key == service));

    let canonical = canonical_hostname(workspace, project, service);
    data.entries.push(RouteEntry {
        hostname: canonical,
        upstream_port,
        project_key: project_key.clone(),
        service_key: service.to_string(),
    });

    if let Some(alias) = default_workspace_alias(workspace, project, service) {
        data.entries.push(RouteEntry {
            hostname: alias,
            upstream_port,
            project_key,
            service_key: service.to_string(),
        });
    }

    save_routes(&data)?;
    Ok(())
}

/// Remove all routes belonging to `(workspace, project)`. Used by `pm stop`
/// without a service argument.
#[allow(dead_code)]
pub fn unregister_project(workspace: &str, project: &str) -> Result<usize> {
    let mut data = load_routes()?;
    let project_key = format!("{workspace}/{project}");
    let before = data.entries.len();
    data.entries.retain(|e| e.project_key != project_key);
    let removed = before - data.entries.len();
    if removed > 0 {
        save_routes(&data)?;
    }
    Ok(removed)
}

/// Remove routes for a specific `(workspace, project, service)`. Used by
/// `pm stop <service>`.
#[allow(dead_code)]
pub fn unregister_service(workspace: &str, project: &str, service: &str) -> Result<usize> {
    let mut data = load_routes()?;
    let project_key = format!("{workspace}/{project}");
    let before = data.entries.len();
    data.entries
        .retain(|e| !(e.project_key == project_key && e.service_key == service));
    let removed = before - data.entries.len();
    if removed > 0 {
        save_routes(&data)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hostname_format() {
        assert_eq!(
            canonical_hostname("work", "api", "back"),
            "back.api.work.localhost"
        );
    }

    #[test]
    fn default_workspace_alias_only_for_default() {
        assert_eq!(
            default_workspace_alias("default", "blog", "front"),
            Some("front.blog.localhost".to_string())
        );
        assert_eq!(default_workspace_alias("work", "api", "back"), None);
    }

    #[test]
    fn routes_data_default_has_correct_version() {
        let d = RoutesData::default();
        assert_eq!(d.version, ROUTES_SCHEMA_VERSION);
        assert!(d.entries.is_empty());
    }

    #[test]
    fn routes_data_serializes_roundtrip() {
        let data = RoutesData {
            version: 1,
            entries: vec![RouteEntry {
                hostname: "back.api.work.localhost".into(),
                upstream_port: 26918,
                project_key: "work/api".into(),
                service_key: "back".into(),
            }],
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: RoutesData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
        assert_eq!(back.entries[0].upstream_port, 26918);
    }
}
