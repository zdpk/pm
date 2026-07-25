use thiserror::Error;

#[derive(Error, Debug)]
pub enum PmError {
    #[error("PM not initialized. Run 'pm init' first.")]
    NotInitialized,

    #[error("PM already initialized. Use --force to overwrite.")]
    AlreadyInitialized,

    #[error("Project '{0}' not found")]
    ProjectNotFound(String),

    #[error("Project '{0}' already exists")]
    ProjectExists(String),

    #[error("Workspace '{0}' not found")]
    WorkspaceNotFound(String),

    #[error("Workspace '{0}' already exists")]
    WorkspaceExists(String),

    #[error("Cannot remove default workspace")]
    CannotRemoveDefault,

    #[error("Cannot remove system workspace '{0}'")]
    CannotRemoveSystem(String),

    #[error("Path does not exist: {0}")]
    PathNotFound(String),

    #[error("Not a directory: {0}")]
    NotADirectory(String),

    #[error("'{0}' is not a git repository. `pm link` only links existing git repositories.")]
    NotAGitRepo(String),

    #[error("Convention path '{0}' is already occupied; refusing to create a link there.")]
    LinkPathOccupied(String),

    // Constructed only in the `#[cfg(not(unix))]` link path; unused on Unix.
    #[allow(dead_code)]
    #[error("`pm link` is supported only on Unix platforms (symlink creation requires it).")]
    LinkNotSupported,

    #[error("Invalid project name '{0}': must match [a-zA-Z][a-zA-Z0-9_-]*")]
    InvalidProjectName(String),

    #[error("Invalid workspace name '{0}': must match [a-zA-Z][a-zA-Z0-9_-]*")]
    InvalidWorkspaceName(String),

    #[error("Invalid project dir '{dir}': {rule}")]
    InvalidProjectDir { dir: String, rule: &'static str },

    #[error("Manifest version {0} is not compatible with this build (requires v4). pm no longer supports automatic migration; recreate workspaces under the new path convention.")]
    ManifestVersionTooOld(u32),

    #[error("Manifest uses unsupported field '{field}' on workspace '{workspace}'. The strict path convention does not allow per-workspace root overrides.")]
    UnsupportedManifestField {
        workspace: String,
        field: &'static str,
    },

    #[error("No remote URL saved for '{0}'")]
    NoRemoteUrl(String),

    #[error("Project '{0}' is registered but missing on disk. Run 'pm sync' to restore it, or 'pm rm {0}' to drop it from the manifest.")]
    ProjectMissing(String),

    #[error(
        "Non-interactive mode cannot restore missing project '{0}'. Run 'pm sw {0}' in a TTY or 'pm sync'."
    )]
    NonInteractiveRestore(String),

    #[error(
        "Non-interactive mode cannot update remote metadata. Run 'pm sync --remotes --yes' or inspect with '--dry-run'."
    )]
    NonInteractiveRemoteSync,

    #[error("Config repo not configured. Set config_repo in config.json.")]
    ConfigRepoNotConfigured,

    #[allow(dead_code)]
    #[error("Config repo not found at '{0}'. Run 'pm project update'.")]
    ConfigRepoCacheMissing(String),

    #[error("No .project.yaml found. Run 'pm project init' first.")]
    ProjNotInitialized,

    #[error("Unsupported language: '{0}'")]
    UnsupportedLanguage(String),

    #[error("Unsupported framework '{0}' for language '{1}'")]
    UnsupportedFramework(String, String),

    #[error("Config repo update failed: {0}")]
    ConfigRepoUpdateFailed(String),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Repo spec '{0}' not found")]
    RepoSpecNotFound(String),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
