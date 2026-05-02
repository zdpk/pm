use crate::models::PortKind;
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

    /// Repo spec version tracking
    #[command(subcommand)]
    Repo(RepoCommand),

    /// Local port management
    #[command(subcommand)]
    Ports(PortsCommand),

    /// Run services or arbitrary commands in a project's environment.
    ///
    /// In v0.4.0, `pm run` has two modes that share the same grammar:
    ///
    /// - **Orchestrator** (when `.proj.yaml` defines `services:`): spawns
    ///   one or all services with the daemon registered to route
    ///   `<service>.<project>.<workspace>.localhost` to each upstream port.
    ///   Examples: `pm run`, `pm run front`, `pm run back api`.
    ///
    /// - **Legacy** (when `--` is present): runs an arbitrary command in
    ///   the project directory with port-related env vars injected.
    ///   Example: `pm run myproj -- pnpm dev`. v0.3.0 behavior preserved.
    Run {
        /// Project / service identifiers (orchestrator mode), or project
        /// name (legacy mode). Up to 2 positional args supported.
        positional: Vec<String>,

        /// Command to execute (legacy mode). Pass after `--`.
        #[arg(last = true)]
        command: Vec<String>,
    },

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

    /// Project config file management
    #[command(visible_alias = "p", subcommand)]
    Project(ProjectCommand),

    /// Tail the log of a running service.
    Logs {
        /// Service identifier (e.g. front, back). Required when the project
        /// has more than one running service.
        service: Option<String>,

        /// Project name (default: current project). Use `<workspace>/<project>`
        /// or `@workspace/project` to disambiguate.
        project: Option<String>,
    },

    /// Stop running services spawned by `pm run`.
    Stop {
        /// Service identifier. Omit to stop all services in the project.
        service: Option<String>,

        /// Project name (default: current project).
        project: Option<String>,
    },

    /// Manage shared local Postgres / Redis containers
    #[command(subcommand)]
    Db(DbCommand),

    /// Manage the local-dev-orchestrator daemon
    #[command(subcommand)]
    Proxy(ProxyCommand),

    /// (internal) entrypoint for the orchestrator daemon. Hidden from help.
    #[command(name = "__daemon", hide = true)]
    Daemon {
        /// Run in foreground (do not detach). Useful for debugging.
        #[arg(long)]
        foreground: bool,
    },

    /// Upgrade PM to the latest version
    Upgrade,
}

#[derive(Subcommand)]
pub enum ProxyCommand {
    /// Show daemon status (PID, uptime, route count)
    Status,

    /// Start the daemon explicitly (auto-spawned by `pm run` otherwise)
    Start {
        /// Run in foreground for debugging
        #[arg(long)]
        foreground: bool,
    },

    /// Stop the daemon (graceful shutdown)
    Stop,
}

#[derive(Subcommand)]
pub enum DbCommand {
    /// Show shared Postgres / Redis container status
    Status,

    /// Start shared Postgres / Redis containers
    Start,

    /// Stop shared Postgres / Redis containers (volumes preserved)
    Stop,
}

#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Initialize project with config files from the config repo
    Init {
        /// Language (rust, ts, python, dart, c)
        #[arg(short, long)]
        language: Option<String>,

        /// Framework (axum, clap, nextjs, nestjs, fastapi, flutter)
        #[arg(short, long)]
        framework: Option<String>,

        /// Include CI/CD workflows
        #[arg(long)]
        ci: bool,

        /// Include Dockerfile
        #[arg(long)]
        docker: bool,

        /// Include pre-commit hooks
        #[arg(long)]
        hooks: bool,

        /// Include everything (ci + docker + hooks)
        #[arg(long, conflicts_with_all = ["ci", "docker", "hooks"])]
        all: bool,

        /// Skip all prompts (non-interactive mode)
        #[arg(short = 'y', long = "no-interactive")]
        yes: bool,

        /// Do not auto-add a default `services:` section to .proj.yaml.
        /// Default behaviour: a single service named after the framework's
        /// kind (e.g. `front` for nextjs) is registered so `pm run` works
        /// out of the box.
        #[arg(long = "no-services")]
        no_services: bool,
    },

    /// Sync config files to latest config repo version
    Sync {
        /// Sync all registered projects
        #[arg(long)]
        all: bool,

        /// Preview changes without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// Check if config files are outdated
    Check {
        /// Check all registered projects
        #[arg(long)]
        all: bool,
    },

    /// Show diff between local and upstream config files
    Diff,

    /// Pull latest config repo
    Update,

    /// Register project for config management (without copying files)
    Add {
        /// Language (rust, ts, python, dart, c)
        #[arg(short, long)]
        language: Option<String>,

        /// Framework (axum, clap, nextjs, nestjs, fastapi, flutter)
        #[arg(short, long)]
        framework: Option<String>,
    },

    /// List projects managed by proj
    #[command(visible_alias = "ls")]
    List,

    /// Synthesize the project's `.gitignore` from bundled templates.
    ///
    /// Writes the file in place by default. With `--diff` only prints the
    /// unified diff between the current file and the freshly synthesized
    /// result without writing. With `--categories` the user overrides the
    /// default category selection (defaults are derived from
    /// `.proj.yaml`'s `language` and `framework`).
    Gitignore {
        /// Show diff against the current `.gitignore` without writing.
        #[arg(long)]
        diff: bool,

        /// Comma-separated category list (e.g. `rust,macos,vscode`).
        /// Overrides the default selection. Use `pm project gitignore`
        /// without flags to see the auto-derived defaults.
        #[arg(long)]
        categories: Option<String>,
    },
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

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Repo spec registry
    #[command(subcommand)]
    Spec(RepoSpecCommand),

    /// Track repo spec version for a project
    Track {
        /// Project name in current workspace
        project: String,

        /// Repo spec id
        #[arg(long)]
        spec: String,

        /// Version to record (default: current spec version)
        #[arg(long)]
        version: Option<String>,
    },

    /// Show repo spec status for a project
    Status {
        /// Project name (default: current project)
        project: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RepoSpecCommand {
    /// List registered repo specs
    #[command(visible_alias = "ls")]
    List,

    /// Show repo spec details
    Show {
        /// Repo spec id
        id: String,
    },
}

#[derive(Subcommand)]
pub enum PortsCommand {
    /// List allocated ports
    #[command(visible_alias = "ls")]
    List,

    /// Assign stable ports to a project
    Assign {
        /// Project name (default: current project)
        project: Option<String>,

        /// Service kind to assign (repeatable)
        #[arg(short, long, value_enum)]
        kind: Vec<PortKind>,

        /// Reassign even if a port already exists
        #[arg(short, long)]
        force: bool,
    },

    /// Check port allocations
    Check {
        /// Project name (default: current project)
        project: Option<String>,

        /// Check all projects
        #[arg(long)]
        all: bool,
    },

    /// Repair duplicate port allocations
    Repair {
        /// Project name (default: current project)
        project: Option<String>,
    },

    /// Release project port allocations
    Release {
        /// Project name (default: current project)
        project: Option<String>,

        /// Service kind to release (repeatable; default: all)
        #[arg(short, long, value_enum)]
        kind: Vec<PortKind>,
    },

    /// Lock a service port against automatic repair
    Lock {
        /// Project name (default: current project)
        project: Option<String>,

        /// Service key such as back, front, db, redis, infra
        #[arg(long)]
        service: String,
    },

    /// Unlock a service port
    Unlock {
        /// Project name (default: current project)
        project: Option<String>,

        /// Service key such as back, front, db, redis, infra
        #[arg(long)]
        service: String,
    },

    /// View or update the shared local Postgres/Redis ports
    Shared {
        /// Set the shared Postgres port
        #[arg(long)]
        postgres: Option<u16>,

        /// Set the shared Redis port
        #[arg(long)]
        redis: Option<u16>,
    },
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
