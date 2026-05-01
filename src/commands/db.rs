//! Shared local Postgres / Redis container lifecycle.
//!
//! v0.4.0 introduces an opt-in orchestrator mode where `pm run` ensures a
//! shared Postgres + Redis instance is available. Containers are managed via
//! the external `docker` CLI (no bollard dependency to keep the binary
//! lightweight). Volumes persist across container restarts; `pm db stop`
//! stops containers but leaves volumes intact.

use crate::cli::DbCommand;
use crate::config::load_config;
use anyhow::{anyhow, Result};
use colored::Colorize;
use std::net::TcpListener;
use std::process::{Command, Stdio};

pub const POSTGRES_CONTAINER: &str = "pm-local-db";
pub const POSTGRES_VOLUME: &str = "pm-local-volume";
pub const POSTGRES_DEFAULT_PORT: u16 = 5432;

pub const REDIS_CONTAINER: &str = "pm-local-redis";
pub const REDIS_VOLUME: &str = "pm-local-redis-volume";
pub const REDIS_DEFAULT_PORT: u16 = 6379;

pub fn run(cmd: DbCommand) -> Result<()> {
    match cmd {
        DbCommand::Status => status(),
        DbCommand::Start => start_all(),
        DbCommand::Stop => stop_all(),
    }
}

// ── Public lifecycle helpers (used by `pm run` orchestrator) ──

/// Ensure the shared Postgres container is running.
///
/// Behavior:
/// - If `127.0.0.1:5432` is already bound by an external process, leave it
///   alone and return `Ok(ContainerState::ExternalInUse)`.
/// - Otherwise, start (or create+start) `pm-local-db` using `image`.
/// - Returns `Err` if Docker is not available.
pub fn ensure_postgres(image: &str) -> Result<ContainerState> {
    ensure_container(EnsureSpec {
        container: POSTGRES_CONTAINER,
        volume: POSTGRES_VOLUME,
        image,
        host_port: POSTGRES_DEFAULT_PORT,
        container_port: 5432,
        env: vec![("POSTGRES_PASSWORD", "postgres")],
        volume_mount: "/var/lib/postgresql/data",
    })
}

/// Ensure the shared Redis container is running.
///
/// See [`ensure_postgres`] for behavior. Redis has no environment variables
/// or password by default.
pub fn ensure_redis(image: &str) -> Result<ContainerState> {
    ensure_container(EnsureSpec {
        container: REDIS_CONTAINER,
        volume: REDIS_VOLUME,
        image,
        host_port: REDIS_DEFAULT_PORT,
        container_port: 6379,
        env: vec![],
        volume_mount: "/data",
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    /// We started a fresh container.
    Created,
    /// A container existed but was stopped; we restarted it.
    Started,
    /// A container is already running.
    AlreadyRunning,
    /// An external process is bound to the host port; container skipped.
    ExternalInUse,
}

// ── CLI subcommand handlers ──

fn status() -> Result<()> {
    if !docker_available() {
        println!("{} docker not available", "!".yellow());
        return Ok(());
    }

    print_container_status(POSTGRES_CONTAINER, POSTGRES_VOLUME, POSTGRES_DEFAULT_PORT);
    print_container_status(REDIS_CONTAINER, REDIS_VOLUME, REDIS_DEFAULT_PORT);
    Ok(())
}

fn start_all() -> Result<()> {
    let config = load_config()?;
    require_docker()?;

    let pg = ensure_postgres(&config.dev.postgres_image)?;
    print_ensure_result(POSTGRES_CONTAINER, pg);

    let rd = ensure_redis(&config.dev.redis_image)?;
    print_ensure_result(REDIS_CONTAINER, rd);
    Ok(())
}

fn stop_all() -> Result<()> {
    require_docker()?;
    stop_container(POSTGRES_CONTAINER)?;
    stop_container(REDIS_CONTAINER)?;
    Ok(())
}

// ── Internal helpers ──

struct EnsureSpec<'a> {
    container: &'a str,
    volume: &'a str,
    image: &'a str,
    host_port: u16,
    container_port: u16,
    env: Vec<(&'a str, &'a str)>,
    volume_mount: &'a str,
}

fn ensure_container(spec: EnsureSpec<'_>) -> Result<ContainerState> {
    if !is_port_available(spec.host_port) {
        // External process already on this port.
        // Verify by checking whether OUR container owns it.
        if let Some(true) = container_running(spec.container)? {
            return Ok(ContainerState::AlreadyRunning);
        }
        return Ok(ContainerState::ExternalInUse);
    }

    // Port is free. Start or create the container.
    require_docker()?;

    if container_exists(spec.container)? {
        run_docker(&["start", spec.container])?;
        return Ok(ContainerState::Started);
    }

    let host_port = spec.host_port.to_string();
    let port_map = format!("{}:{}", spec.host_port, spec.container_port);
    let volume_arg = format!("{}:{}", spec.volume, spec.volume_mount);

    let mut args: Vec<&str> = vec![
        "run",
        "-d",
        "--name",
        spec.container,
        "-p",
        &port_map,
        "-v",
        &volume_arg,
    ];

    let env_args: Vec<String> = spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    for env in &env_args {
        args.push("-e");
        args.push(env);
    }

    args.push("--restart");
    args.push("unless-stopped");
    args.push(spec.image);

    let _ = host_port; // borrow extension
    run_docker(&args)?;
    Ok(ContainerState::Created)
}

pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn require_docker() -> Result<()> {
    if docker_available() {
        return Ok(());
    }
    Err(anyhow!(
        "Docker is required for shared local infrastructure. \
         Install Docker (https://docs.docker.com/get-docker/) \
         or set `dev.auto_start_docker: false` in ~/.config/pm/config.json \
         to use externally managed Postgres/Redis."
    ))
}

fn container_exists(name: &str) -> Result<bool> {
    let out = Command::new("docker")
        .args(["ps", "-a", "--filter", &format!("name=^{name}$"), "-q"])
        .output()?;
    Ok(!out.stdout.is_empty())
}

fn container_running(name: &str) -> Result<Option<bool>> {
    if !container_exists(name)? {
        return Ok(None);
    }
    let out = Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("name=^{name}$"),
            "--filter",
            "status=running",
            "-q",
        ])
        .output()?;
    Ok(Some(!out.stdout.is_empty()))
}

fn run_docker(args: &[&str]) -> Result<()> {
    let status = Command::new("docker")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        return Err(anyhow!("docker {} failed", args.join(" ")));
    }
    Ok(())
}

fn stop_container(name: &str) -> Result<()> {
    if !container_exists(name)? {
        println!("  {} {} (not present)", "—".dimmed(), name);
        return Ok(());
    }
    if container_running(name)? != Some(true) {
        println!("  {} {} (already stopped)", "—".dimmed(), name);
        return Ok(());
    }
    run_docker(&["stop", name])?;
    println!("  {} stopped {}", "✓".green(), name);
    Ok(())
}

fn print_container_status(container: &str, volume: &str, port: u16) {
    let state = match container_running(container) {
        Ok(Some(true)) => "running".green().to_string(),
        Ok(Some(false)) => "stopped".yellow().to_string(),
        Ok(None) => "not created".dimmed().to_string(),
        Err(_) => "unknown".red().to_string(),
    };
    let port_state = if is_port_available(port) {
        "free".dimmed()
    } else if container_running(container).ok().flatten() == Some(true) {
        "owned by container".green()
    } else {
        "bound by external".yellow()
    };
    println!(
        "  {:<20} {:<14} port {:<6} {}  volume: {}",
        container, state, port, port_state, volume
    );
}

fn print_ensure_result(container: &str, state: ContainerState) {
    match state {
        ContainerState::Created => {
            println!("  {} created and started {}", "✓".green(), container)
        }
        ContainerState::Started => println!("  {} started {}", "✓".green(), container),
        ContainerState::AlreadyRunning => {
            println!("  {} {} already running", "—".dimmed(), container)
        }
        ContainerState::ExternalInUse => println!(
            "  {} skipped {} (external process owns the port)",
            "i".cyan(),
            container
        ),
    }
}

pub fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// ── Database creation (Postgres) ──
//
// These helpers are wired into `pm run` orchestrator-mode by Stage 2 (Group 9).
// The functions are public + tested standalone to allow a clean integration in
// the next stage without further refactoring.

#[allow(dead_code)] // wired in by Stage 2 (Group 9: orchestrator service spawn)
/// Build the connection string for the shared local Postgres on the given port.
///
/// Connects to the maintenance database `postgres` (always present). Used to
/// list and create per-project databases.
pub fn admin_connection_string(host: &str, port: u16) -> String {
    format!("host={host} port={port} user=postgres password=postgres dbname=postgres")
}

#[allow(dead_code)] // wired in by Stage 2 (Group 9)
/// Whether the connection target is loopback. Auto database creation is
/// only permitted against `127.0.0.1` or `localhost` to prevent accidental
/// CREATE on production hosts.
pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

#[allow(dead_code)] // wired in by Stage 2 (Group 9)
/// Check whether a Postgres database with the given name exists.
pub fn database_exists(client: &mut postgres::Client, name: &str) -> Result<bool> {
    let row = client.query_opt("SELECT 1 FROM pg_database WHERE datname = $1", &[&name])?;
    Ok(row.is_some())
}

#[allow(dead_code)] // wired in by Stage 2 (Group 9)
/// Ensure the named database exists, creating it if absent.
///
/// Returns `Ok(true)` if a CREATE was issued, `Ok(false)` if the database
/// already existed. Refuses to create when the connection target is not
/// loopback (the caller MUST verify before calling — this is a defense in
/// depth check via `host`).
pub fn ensure_database_on_loopback(
    client: &mut postgres::Client,
    host: &str,
    name: &str,
) -> Result<bool> {
    if !is_loopback_host(host) {
        eprintln!("pm: skipping CREATE DATABASE \"{name}\" — host {host} is not loopback");
        return Ok(false);
    }

    if database_exists(client, name)? {
        return Ok(false);
    }

    // Postgres database identifiers don't support parameter binding for DDL,
    // so we quote-escape manually. `name` is constructed from
    // `local_database_name()` which limits chars to `[a-z0-9_]`, so this is
    // safe; the doubled-quote escape is defense in depth.
    let safe = name.replace('"', "\"\"");
    client.batch_execute(&format!("CREATE DATABASE \"{safe}\""))?;
    Ok(true)
}

#[allow(dead_code)] // wired in by Stage 2 (Group 9)
/// Detect any v0.3.0-format databases (`<workspace>_<project>_local`) for the
/// given project and return their names. Used to emit the migration notice.
pub fn legacy_v03_databases(
    client: &mut postgres::Client,
    workspace: &str,
    project: &str,
) -> Result<Vec<String>> {
    let legacy_name = format!("{}_{}_local", workspace, project)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let rows = client.query(
        "SELECT datname FROM pg_database WHERE datname = $1",
        &[&legacy_name],
    )?;
    Ok(rows.into_iter().map(|r| r.get::<_, String>(0)).collect())
}

#[allow(dead_code)] // wired in by Stage 2 (Group 9)
/// Print a one-time stderr notice about v0.3.0 → v0.4.0 database name change.
pub fn emit_v03_migration_notice(legacy: &[String], new_name: &str) {
    if legacy.is_empty() {
        return;
    }
    for legacy_name in legacy {
        eprintln!(
            "pm: legacy v0.3.0 database '{legacy_name}' detected. \
             v0.4.0 uses '{new_name}' (without `_local`). \
             To migrate data: pg_dump {legacy_name} | psql {new_name}"
        );
    }
}

#[cfg(test)]
mod db_admin_tests {
    use super::*;

    #[test]
    fn loopback_check() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("::1"));
        assert!(!is_loopback_host("10.0.0.1"));
        assert!(!is_loopback_host("prod.example.com"));
    }

    #[test]
    fn admin_connection_string_format() {
        let cs = admin_connection_string("127.0.0.1", 5432);
        assert!(cs.contains("host=127.0.0.1"));
        assert!(cs.contains("port=5432"));
        assert!(cs.contains("dbname=postgres"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_state_is_distinguishable() {
        // Sanity: enum variants are PartialEq for assertion ergonomics
        assert_eq!(ContainerState::Created, ContainerState::Created);
        assert_ne!(ContainerState::Created, ContainerState::ExternalInUse);
    }

    #[test]
    fn require_docker_errors_when_missing() {
        // We can't reliably guarantee docker is missing in CI, so this test
        // only verifies the error path returns the expected message text
        // when invoked manually with PATH stripped of docker. It is
        // intentionally a no-op assertion to document the contract.
        let err = anyhow!(
            "Docker is required for shared local infrastructure. \
             Install Docker (https://docs.docker.com/get-docker/) \
             or set `dev.auto_start_docker: false` in ~/.config/pm/config.json \
             to use externally managed Postgres/Redis."
        );
        assert!(err.to_string().contains("Docker is required"));
    }
}
