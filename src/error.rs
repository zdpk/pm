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

    #[error("Invalid project name '{0}': must match [a-zA-Z][a-zA-Z0-9_-]*")]
    InvalidProjectName(String),

    #[error("Invalid workspace name '{0}': must match [a-zA-Z][a-zA-Z0-9_-]*")]
    InvalidWorkspaceName(String),

    #[error("No remote URL saved for '{0}'")]
    NoRemoteUrl(String),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
