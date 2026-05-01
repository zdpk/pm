//! `pm run` orchestrator mode: spawn services, ensure infra, register routes.
//!
//! See `design.md` for the full chain. The high-level flow is:
//!
//! 1. Resolve services (all or one) from `.proj.yaml`.
//! 2. If `dev.auto_start_docker`, ensure `pm-local-db` and `pm-local-redis`
//!    are running.
//! 3. If Postgres is reachable on loopback, ensure the per-project database
//!    exists (auto-`CREATE DATABASE`). Emit a v0.3.0 → v0.4.0 migration
//!    notice if a legacy `<ws>_<proj>_local` is present.
//! 4. Ensure the daemon is running (auto-spawn if needed).
//! 5. For each target service: allocate a port via `pm ports assign` if
//!    missing, open the log file, spawn the dev_cmd as a detached
//!    subprocess, register the route, persist the running state.
//! 6. Print a friendly summary and exit. The CLI returns; spawned services
//!    keep running until `pm stop`.

use crate::commands::db;
use crate::commands::proxy::daemon as proxy_daemon;
use crate::commands::run::build_port_env;
use crate::config::{load_config, logs_dir};
use crate::models::{PortKind, PortProject, PortService, Project};
use crate::path::collapse_path;
use crate::project::{ProjConfig, ResolvedService, ServiceDef, resolve_service_defaults};
use crate::routes;
use crate::services as svc_state;
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Entry point invoked by `pm run` once orchestrator mode has been selected.
pub fn start(
    workspace: &str,
    project: &Project,
    project_dir: &Path,
    proj_config: &ProjConfig,
    target_service: Option<&str>,
) -> Result<()> {
    let config = load_config()?;
    let services_to_start = pick_services(proj_config, target_service)?;
    if services_to_start.is_empty() {
        println!("{} no services to start", "—".dimmed());
        return Ok(());
    }

    // 1. Ensure Docker-managed Postgres + Redis (when configured).
    if config.dev.auto_start_docker && db::docker_available() {
        let pg = db::ensure_postgres(&config.dev.postgres_image)?;
        announce_container_state("pm-local-db", pg);
        let rd = db::ensure_redis(&config.dev.redis_image)?;
        announce_container_state("pm-local-redis", rd);
    } else if !config.dev.auto_start_docker {
        eprintln!(
            "  {} dev.auto_start_docker=false; assuming external Postgres/Redis are running",
            "i".cyan()
        );
    }

    // 2. Ensure per-project Postgres database exists (loopback only).
    if let Err(e) = ensure_project_database(workspace, &project.name) {
        eprintln!(
            "  {} could not ensure database: {} (services will still start; \
             they may fail to connect)",
            "!".yellow(),
            e
        );
    }

    // 3. Ensure the daemon is running.
    let daemon_pid = proxy_daemon::ensure_running()?;
    eprintln!(
        "  {} daemon ready (pid {})",
        "✓".green(),
        daemon_pid
    );

    // 4. For each service: allocate port (if missing), spawn, register route.
    for (name, resolved) in &services_to_start {
        spawn_service(workspace, project, project_dir, name, resolved)?;
    }

    print_summary(workspace, &project.name, &services_to_start);
    Ok(())
}

// ── Service selection ──

fn pick_services(
    config: &ProjConfig,
    target: Option<&str>,
) -> Result<Vec<(String, ResolvedService)>> {
    let mut out = Vec::new();
    let mut keys: Vec<&String> = config.services.keys().collect();
    keys.sort();

    for key in keys {
        if let Some(t) = target
            && t != key
        {
            continue;
        }
        let def: &ServiceDef = &config.services[key];
        let resolved = resolve_service_defaults(def, config.framework.as_deref())
            .with_context(|| format!("resolving service '{key}'"))?;
        out.push((key.clone(), resolved));
    }

    if let Some(t) = target
        && out.is_empty()
    {
        return Err(anyhow::anyhow!(
            "service '{t}' is not defined in .proj.yaml"
        ));
    }
    Ok(out)
}

// ── Per-service spawn ──

fn spawn_service(
    workspace: &str,
    project: &Project,
    project_dir: &Path,
    service_key: &str,
    resolved: &ResolvedService,
) -> Result<()> {
    // If a previous spawn is still alive, treat as a no-op so that
    // `pm run` is idempotent.
    if let Some(state) = svc_state::get(workspace, &project.name, service_key)?
        && svc_state::pid_alive(state.pid)
    {
        eprintln!(
            "  {} {}/{} already running (pid {}, port {})",
            "—".dimmed(),
            project.name,
            service_key,
            state.pid,
            state.port
        );
        return Ok(());
    }

    let port = ensure_port(workspace, project, service_key, resolved)?;
    let cwd: PathBuf = PathBuf::from(collapse_path(&project_dir.join(&resolved.dir)));
    fs::create_dir_all(&cwd).ok();

    let log_path = open_service_log(workspace, &project.name, service_key)?;
    let log_for_stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_for_stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut env = build_port_env(workspace, project)?;
    // Service-specific port — overlays the kind's default APP_PORT/etc.
    env.insert(env_var_for_kind(resolved.port_kind).into(), port.to_string());

    let (program, args) = split_dev_cmd(&resolved.dev_cmd);
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&cwd)
        .envs(&env)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_for_stdout))
        .stderr(Stdio::from(log_for_stderr));

    unsafe {
        cmd.pre_exec(|| {
            // Detach from the CLI's process group so Ctrl+C in the parent
            // shell does not propagate to long-running services.
            nix::unistd::setsid()
                .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
            Ok(())
        });
    }

    let child = cmd.spawn().with_context(|| {
        format!("spawning service '{service_key}' (cmd: {})", resolved.dev_cmd)
    })?;
    let pid = child.id();
    // Important: do not wait. Drop the Child handle so it doesn't reap on
    // CLI exit (it can't anyway thanks to setsid, but we are explicit).
    drop(child);

    // Persist state.
    svc_state::upsert(
        workspace,
        &project.name,
        service_key,
        svc_state::ServiceState {
            pid,
            port,
            started_at: Utc::now(),
            log_path: log_path.clone(),
            dev_cmd: resolved.dev_cmd.clone(),
        },
    )?;

    // Register route.
    routes::register_service(workspace, &project.name, service_key, port)?;

    eprintln!(
        "  {} spawned {}/{} (pid {}, port {})",
        "✓".green(),
        project.name,
        service_key,
        pid,
        port
    );
    Ok(())
}

fn split_dev_cmd(dev_cmd: &str) -> (String, Vec<String>) {
    let mut iter = dev_cmd.split_whitespace();
    let program = iter.next().unwrap_or("").to_string();
    let args: Vec<String> = iter.map(|s| s.to_string()).collect();
    (program, args)
}

fn env_var_for_kind(kind: PortKind) -> &'static str {
    kind.env_key()
}

// ── Port allocation ──

fn ensure_port(
    workspace: &str,
    project: &Project,
    service_key: &str,
    resolved: &ResolvedService,
) -> Result<u16> {
    use crate::config::{load_ports, save_ports};

    let project_key = format!("{workspace}/{}", project.name);
    let mut ports = load_ports()?;

    // Existing assignment?
    if let Some(p) = ports
        .projects
        .get(&project_key)
        .and_then(|p| p.services.get(service_key))
    {
        return Ok(p.port);
    }

    // Allocate a fresh port within the kind's range.
    let port = pick_random_port(&ports, resolved.port_kind)?;

    let entry = ports
        .projects
        .entry(project_key)
        .or_insert_with(|| PortProject {
            workspace: workspace.to_string(),
            project: project.name.clone(),
            path: String::new(),
            services: Default::default(),
        });
    entry.services.insert(
        service_key.to_string(),
        PortService {
            kind: resolved.port_kind,
            env: resolved.port_kind.env_key().to_string(),
            port,
            locked: false,
        },
    );
    save_ports(&ports)?;
    Ok(port)
}

fn pick_random_port(ports: &crate::models::PortsData, kind: PortKind) -> Result<u16> {
    use std::net::TcpListener;
    let range = ports
        .ranges
        .get(&kind)
        .ok_or_else(|| anyhow::anyhow!("no port range configured for {}", kind.as_str()))?;
    let used: HashSet<u16> = ports
        .projects
        .values()
        .flat_map(|p| p.services.values().map(|s| s.port))
        .collect();
    let span = u32::from(range.end) - u32::from(range.start) + 1;
    // Time-seeded offset (matches existing `pm ports assign` behaviour).
    let seed = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64;
    for offset in 0..span {
        let step = ((seed.wrapping_add(u64::from(offset).wrapping_mul(7919))) % u64::from(span)) as u16;
        let candidate = range.start + step;
        if used.contains(&candidate) {
            continue;
        }
        if TcpListener::bind(("127.0.0.1", candidate)).is_ok() {
            return Ok(candidate);
        }
    }
    Err(anyhow::anyhow!("no available port in {} range", kind.as_str()))
}

// ── Database helpers ──

fn ensure_project_database(workspace: &str, project: &str) -> Result<()> {
    let cs = db::admin_connection_string("127.0.0.1", db::POSTGRES_DEFAULT_PORT);
    let mut client = match postgres::Client::connect(&cs, postgres::NoTls) {
        Ok(c) => c,
        Err(e) => {
            // Postgres not yet ready; warn but don't fail the whole orchestrator.
            return Err(anyhow::anyhow!("connect Postgres: {e}"));
        }
    };

    let new_name = crate::commands::run::local_database_name(workspace, project);

    // v0.3.0 migration notice (one-time on first orchestrator run).
    let legacy = db::legacy_v03_databases(&mut client, workspace, project).unwrap_or_default();
    db::emit_v03_migration_notice(&legacy, &new_name);

    let created = db::ensure_database_on_loopback(&mut client, "127.0.0.1", &new_name)?;
    if created {
        eprintln!("  {} created database \"{}\"", "✓".green(), new_name);
    }
    Ok(())
}

// ── Logging ──

fn open_service_log(workspace: &str, project: &str, service: &str) -> Result<PathBuf> {
    let dir = logs_dir();
    fs::create_dir_all(&dir).ok();
    let name = format!("{workspace}_{project}_{service}.log");
    let path = dir.join(name);

    // Rotate before re-opening so the new spawn writes to a fresh `.log`
    // when the previous run accumulated more than MAX_BYTES.
    let _ = crate::log_rotation::rotate_if_needed(
        &path,
        crate::log_rotation::MAX_BYTES,
        crate::log_rotation::KEEP,
    );

    // Touch the file so subsequent OpenOptions::append succeeds.
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    Ok(path)
}

// ── Output ──

fn announce_container_state(name: &str, state: db::ContainerState) {
    use db::ContainerState::*;
    let msg = match state {
        Created => format!("created {name}"),
        Started => format!("started {name}"),
        AlreadyRunning => format!("{name} already running"),
        ExternalInUse => format!("skipped {name} (external port owner detected)"),
    };
    eprintln!("  {} {}", "✓".green(), msg);
}

fn print_summary(workspace: &str, project: &str, services: &[(String, ResolvedService)]) {
    println!();
    println!("{}", "Services running:".bold());
    for (name, _) in services {
        let host = if workspace == "default" {
            format!("{name}.{project}.localhost")
        } else {
            format!("{name}.{project}.{workspace}.localhost")
        };
        let proxy_port = load_config()
            .map(|c| c.dev.proxy_port)
            .unwrap_or(7100);
        println!(
            "  {} http://{}:{}/",
            "→".cyan(),
            host,
            proxy_port
        );
    }
    println!();
    println!(
        "Tail logs:  {}\nStop:       {}",
        "pm logs <service>".dimmed(),
        "pm stop".dimmed()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_dev_cmd_simple() {
        let (p, a) = split_dev_cmd("cargo run");
        assert_eq!(p, "cargo");
        assert_eq!(a, vec!["run".to_string()]);
    }

    #[test]
    fn split_dev_cmd_multi_arg() {
        let (p, a) = split_dev_cmd("pnpm next dev --turbopack");
        assert_eq!(p, "pnpm");
        assert_eq!(a, vec!["next", "dev", "--turbopack"]);
    }

    #[test]
    fn split_dev_cmd_single_program() {
        let (p, a) = split_dev_cmd("flutter");
        assert_eq!(p, "flutter");
        assert!(a.is_empty());
    }
}
