use crate::cli::PluginCommand;
use crate::plugin::{discover_plugins, set_plugin_enabled};
use anyhow::Result;
use colored::Colorize;

pub fn run(command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::List => list(),
        PluginCommand::Enable { name } => set_enabled(&name, true),
        PluginCommand::Disable { name } => set_enabled(&name, false),
    }
}

fn list() -> Result<()> {
    let plugins = discover_plugins();

    if plugins.is_empty() {
        println!("No plugins installed.");
        return Ok(());
    }

    println!(
        "{:<16} {:<10} {:<10} {:<8} DESCRIPTION",
        "NAME", "VERSION", "LANGUAGE", "STATUS"
    );

    for plugin in plugins {
        let status = if plugin.enabled {
            "enabled".green().to_string()
        } else {
            "disabled".dimmed().to_string()
        };
        println!(
            "{:<16} {:<10} {:<10} {:<8} {}",
            plugin.name,
            plugin.version,
            plugin.language.label(),
            status,
            plugin.description
        );
    }

    Ok(())
}

fn set_enabled(name: &str, enabled: bool) -> Result<()> {
    let plugin = set_plugin_enabled(name, enabled)?;
    let status = if enabled { "enabled" } else { "disabled" };
    println!("{} {} '{}'", "✓".green(), status, plugin.name.cyan());
    Ok(())
}
