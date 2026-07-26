use crate::error::PmError;
use crate::models::{Config, HistoryData, Manifest, PortsData, RepoSpec, SharedInfra};
use crate::path::{expand_path, validate_project_dir};
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// Current manifest schema version produced by this build.
pub const MANIFEST_VERSION: u32 = 4;

/// Get config directory path
/// - pmd binary: ~/.config/pm-dev/
/// - pm binary: ~/.config/pm/
/// - Override: PM_CONFIG_DIR environment variable
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PM_CONFIG_DIR") {
        return expand_path(&dir);
    }

    let base = dirs::config_dir().expect("Could not determine config directory");
    let is_dev = std::env::args()
        .next()
        .map(|arg| {
            std::path::Path::new(&arg)
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| name == "pmd" || name.ends_with("/pmd"))
        })
        .unwrap_or(false);

    if is_dev {
        base.join("pm-dev")
    } else {
        base.join("pm")
    }
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn manifest_path() -> PathBuf {
    config_dir().join("manifest.json")
}

pub fn history_path() -> PathBuf {
    config_dir().join("history.json")
}

pub fn ports_path() -> PathBuf {
    config_dir().join("ports.json")
}

pub fn ports_backup_path_v1() -> PathBuf {
    config_dir().join("ports.json.bak.v1")
}

pub fn routes_path() -> PathBuf {
    config_dir().join("routes.json")
}

pub fn daemon_pid_path() -> PathBuf {
    config_dir().join("daemon.pid")
}

#[allow(dead_code)] // wired in by Stage 3 (services state for `pm logs`/`pm stop`)
pub fn services_state_path() -> PathBuf {
    config_dir().join("services.json")
}

pub fn foreground_processes_path() -> PathBuf {
    config_dir().join("foreground-processes.json")
}

pub fn logs_dir() -> PathBuf {
    config_dir().join("logs")
}

pub fn repo_specs_dir() -> PathBuf {
    config_dir().join("repo-specs")
}

pub fn repo_spec_path(id: &str) -> PathBuf {
    repo_specs_dir().join(format!("{id}.json"))
}

pub fn is_initialized() -> bool {
    config_dir().exists() && config_path().exists()
}

pub fn ensure_initialized() -> Result<()> {
    if !is_initialized() {
        return Err(PmError::NotInitialized.into());
    }
    Ok(())
}

pub fn load_config() -> Result<Config> {
    ensure_initialized()?;
    let content = fs::read_to_string(config_path())?;
    Ok(serde_json::from_str(&content)?)
}

pub fn save_config(config: &Config) -> Result<()> {
    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path(), content)?;
    Ok(())
}

// ──────────────────────────────────────────────
// Ports (global)
// ──────────────────────────────────────────────

pub fn load_ports() -> Result<PortsData> {
    ensure_initialized()?;
    let path = ports_path();
    if !path.exists() {
        let defaults = PortsData::default();
        save_ports(&defaults)?;
        return Ok(defaults);
    }
    let content = fs::read_to_string(&path)?;
    let raw: Value = serde_json::from_str(&content)?;

    let needs_migration = !matches!(raw.get("version").and_then(Value::as_u64), Some(v) if v >= 2);

    if needs_migration {
        let migrated = migrate_ports_v1_to_v2(&raw)?;
        let backup = ports_backup_path_v1();
        if !backup.exists() {
            fs::write(&backup, &content)?;
        }
        save_ports(&migrated)?;
        eprintln!(
            "pm: migrated ports.json v1 → v2; backup saved to {}",
            backup.display()
        );
        return Ok(migrated);
    }

    Ok(serde_json::from_value(raw)?)
}

pub fn save_ports(ports: &PortsData) -> Result<()> {
    let content = serde_json::to_string_pretty(ports)?;
    fs::write(ports_path(), content)?;
    Ok(())
}

fn migrate_ports_v1_to_v2(raw: &Value) -> Result<PortsData> {
    let mut value = raw.clone();
    let obj = value.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("Invalid ports.json: expected an object at the top level")
    })?;

    if let Some(projects) = obj.get_mut("projects").and_then(Value::as_object_mut) {
        for project in projects.values_mut() {
            if let Some(services) = project
                .as_object_mut()
                .and_then(|p| p.get_mut("services"))
                .and_then(Value::as_object_mut)
            {
                services.retain(|_, service| {
                    let kind = service.get("kind").and_then(Value::as_str);
                    !matches!(kind, Some("database") | Some("redis"))
                });
            }
        }
    }

    if let Some(ranges) = obj.get_mut("ranges").and_then(Value::as_object_mut) {
        ranges.remove("database");
        ranges.remove("redis");
    }

    obj.entry("shared")
        .or_insert_with(|| serde_json::to_value(SharedInfra::default()).unwrap());
    obj.insert("version".to_string(), Value::from(2u32));

    Ok(serde_json::from_value(value)?)
}

// ──────────────────────────────────────────────
// Repo specs
// ──────────────────────────────────────────────

pub fn load_repo_spec(id: &str) -> Result<RepoSpec> {
    ensure_initialized()?;
    let path = repo_spec_path(id);
    if !path.exists() {
        return Err(PmError::RepoSpecNotFound(id.to_string()).into());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn list_repo_specs() -> Result<Vec<RepoSpec>> {
    ensure_initialized()?;
    let dir = repo_specs_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut specs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(path)?;
        specs.push(serde_json::from_str(&content)?);
    }

    specs.sort_by(|a: &RepoSpec, b: &RepoSpec| a.id.cmp(&b.id));
    Ok(specs)
}

pub fn load_manifest() -> Result<Manifest> {
    ensure_initialized()?;

    if !manifest_path().exists() {
        let manifest = Manifest::default();
        save_manifest(&manifest)?;
        return Ok(manifest);
    }

    let content = fs::read_to_string(manifest_path())?;
    let raw: Value = serde_json::from_str(&content)?;
    validate_manifest_shape(&raw)?;

    let manifest: Manifest = serde_json::from_value(raw)?;
    Ok(manifest)
}

/// Validate the manifest's *shape* against the strict path convention.
///
/// Checks:
/// - schema version is exactly v4 (older schemas are rejected without migration)
/// - no workspace carries a legacy `root` override field
/// - every project's `dir` is a single safe path component
///
/// This intentionally does NOT check whether project directories physically
/// exist (D8: manifest is desired state, filesystem is observed state). A
/// missing directory must not break manifest loading.
fn validate_manifest_shape(raw: &Value) -> Result<()> {
    let obj = raw
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("manifest.json: expected a JSON object at the top level"))?;

    let version = obj
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("manifest.json: missing or invalid 'version' field"))?
        as u32;
    if version < MANIFEST_VERSION {
        return Err(PmError::ManifestVersionTooOld(version).into());
    }
    if version > MANIFEST_VERSION {
        return Err(anyhow::anyhow!(
            "manifest.json: version {} is newer than this build supports (max v{}). Update pm.",
            version,
            MANIFEST_VERSION
        ));
    }

    if let Some(workspaces) = obj.get("workspaces").and_then(Value::as_array) {
        for ws in workspaces {
            let name = ws
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
                .to_string();
            if ws.get("root").is_some() {
                return Err(PmError::UnsupportedManifestField {
                    workspace: name,
                    field: "root",
                }
                .into());
            }
        }
    }

    if let Some(projects) = obj.get("projects").and_then(Value::as_array) {
        for project in projects {
            let dir = project.get("dir").and_then(Value::as_str).unwrap_or("");
            if let Err(e) = validate_project_dir(dir) {
                let name = project
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("<unknown>");
                return Err(anyhow::anyhow!("project '{}': {}", name, e));
            }
        }
    }

    Ok(())
}

pub fn save_manifest(manifest: &Manifest) -> Result<()> {
    let content = serde_json::to_string_pretty(manifest)?;
    fs::write(manifest_path(), content)?;
    Ok(())
}

pub fn load_history() -> Result<HistoryData> {
    ensure_initialized()?;
    if !history_path().exists() {
        let history = HistoryData::default();
        save_history(&history)?;
        return Ok(history);
    }
    let content = fs::read_to_string(history_path())?;
    Ok(serde_json::from_str(&content)?)
}

pub fn save_history(history: &HistoryData) -> Result<()> {
    let content = serde_json::to_string_pretty(history)?;
    fs::write(history_path(), content)?;
    Ok(())
}

pub fn save_all(config: &Config, manifest: &Manifest) -> Result<()> {
    save_config(config)?;
    save_manifest(manifest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_manifest_shape_accepts_v4_manifest() {
        let raw = serde_json::json!({
            "version": MANIFEST_VERSION,
            "workspaces": [
                { "name": "default", "created_at": "2026-01-01T00:00:00Z" }
            ],
            "projects": [
                {
                    "name": "web",
                    "workspace": "default",
                    "dir": "web",
                    "repo_slug": "web",
                    "added_at": "2026-01-01T00:00:00Z",
                    "last_accessed": "2026-01-01T00:00:00Z"
                }
            ]
        });
        validate_manifest_shape(&raw).expect("v4 manifest should validate");
    }

    #[test]
    fn validate_manifest_shape_rejects_v3_manifest() {
        let raw = serde_json::json!({ "version": 3, "workspaces": [], "projects": [] });
        let err = validate_manifest_shape(&raw).unwrap_err();
        assert!(
            err.to_string().contains("v4"),
            "error should mention v4: {err}"
        );
    }

    #[test]
    fn validate_manifest_shape_rejects_workspace_root_override() {
        let raw = serde_json::json!({
            "version": MANIFEST_VERSION,
            "workspaces": [
                {
                    "name": "acme",
                    "root": "/somewhere/else",
                    "created_at": "2026-01-01T00:00:00Z"
                }
            ],
            "projects": []
        });
        let err = validate_manifest_shape(&raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("acme"), "error should name workspace: {msg}");
        assert!(msg.contains("root"), "error should name field: {msg}");
    }

    #[test]
    fn validate_manifest_shape_rejects_nested_project_dir() {
        let raw = serde_json::json!({
            "version": MANIFEST_VERSION,
            "workspaces": [
                { "name": "default", "created_at": "2026-01-01T00:00:00Z" }
            ],
            "projects": [
                {
                    "name": "bad",
                    "workspace": "default",
                    "dir": "web/frontend",
                    "repo_slug": "bad",
                    "added_at": "2026-01-01T00:00:00Z",
                    "last_accessed": "2026-01-01T00:00:00Z"
                }
            ]
        });
        let err = validate_manifest_shape(&raw).unwrap_err();
        assert!(
            err.to_string().contains("bad"),
            "error should name the project"
        );
    }

    #[test]
    fn validate_manifest_shape_allows_missing_project_dir_on_disk() {
        // The validator is shape-only. It never inspects the filesystem.
        // A manifest pointing at a non-existent directory must still validate
        // so that other commands can continue to work (D8).
        let raw = serde_json::json!({
            "version": MANIFEST_VERSION,
            "workspaces": [
                { "name": "default", "created_at": "2026-01-01T00:00:00Z" }
            ],
            "projects": [
                {
                    "name": "ghost",
                    "workspace": "default",
                    "dir": "ghost",
                    "repo_slug": "ghost",
                    "added_at": "2026-01-01T00:00:00Z",
                    "last_accessed": "2026-01-01T00:00:00Z"
                }
            ]
        });
        validate_manifest_shape(&raw).expect("missing dir must not break loading");
    }

    #[test]
    fn migrate_v1_drops_shared_kinds_and_injects_shared_defaults() {
        let v1 = serde_json::json!({
            "version": 1,
            "ranges": {
                "frontend": { "start": 10000, "end": 19999 },
                "backend":  { "start": 20000, "end": 29999 },
                "database": { "start": 30000, "end": 39999 },
                "redis":    { "start": 40000, "end": 44999 },
                "infra":    { "start": 45000, "end": 49999 }
            },
            "projects": {
                "work/api": {
                    "workspace": "work",
                    "project": "api",
                    "path": "~/work/api",
                    "services": {
                        "back":  { "kind": "backend",  "env": "APP_PORT",            "port": 20100 },
                        "db":    { "kind": "database", "env": "LOCAL_POSTGRES_PORT", "port": 30100 },
                        "redis": { "kind": "redis",    "env": "LOCAL_REDIS_PORT",    "port": 40100 }
                    }
                }
            }
        });

        let migrated = migrate_ports_v1_to_v2(&v1).expect("migration should succeed");

        assert_eq!(migrated.version, 2);
        assert_eq!(migrated.shared.postgres_port, 5432);
        assert_eq!(migrated.shared.redis_port, 6379);
        assert!(!migrated
            .ranges
            .contains_key(&crate::models::PortKind::Database));
        assert!(!migrated
            .ranges
            .contains_key(&crate::models::PortKind::Redis));

        let project = migrated
            .projects
            .get("work/api")
            .expect("project preserved");
        assert!(project.services.contains_key("back"));
        assert!(!project.services.contains_key("db"));
        assert!(!project.services.contains_key("redis"));
    }

    #[test]
    fn migrate_handles_missing_version_field() {
        let raw = serde_json::json!({
            "ranges": {},
            "projects": {}
        });
        let migrated = migrate_ports_v1_to_v2(&raw).expect("migration should succeed");
        assert_eq!(migrated.version, 2);
        assert_eq!(migrated.shared.postgres_port, 5432);
    }

    #[test]
    fn migrate_preserves_existing_shared_field() {
        let raw = serde_json::json!({
            "version": 1,
            "shared": { "postgres_port": 5500, "redis_port": 6500 },
            "ranges": {},
            "projects": {}
        });
        let migrated = migrate_ports_v1_to_v2(&raw).expect("migration should succeed");
        assert_eq!(migrated.shared.postgres_port, 5500);
        assert_eq!(migrated.shared.redis_port, 6500);
    }

    #[test]
    fn config_loads_with_default_dev_section_when_missing() {
        // Existing v0.3.0 config.json files do not have a `dev` section.
        // After v0.4.0, loading such a file MUST fill in DevConfig::default().
        let raw = serde_json::json!({
            "editor": "vim",
            "git_host": "https://github.com",
            "base_root": "~/work",
            "current_workspace": "default"
        });
        let cfg: crate::models::Config =
            serde_json::from_value(raw).expect("legacy config should load");
        assert!(cfg.dev.auto_start_docker);
        assert_eq!(cfg.dev.proxy_port, 7100);
        assert_eq!(cfg.dev.control_port, 7101);
        assert_eq!(cfg.dev.postgres_image, "postgres:16");
        assert_eq!(cfg.dev.redis_image, "redis:7");
    }

    #[test]
    fn config_dev_section_overrides_apply() {
        let raw = serde_json::json!({
            "editor": "vim",
            "git_host": "https://github.com",
            "base_root": "~/work",
            "current_workspace": "default",
            "dev": {
                "auto_start_docker": false,
                "proxy_port": 8080,
                "postgres_image": "postgres:15"
            }
        });
        let cfg: crate::models::Config = serde_json::from_value(raw).expect("config should load");
        assert!(!cfg.dev.auto_start_docker);
        assert_eq!(cfg.dev.proxy_port, 8080);
        assert_eq!(cfg.dev.postgres_image, "postgres:15");
        // unspecified fields fall back to defaults
        assert_eq!(cfg.dev.control_port, 7101);
        assert_eq!(cfg.dev.redis_image, "redis:7");
    }
}
