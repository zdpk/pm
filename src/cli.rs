use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "pm")]
#[command(about = "Git project directory manager", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize PM configuration
    Init {
        /// Overwrite existing configuration
        #[arg(short, long)]
        force: bool,
    },

    /// Add existing directory as a project
    Add {
        /// Project path (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Project name (default: directory name)
        #[arg(short, long)]
        name: Option<String>,

        /// Tags (comma separated)
        #[arg(short, long)]
        tags: Option<String>,

        /// Note
        #[arg(long)]
        note: Option<String>,

        /// Overwrite if already registered
        #[arg(short, long)]
        force: bool,
    },

    /// List projects
    #[command(visible_alias = "ls")]
    List {
        /// Show all workspaces
        #[arg(short, long)]
        all: bool,

        /// Filter by tags
        #[arg(short, long)]
        tags: Option<String>,

        /// Show full path
        #[arg(short, long)]
        path: bool,

        /// Skip git status
        #[arg(long)]
        no_status: bool,

        /// Sort by field
        #[arg(short, long, value_enum, default_value = "accessed")]
        sort: SortField,

        /// Reverse sort order
        #[arg(short, long)]
        reverse: bool,

        /// Filter projects
        #[arg(short, long, value_enum)]
        filter: Option<FilterType>,
    },

    /// Switch to a project directory
    #[command(visible_alias = "sw")]
    Switch {
        /// Project name or @workspace/project
        target: String,
    },

    /// Switch workspace
    Use {
        /// Workspace name
        workspace: String,
    },

    /// Print project path (for shell integration)
    Path {
        /// Project name or @workspace/project
        target: String,
    },

    /// Remove a project
    #[command(visible_alias = "rm")]
    Remove {
        /// Project name
        project: String,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,

        /// Move to trash
        #[arg(short, long)]
        force: bool,

        /// Delete files too (requires -f)
        #[arg(short = 'r', long)]
        recursive: bool,
    },

    /// Workspace management
    #[command(visible_alias = "ws", subcommand)]
    Workspace(WorkspaceCommand),

    /// Synchronize missing repositories from manifest
    Sync {
        /// Sync a single workspace
        workspace: Option<String>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,

        /// Parallel jobs
        #[arg(long, default_value_t = 4)]
        jobs: usize,
    },

    /// Manifest management
    #[command(subcommand)]
    Manifest(ManifestCommand),

    /// Generate shell completion script
    Completion {
        /// Shell type
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Show project removal history
    History {
        /// Maximum number of entries to show
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
    },

    /// Check all project paths
    Check,

    /// Manage plugins
    #[command(subcommand)]
    Plugin(PluginCommand),
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// List installed command plugins
    #[command(visible_alias = "ls")]
    List,

    /// Enable a plugin
    Enable {
        /// Plugin name
        name: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// List workspaces
    #[command(visible_alias = "ls")]
    List,

    /// Create a new workspace
    New {
        /// Workspace name
        name: String,

        /// Workspace root path
        #[arg(long)]
        root: Option<String>,
    },

    /// Remove a workspace
    #[command(visible_alias = "rm")]
    Remove {
        /// Workspace name
        name: String,

        /// Unregister projects (keep files)
        #[arg(short, long)]
        force: bool,

        /// Delete project files too
        #[arg(short = 'r', long)]
        recursive: bool,
    },

    /// Move project to workspace
    #[command(visible_alias = "mv")]
    Move {
        /// Project names
        #[arg(required = true)]
        projects: Vec<String>,

        /// Target workspace
        #[arg(last = true)]
        workspace: String,
    },

    /// Configure workspace git settings
    Config {
        /// Workspace name
        workspace: String,

        /// Config key
        key: Option<String>,

        /// Config value
        value: Option<String>,

        /// List all settings
        #[arg(long)]
        list: bool,

        /// Unset a key
        #[arg(long)]
        unset: Option<String>,
    },

    /// Apply git config to all projects in workspace
    ApplyGit {
        /// Workspace name
        workspace: String,
    },

    /// Configure workspace root
    Root {
        #[command(subcommand)]
        command: WorkspaceRootCommand,
    },
}

#[derive(Subcommand)]
pub enum WorkspaceRootCommand {
    /// Set workspace root
    Set {
        /// Workspace name
        workspace: String,

        /// Root path
        path: String,
    },
}

#[derive(Subcommand)]
pub enum ManifestCommand {
    /// Migrate legacy projects/workspaces files into manifest.json
    Migrate,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SortField {
    /// Last accessed
    Accessed,
    /// Project name
    Name,
    /// Path
    Path,
    /// Date added
    Added,
    /// Access frequency
    Frequency,
    /// Git status
    Status,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FilterType {
    /// Git repositories only
    Git,
    /// Non-git directories
    NonGit,
    /// Invalid paths
    Orphan,
}
