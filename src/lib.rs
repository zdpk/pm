use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub mod backup;
pub mod commands;
pub mod config;
pub mod constants;
pub mod display;
pub mod error;
pub mod extensions;
pub mod shell_integration;
pub mod tag_commands;
pub mod utils;
pub mod validation;

pub use error::PmError;

pub use commands::config::ExportFormat;
pub use commands::{backup as backup_cmd, config as config_cmd, init, project, status, tag};
pub use config::load_config;
pub use constants::*;
pub use display::display_error;
pub use error::handle_error;

pub fn handle_config_error(e: anyhow::Error) -> ! {
    if e.to_string().contains("Configuration file not found") {
        display_error("PM not initialized", "Configuration file not found");
        println!("\n💡 Please initialize PM first:");
        println!("   {} init", utils::get_binary_name());
        std::process::exit(1);
    } else {
        handle_error(e, ERROR_CONFIG_LOAD);
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None, disable_help_subcommand = true, disable_version_flag = true)]
pub struct Cli {
    /// Show version information
    #[arg(short = 'v', long = "version")]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a new project to manage (alias: a)
    #[command(alias = "a")]
    Add {
        /// Path to the project directory
        path: PathBuf,

        #[arg(short, long)]
        name: Option<String>,

        #[arg(short, long, value_delimiter = ',')]
        tags: Vec<String>,

        #[arg(short, long)]
        description: Option<String>,
    },

    /// Clone repositories from GitHub (interactive browse or direct clone) (alias: cl)
    #[command(alias = "cl")]
    Clone {
        /// Repository in format owner/repo (optional for interactive browse)
        repo: Option<String>,

        /// Target directory (defaults to <current_dir>/<owner>/<repo>)
        #[arg(short, long)]
        directory: Option<PathBuf>,
    },

    /// List managed projects (alias: ls)
    #[command(alias = "ls")]
    List {
        /// Filter projects by tags (comma-separated, all tags must match)
        #[arg(short = 't', long, value_delimiter = ',')]
        tags: Vec<String>,

        /// Filter projects by tags (comma-separated, any tag can match)
        #[arg(long, value_delimiter = ',')]
        tags_any: Vec<String>,

        /// Show only projects updated within the last time period (e.g., 7d, 2w, 1m, 1y)
        #[arg(short = 'r', long)]
        recent: Option<String>,

        /// Limit the number of results
        #[arg(short = 'l', long)]
        limit: Option<usize>,

        /// Show verbose information
        #[arg(short = 'v', long)]
        verbose: bool,
    },

    /// Switch to a project directory (alias: sw)
    #[command(alias = "sw")]
    Switch { name: String },

    /// Scan for Git repositories and add them to PM (alias: sc)
    #[command(alias = "sc")]
    Scan {
        /// Directory to scan (defaults to current directory)
        #[arg(short, long)]
        directory: Option<PathBuf>,

        /// Show all repositories found, don't prompt for selection
        #[arg(long)]
        show_all: bool,
    },

    /// Manage project tags (alias: t)
    #[command(alias = "t")]
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },

    /// Remove projects from PM (alias: rm)
    #[command(alias = "rm")]
    Remove {
        /// Project name (optional for interactive mode)
        project: Option<String>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Manage configuration (alias: cf)
    #[command(alias = "cf")]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommands>,
    },

    /// Manage backups (list, restore, clean) (alias: bk)
    #[command(alias = "bk")]
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },

    /// Initialize PM with basic configuration
    Init {
        /// Skip initialization if config exists (non-interactive)
        #[arg(long)]
        skip: bool,

        /// Replace existing config with backup (non-interactive)
        #[arg(long)]
        replace: bool,
    },

    /// Show current project status (for prompt integration)
    Status {
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        format: String,

        /// Quiet mode for prompt integration
        #[arg(short, long)]
        quiet: bool,
    },

    /// Manage extensions (alias: ext)
    #[command(alias = "ext")]
    Extension {
        #[command(subcommand)]
        action: ExtensionAction,
    },

    /// Run an installed extension (alias: r)
    #[command(alias = "r")]
    Run {
        /// Extension name
        extension: String,
        /// Arguments to pass to the extension
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// External extension commands
    #[command(external_subcommand)]
    External(Vec<String>),
}


#[derive(Subcommand)]
pub enum BackupAction {
    /// List all available backups
    List,

    /// Restore a specific backup or select interactively
    Restore {
        /// Backup ID to restore (optional for interactive mode)
        backup_id: Option<String>,

        /// Skip confirmation prompt
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Clean old backups (keep most recent N)
    Clean {
        /// Number of backups to keep
        #[arg(default_value = "5")]
        keep: usize,

        /// Skip confirmation prompt
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Show backup system status
    Status,
}

#[derive(Subcommand)]
pub enum TagAction {
    /// Add tags to a project
    Add {
        /// The name of the project
        project_name: String,
        /// The tags to add
        #[arg(required = true)]
        tags: Vec<String>,
    },
    /// Remove tags from a project
    #[command(alias = "rm")]
    Remove {
        /// The name of the project
        project_name: String,
        /// The tags to remove
        #[arg(required = true)]
        tags: Vec<String>,
    },
    /// List all unique tags and their counts
    #[command(alias = "ls")]
    List {},
    /// Show tags for a specific project
    Show {
        /// The name of the project (optional, defaults to current directory's project)
        project_name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show {},

    /// Edit configuration file
    Edit {},

    /// Validate configuration file
    Validate {},

    /// Reset configuration to defaults
    Reset {},

    /// Get a specific configuration value
    Get {
        /// Configuration key (supports dot notation like 'settings.show_git_status')
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key (supports dot notation like 'settings.show_git_status')
        key: String,
        /// New value
        value: String,
    },

    /// List all available configuration keys
    List {},

    /// Backup and restore operations
    #[command(subcommand)]
    Backup(BackupCommands),

    /// Template operations
    #[command(subcommand)]
    Template(TemplateCommands),

    /// Interactive configuration setup
    Setup {
        /// Use quick setup with defaults
        #[arg(long)]
        quick: bool,
    },

    /// Export configuration
    Export {
        /// Output format
        #[arg(long, value_enum, default_value = "yaml")]
        format: ExportFormat,
        /// Output file path
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Import configuration from file
    Import {
        /// Input file path
        file: PathBuf,
        /// Force import without backup
        #[arg(long)]
        force: bool,
    },

    /// Show differences between current config and backup
    Diff {
        /// Backup name to compare with (defaults to latest)
        backup: Option<String>,
    },

    /// Show configuration change history
    History {
        /// Limit number of entries
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}

#[derive(Subcommand)]
pub enum BackupCommands {
    /// Create a backup of current configuration
    Create {
        /// Backup name (optional, defaults to timestamp)
        #[arg(long)]
        name: Option<String>,
    },
    /// Restore configuration from backup
    Restore {
        /// Backup name
        name: String,
    },
    /// List all available backups
    List {},
    /// Delete a backup
    Delete {
        /// Backup name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum TemplateCommands {
    /// List available templates
    List {},
    /// Apply a template
    Apply {
        /// Template name
        name: String,
    },
    /// Save current configuration as template
    Save {
        /// Template name
        name: String,
        /// Template description
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete a template
    Delete {
        /// Template name
        name: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExtensionType {
    Bash,
    Python,
    Rust,
}

#[derive(Subcommand)]
pub enum ExtensionAction {
    /// Create a new extension
    Create {
        /// Extension name
        name: String,
        /// Extension type (bash, python, rust)
        #[arg(short = 't', long, value_enum)]
        ext_type: Option<ExtensionType>,
        /// Target directory (defaults to current directory)
        #[arg(short = 'd', long)]
        directory: Option<PathBuf>,
        /// Extension description
        #[arg(short = 's', long)]
        description: Option<String>,
        /// Author name
        #[arg(short, long)]
        author: Option<String>,
        /// Skip interactive prompts (use defaults)
        #[arg(long)]
        non_interactive: bool,
    },
    /// Install an extension
    Install {
        /// Extension name or path (use "." for current directory)
        name: String,
        /// Installation source (URL, GitHub repo, or local path)
        #[arg(long)]
        source: Option<String>,
        /// Specific version to install
        #[arg(long)]
        version: Option<String>,
        /// Install from local directory path (supports relative paths)
        #[arg(long)]
        local: bool,
        /// Registry to install from (defaults to configured registries)
        #[arg(long)]
        registry: Option<String>,
        /// Force reinstallation if already installed
        #[arg(long)]
        force: bool,
    },
    /// Uninstall an extension (alias: remove, rm)
    #[command(alias = "remove", alias = "rm")]
    Uninstall {
        /// Extension name
        name: String,
        /// Force removal without confirmation
        #[arg(long)]
        force: bool,
    },
    /// List installed extensions
    #[command(alias = "ls")]
    List {
        /// Show all available extensions (not just installed)
        #[arg(long)]
        all: bool,
    },
    /// Show extension information
    Info {
        /// Extension name
        name: String,
    },
    /// Update extensions
    Update {
        /// Extension name (update all if not specified)
        name: Option<String>,
    },
    /// Search for extensions
    Search {
        /// Search query
        query: String,
        /// Registry to search in (defaults to all configured registries)
        #[arg(long)]
        registry: Option<String>,
        /// Filter by category
        #[arg(long)]
        category: Option<String>,
        /// Filter by author
        #[arg(long)]
        author: Option<String>,
        /// Sort results by (downloads, updated, created, name)
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Manage extension registries
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },
}


/// Registry management actions
#[derive(Subcommand)]
pub enum RegistryAction {
    /// Add a new registry
    Add {
        /// Registry name
        name: String,
        /// Registry URL
        url: String,
        /// Authentication token
        #[arg(long)]
        token: Option<String>,
        /// Set as default registry
        #[arg(long)]
        default: bool,
    },
    /// Remove a registry
    Remove {
        /// Registry name
        name: String,
    },
    /// List configured registries
    List,
    /// Set default registry
    Default {
        /// Registry name
        name: String,
    },
    /// Test registry connectivity
    Ping {
        /// Registry name (defaults to all registries)
        name: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[schemars(title = "Project", description = "A managed project")]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub git_updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    #[schemars(description = "Whether this project is a Git repository")]
    pub is_git_repository: bool,
}

#[derive(Serialize, Deserialize, Debug, Default, JsonSchema)]
#[schemars(
    title = "Machine Metadata",
    description = "Machine-specific project metadata"
)]
pub struct MachineMetadata {
    pub last_accessed: std::collections::HashMap<Uuid, DateTime<Utc>>,
    pub access_counts: std::collections::HashMap<Uuid, u32>,
}

/// Common CLI handling logic
pub async fn handle_command(command: &Commands) -> anyhow::Result<()> {
    match command {
        Commands::Add {
            path,
            name,
            tags,
            description,
        } => project::handle_add(path, name, tags, description).await,
        Commands::Clone { repo, directory } => {
            project::handle_clone(repo.as_deref(), directory.as_deref()).await
        }
        Commands::List {
            tags,
            tags_any,
            recent,
            limit,
            verbose,
        } => project::handle_list(tags, tags_any, recent, limit, *verbose).await,
        Commands::Switch { name } => {
            let mut config = load_config().await?;
            project::handle_switch(&mut config, name).await
        }
        Commands::Scan {
            directory,
            show_all,
        } => project::handle_scan(directory.as_deref(), *show_all)
            .await
            .map(|_| ()),
        Commands::Tag { action } => match action {
            TagAction::Add { project_name, tags } => tag::handle_tag_add(project_name, tags).await,
            TagAction::Remove { project_name, tags } => {
                tag::handle_tag_remove(project_name, tags).await
            }
            TagAction::List {} => tag::handle_tag_list().await,
            TagAction::Show { project_name } => tag::handle_tag_show(project_name.as_deref()).await,
        },
        Commands::Remove { project, yes } => project::handle_remove(project.as_deref(), *yes).await,
        Commands::Config { command } => {
            match command.as_ref().unwrap_or(&ConfigCommands::Show {}) {
                ConfigCommands::Show {} => config_cmd::handle_show().await,
                ConfigCommands::Edit {} => config_cmd::handle_edit().await,
                ConfigCommands::Validate {} => config_cmd::handle_validate().await,
                ConfigCommands::Reset {} => config_cmd::handle_reset().await,
                ConfigCommands::Get { key } => config_cmd::handle_get(key).await,
                ConfigCommands::Set { key, value } => config_cmd::handle_set(key, value).await,
                ConfigCommands::List {} => config_cmd::handle_list().await,
                ConfigCommands::Backup(backup_command) => match backup_command {
                    BackupCommands::Create { name } => {
                        config_cmd::handle_backup_create(name.as_deref()).await
                    }
                    BackupCommands::Restore { name } => {
                        config_cmd::handle_backup_restore(name).await
                    }
                    BackupCommands::List {} => config_cmd::handle_backup_list().await,
                    BackupCommands::Delete { name } => config_cmd::handle_backup_delete(name).await,
                },
                ConfigCommands::Template(template_command) => match template_command {
                    TemplateCommands::List {} => config_cmd::handle_template_list().await,
                    TemplateCommands::Apply { name } => {
                        config_cmd::handle_template_apply(name).await
                    }
                    TemplateCommands::Save { name, description } => {
                        config_cmd::handle_template_save(name, description.as_deref()).await
                    }
                    TemplateCommands::Delete { name } => {
                        config_cmd::handle_template_delete(name).await
                    }
                },
                ConfigCommands::Setup { quick } => config_cmd::handle_setup(*quick).await,
                ConfigCommands::Export { format, file } => {
                    config_cmd::handle_export(format, file.as_deref()).await
                }
                ConfigCommands::Import { file, force } => {
                    config_cmd::handle_import(file, *force).await
                }
                ConfigCommands::Diff { backup } => config_cmd::handle_diff(backup.as_deref()).await,
                ConfigCommands::History { limit } => config_cmd::handle_history(*limit).await,
            }
        }
        Commands::Backup { action } => match action {
            BackupAction::List => backup_cmd::handle_backup_list().await,
            BackupAction::Restore { backup_id, force } => {
                if let Some(id) = backup_id {
                    backup_cmd::handle_backup_restore(id, *force).await
                } else {
                    backup_cmd::handle_backup_restore_interactive().await
                }
            }
            BackupAction::Clean { keep, force } => {
                backup_cmd::handle_backup_clean(*keep, *force).await
            }
            BackupAction::Status => backup_cmd::handle_backup_status().await,
        },
        Commands::Init { skip, replace } => init::handle_init(*skip, *replace).await,
        Commands::Status { format, quiet } => status::handle_status(format, *quiet).await,
        Commands::Extension { action } => {
            // Handle extension management commands
            extensions::handle_extension_command(action).await
        }
        Commands::Run { extension, args } => {
            // Check for special commands
            match extension.as_str() {
                "-h" | "--help" | "help" => {
                    extensions::show_run_help().await
                }
                "ls" | "list" => {
                    extensions::list_extensions().await
                }
                _ => {
                    // Handle normal extension execution
                    let mut extension_args = vec![extension.clone()];
                    extension_args.extend(args.clone());
                    extensions::execute_extension_command(&extension_args).await
                }
            }
        }
        Commands::External(args) => {
            // Handle external extension commands
            extensions::execute_extension_command(args).await
        }
    }
}
