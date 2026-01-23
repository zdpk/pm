mod cli;
mod commands;
mod config;
mod error;
mod git;
mod models;
mod path;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => commands::init::run(force),
        Commands::Add {
            path,
            name,
            tags,
            note,
            force,
        } => commands::add::run(path, name, tags, note, force),
        Commands::List {
            all,
            tags,
            path,
            no_status,
            sort,
            reverse,
            filter,
        } => commands::list::run(all, tags, path, no_status, sort, reverse, filter),
        Commands::Switch { target } => commands::switch::run(target),
        Commands::Use { workspace } => commands::use_ws::run(workspace),
        Commands::Path { target } => commands::path::run(target),
        Commands::Remove {
            project,
            force,
            recursive,
        } => commands::remove::run(project, force, recursive),
        Commands::Workspace(ws_cmd) => commands::workspace::run(ws_cmd),
        Commands::Completion { shell } => commands::completion::run(shell),
        Commands::Check => commands::check::run(),
    }
}
