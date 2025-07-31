use crate::backup::{BackupReason, create_backup, add_backup_entry};
use crate::config::{get_config_path, get_config_dir, save_config, Config, ConfigSettings};
use crate::constants::*;
use crate::display::*;
use crate::error::{handle_inquire_error, PmError};
use crate::shell_integration;
use crate::utils;
use anyhow::Result;
use inquire::{Confirm, Select, Text};
use std::path::{Path, PathBuf};

#[derive(Debug)]
enum ConflictAction {
    Skip,
    Replace,
    Cancel,
}

pub async fn handle_init(
    skip: bool,
    replace: bool,
) -> Result<()> {
    let config_path = get_config_path()?;
    let mut backup_entry = None;

    // Handle existing config with new skip/replace model
    if config_path.exists() {
        if skip {
            // Non-interactive skip mode
            display_success(&format!(
                "{} is already initialized",
                APP_NAME.to_uppercase()
            ));
            println!("📁 Configuration file: {}", config_path.display());
            println!("\n💡 To reinitialize with backup:");
            println!("   pm init --replace   # Backup existing and recreate");
            return Ok(());
        } else if replace {
            // Non-interactive replace mode
            println!("💾 Creating backup of existing config...");
            backup_entry = Some(create_backup(&config_path, BackupReason::InitForceRecreate).await?);
        } else {
            // Interactive mode: skip/replace/cancel
            let action = handle_config_conflict_interactive(&config_path).await?;
            
            match action {
                ConflictAction::Skip => {
                    display_success(&format!(
                        "{} is already initialized",
                        APP_NAME.to_uppercase()
                    ));
                    println!("📁 Configuration file: {}", config_path.display());
                    println!("\n💡 To reinitialize with backup:");
                    println!("   pm init --replace   # Backup existing and recreate");
                    return Ok(());
                }
                ConflictAction::Replace => {
                    println!("💾 Creating backup and recreating config...");
                    backup_entry = Some(create_backup(&config_path, BackupReason::InitConflictResolution).await?);
                }
                ConflictAction::Cancel => {
                    println!("🚫 Initialization cancelled");
                    return Ok(());
                }
            }
        }
    }

    println!("🚀 Initializing {}...\n", APP_NAME.to_uppercase());

    // Step 1: Configuration directory setup
    let config_dir_path = {
        let default_config_dir = get_config_dir()?;

        let config_input = handle_inquire_error(Text::new("Configuration directory:")
            .with_default(&default_config_dir.to_string_lossy())
            .with_help_message("Where PM configuration files will be stored (press Enter for default)")
            .prompt())?;

        PathBuf::from(shellexpand::tilde(&config_input).to_string())
    };


    // Step 3: Additional settings

    let show_git_status = handle_inquire_error(Confirm::new("Show git status in project listings?")
        .with_default(true)
        .prompt())?;


    // Create the config directory if it doesn't exist
    if !config_dir_path.exists() {
        println!(
            "📂 Creating configuration directory: {}",
            config_dir_path.display()
        );
        if let Err(e) = std::fs::create_dir_all(&config_dir_path) {
            display_error("Failed to create config directory", &e.to_string());
            println!("   Path: {}", config_dir_path.display());
            return Err(PmError::DirectoryCreationFailed.into());
        }
    }

    // Step 4: Create and save configuration
    let config = Config {
        version: crate::constants::CONFIG_VERSION.to_string(),
        config_path: config_dir_path.clone(),
        settings: ConfigSettings {
            show_git_status,
            recent_projects_limit: 10, // default
        },
        projects: std::collections::HashMap::new(),
        machine_metadata: std::collections::HashMap::new(),
    };

    save_config(&config).await?;
    display_init_success(&config_dir_path, &config_path);
    
    // Show configuration file path info
    println!("📄 Configuration file: {}", config_path.display());
    
    // Step 5: Shell integration setup with backup support
    println!();
    let shell_backup = setup_shell_integration_with_backup(skip, replace).await?;
    
    // Step 6: Save backup metadata if we created any backups
    if let Some(mut backup) = backup_entry {
        if let Some(shell_backup) = shell_backup {
            backup.files.extend(shell_backup.files);
            backup.shell_changes.extend(shell_backup.shell_changes);
        }
        add_backup_entry(backup).await?;
        println!("💾 Backup created successfully");
    }
    
    // Step 7: Complete setup
    let binary_name = utils::get_binary_name();
    println!("\n📖 Use '{} --help' to see all available commands", binary_name);

    Ok(())
}

/// Handle configuration file conflicts with interactive user choice
async fn handle_config_conflict_interactive(
    config_path: &Path,
) -> Result<ConflictAction> {
    println!("⚠️  Configuration already exists: {}", config_path.display());
    
    // Interactive prompt with skip/replace choices
    let choices = vec!["Skip (keep existing)", "Replace (backup and recreate)", "Cancel"];
    let choice = handle_inquire_error(
        Select::new("What would you like to do?", choices)
            .prompt()
    )?;
    
    match choice {
        "Skip (keep existing)" => Ok(ConflictAction::Skip),
        "Replace (backup and recreate)" => Ok(ConflictAction::Replace),
        "Cancel" => Ok(ConflictAction::Cancel),
        _ => unreachable!(),
    }
}

/// Setup shell integration with backup support
async fn setup_shell_integration_with_backup(
    skip: bool,
    replace: bool,
) -> Result<Option<crate::backup::BackupEntry>> {
    // Use the new backup-enabled shell integration setup
    match shell_integration::setup_shell_integration_with_backup(skip, replace).await {
        Ok(backup_entry) => Ok(backup_entry),
        Err(e) => {
            display_warning(&format!("Failed to setup shell integration: {}", e));
            println!("💡 You can manually setup shell integration later");
            Ok(None)
        }
    }
}


