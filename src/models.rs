use chrono::{DateTime, Utc};
use clap::ValueEnum;
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

    #[serde(default = "default_base_root")]
    pub base_root: String,

    #[serde(default = "default_workspace")]
    pub current_workspace: String,

    #[serde(default)]
    pub current_project: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_repo: Option<ConfigRepoSettings>,
}

fn default_editor() -> String {
    "code".to_string()
}

fn default_git_host() -> String {
    "https://github.com".to_string()
}

fn default_base_root() -> String {
    "~/".to_string()
}

fn default_workspace() -> String {
    "default".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: default_editor(),
            git_host: default_git_host(),
            display: DisplayConfig::default(),
            git: GitConfig::default(),
            git_hooks: GitHooksConfig::default(),
            base_root: default_base_root(),
            current_workspace: default_workspace(),
            current_project: None,
            config_repo: None,
        }
    }
}

/// Config repo settings for proj config management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigRepoSettings {
    pub url: String,

    #[serde(default = "default_config_repo_cache")]
    pub cache_dir: String,
}

fn default_config_repo_cache() -> String {
    "~/.config/pm/config-repo".to_string()
}

/// Cached proj metadata stored in manifest.json per project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjMeta {
    pub language: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,

    pub config_version: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryData {
    pub version: u32,
    pub entries: Vec<HistoryEntry>,
}

impl Default for HistoryData {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub action: HistoryAction,
    pub project: HistoryProjectSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    Unregistered,
    Trashed,
    Deleted,
}

impl HistoryAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unregistered => "unregistered",
            Self::Trashed => "trashed",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryProjectSnapshot {
    pub name: String,
    pub workspace: String,
    pub repo_slug: String,
    pub dir: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,

    pub path: String,
}

/// manifest.json schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub workspaces: Vec<Workspace>,
    pub projects: Vec<Project>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: 2,
            workspaces: vec![
                Workspace::new("default".to_string(), None),
                Workspace::new_system(".trash".to_string(), None),
            ],
            projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,

    pub created_at: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub git: HashMap<String, String>,
}

impl Workspace {
    pub fn new(name: String, root: Option<String>) -> Self {
        Self {
            name,
            root,
            created_at: Utc::now(),
            git: HashMap::new(),
        }
    }

    pub fn new_system(name: String, root: Option<String>) -> Self {
        Self {
            name,
            root,
            created_at: Utc::now(),
            git: HashMap::new(),
        }
    }

    pub fn is_system(&self) -> bool {
        self.name.starts_with('.')
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub workspace: String,
    pub repo_slug: String,
    pub dir: String,

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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proj: Option<ProjMeta>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_spec: Option<RepoSpecMetadata>,
}

impl Project {
    pub fn new(name: String, workspace: String, repo_slug: String, dir: String) -> Self {
        let now = Utc::now();
        Self {
            name,
            workspace,
            repo_slug,
            dir,
            remote: None,
            tags: Vec::new(),
            note: None,
            added_at: now,
            last_accessed: now,
            access_count: 0,
            proj: None,
            repo_spec: None,
        }
    }
}

/// Repo spec applied to a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSpecMetadata {
    pub id: String,
    pub version: String,
    pub applied_at: DateTime<Utc>,
}

/// Repo initialization spec registry entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSpec {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum PortKind {
    Frontend,
    Backend,
    Database,
    Redis,
    Infra,
}

impl PortKind {
    pub fn service_key(self) -> &'static str {
        match self {
            Self::Frontend => "front",
            Self::Backend => "back",
            Self::Database => "db",
            Self::Redis => "redis",
            Self::Infra => "infra",
        }
    }

    /// Whether this kind is provided by a shared local instance instead of
    /// a per-project port allocation.
    pub fn is_shared(self) -> bool {
        matches!(self, Self::Database | Self::Redis)
    }

    pub fn env_key(self) -> &'static str {
        match self {
            Self::Frontend => "FRONTEND_PORT",
            Self::Backend => "APP_PORT",
            Self::Database => "LOCAL_POSTGRES_PORT",
            Self::Redis => "LOCAL_REDIS_PORT",
            Self::Infra => "LOCAL_INFRA_PORT",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::Backend => "backend",
            Self::Database => "database",
            Self::Redis => "redis",
            Self::Infra => "infra",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortService {
    pub kind: PortKind,
    pub env: String,
    pub port: u16,

    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortProject {
    pub workspace: String,
    pub project: String,
    pub path: String,
    pub services: HashMap<String, PortService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedInfra {
    pub postgres_port: u16,
    pub redis_port: u16,
}

impl Default for SharedInfra {
    fn default() -> Self {
        Self {
            postgres_port: 5432,
            redis_port: 6379,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortsData {
    pub version: u32,
    #[serde(default)]
    pub shared: SharedInfra,
    pub ranges: HashMap<PortKind, PortRange>,
    pub projects: HashMap<String, PortProject>,
}

impl Default for PortsData {
    fn default() -> Self {
        let mut ranges = HashMap::new();
        ranges.insert(
            PortKind::Frontend,
            PortRange {
                start: 10000,
                end: 19999,
            },
        );
        ranges.insert(
            PortKind::Backend,
            PortRange {
                start: 20000,
                end: 29999,
            },
        );
        ranges.insert(
            PortKind::Infra,
            PortRange {
                start: 45000,
                end: 49999,
            },
        );

        Self {
            version: 2,
            shared: SharedInfra::default(),
            ranges,
            projects: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyProjectsData {
    pub version: u32,
    pub projects: Vec<LegacyProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyProject {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyWorkspacesData {
    pub version: u32,
    pub current: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_project: Option<String>,
    pub workspaces: Vec<LegacyWorkspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyWorkspace {
    pub name: String,
    pub projects: Vec<String>,
    pub created_at: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub git: HashMap<String, String>,
}
