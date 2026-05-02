mod cli;
mod commands;
mod config;
mod error;
mod git;
mod history;
mod models;
mod path;
mod log_rotation;
mod plugin;
mod project;
mod restore;
mod routes;
mod services;
mod state;
mod templates;

use anyhow::Result;
use clap::{error::ErrorKind, Parser};
use cli::{Cli, Commands};

fn main() -> Result<()> {
    match Cli::try_parse() {
        Ok(cli) => dispatch(cli),
        Err(err) => {
            if err.kind() == ErrorKind::InvalidSubcommand {
                let args: Vec<String> = std::env::args().collect();
                if let Some(name) = args.get(1) {
                    if let Some(plugin) = plugin::find_plugin(name) {
                        let plugin_args: Vec<String> = args.into_iter().skip(2).collect();
                        return plugin::run_plugin(&plugin, &plugin_args);
                    }
                }
            }

            err.exit();
        }
    }
}

fn dispatch(cli: Cli) -> Result<()> {
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
            yes,
            force,
            recursive,
        } => commands::remove::run(project, yes, force, recursive),
        Commands::Workspace(ws_cmd) => commands::workspace::run(ws_cmd),
        Commands::Sync {
            workspace,
            yes,
            jobs,
        } => commands::sync::run(workspace, yes, jobs),
        Commands::Manifest(cmd) => commands::manifest::run(cmd),
        Commands::Repo(repo_cmd) => commands::repo::run(repo_cmd),
        Commands::Ports(ports_cmd) => commands::ports::run(ports_cmd),
        Commands::Run {
            positional,
            command,
        } => commands::run::run(positional, command),
        Commands::Completion { shell } => commands::completion::run(shell),
        Commands::History { limit } => commands::history::run(limit),
        Commands::Check => commands::check::run(),
        Commands::Plugin(command) => commands::plugin::run(command),
        Commands::Project(cmd) => commands::project::run(cmd),
        Commands::Db(cmd) => commands::db::run(cmd),
        Commands::Proxy(cmd) => commands::proxy::run(cmd),
        Commands::Daemon { foreground } => commands::proxy::run_daemon(foreground),
        #[cfg(unix)]
        Commands::Logs { service, project } => commands::logs::run(service, project),
        #[cfg(not(unix))]
        Commands::Logs { .. } => Err(anyhow::anyhow!(
            "pm logs is Unix-only in v0.4.0 (orchestrator mode)"
        )),
        #[cfg(unix)]
        Commands::Stop { service, project } => commands::stop::run(service, project),
        #[cfg(not(unix))]
        Commands::Stop { .. } => Err(anyhow::anyhow!(
            "pm stop is Unix-only in v0.4.0 (orchestrator mode)"
        )),
        Commands::Upgrade => commands::upgrade::run(),
    }
}
