use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// config.json schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_editor")]
    pub editor: String,

    #[serde(default = "default_git_host")]
    pub git_host: String,

    #[serde(default)]
    pub display: DisplayConfig,

    #[serde(default)]
    pub git: GitConfig,

    #[serde(default)]
    pub git_hooks: GitHooksConfig,
}

fn default_editor() -> String {
    "code".to_string()
}

fn default_git_host() -> String {
    "https://github.com".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: default_editor(),
            git_host: default_git_host(),
            display: DisplayConfig::default(),
            git: GitConfig::default(),
            git_hooks: GitHooksConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplayConfig {
    #[serde(default)]
    pub show_full_path: bool,

    #[serde(default = "default_true")]
    pub color: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitConfig {
    #[serde(default)]
    pub auto_fetch: bool,

    #[serde(default = "default_fetch_interval")]
    pub fetch_interval: u64,
}

fn default_fetch_interval() -> u64 {
    3600
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitHooksConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub auto_install: bool,
}

/// projects.json schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectsData {
    pub version: u32,
    pub projects: Vec<Project>,
}

impl Default for ProjectsData {
    fn default() -> Self {
        Self {
            version: 1,
            projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    pub added_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,

    #[serde(default)]
    pub access_count: u32,
}

impl Project {
    pub fn new(name: String, path: String) -> Self {
        let now = Utc::now();
        Self {
            name,
            path,
            remote: None,
            tags: Vec::new(),
            note: None,
            added_at: now,
            last_accessed: now,
            access_count: 0,
        }
    }
}

/// workspaces.json schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacesData {
    pub version: u32,
    pub current: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_project: Option<String>,
    pub workspaces: Vec<Workspace>,
}

impl Default for WorkspacesData {
    fn default() -> Self {
        Self {
            version: 1,
            current: "default".to_string(),
            current_project: None,
            workspaces: vec![
                Workspace::new("default".to_string()),
                Workspace::new_system(".trash".to_string()),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub projects: Vec<String>,
    pub created_at: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub git: HashMap<String, String>,
}

impl Workspace {
    pub fn new(name: String) -> Self {
        Self {
            name,
            projects: Vec::new(),
            created_at: Utc::now(),
            git: HashMap::new(),
        }
    }

    pub fn new_system(name: String) -> Self {
        Self {
            name,
            projects: Vec::new(),
            created_at: Utc::now(),
            git: HashMap::new(),
        }
    }

    pub fn is_system(&self) -> bool {
        self.name.starts_with('.')
    }
}
