use crate::error::PmError;
use crate::models::{Config, ProjectsData, WorkspacesData};
use crate::path::expand_path;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

/// Get config directory path
/// - pmd binary: ~/.config/pm-dev/
/// - pm binary: ~/.config/pm/
/// - Override: PM_CONFIG_DIR environment variable
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PM_CONFIG_DIR") {
        return expand_path(&dir);
    }

    let base = dirs::config_dir().expect("Could not determine config directory");

    // Check binary name to determine config directory
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

pub fn projects_path() -> PathBuf {
    config_dir().join("projects.json")
}

pub fn workspaces_path() -> PathBuf {
    config_dir().join("workspaces.json")
}

/// Check if PM is initialized
pub fn is_initialized() -> bool {
    config_dir().exists() && config_path().exists()
}

/// Ensure PM is initialized
pub fn ensure_initialized() -> Result<()> {
    if !is_initialized() {
        return Err(PmError::NotInitialized.into());
    }
    Ok(())
}

/// Load config.json
pub fn load_config() -> Result<Config> {
    ensure_initialized()?;
    let content = fs::read_to_string(config_path())?;
    Ok(serde_json::from_str(&content)?)
}

/// Save config.json
pub fn save_config(config: &Config) -> Result<()> {
    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path(), content)?;
    Ok(())
}

/// Load projects.json
pub fn load_projects() -> Result<ProjectsData> {
    ensure_initialized()?;
    let content = fs::read_to_string(projects_path())?;
    Ok(serde_json::from_str(&content)?)
}

/// Save projects.json
pub fn save_projects(data: &ProjectsData) -> Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    fs::write(projects_path(), content)?;
    Ok(())
}

/// Load workspaces.json
pub fn load_workspaces() -> Result<WorkspacesData> {
    ensure_initialized()?;
    let content = fs::read_to_string(workspaces_path())?;
    Ok(serde_json::from_str(&content)?)
}

/// Save workspaces.json
pub fn save_workspaces(data: &WorkspacesData) -> Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    fs::write(workspaces_path(), content)?;
    Ok(())
}
