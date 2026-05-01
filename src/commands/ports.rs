use crate::cli::PortsCommand;
use crate::config::{load_ports, save_ports};
use crate::error::PmError;
use crate::models::{PortKind, PortProject, PortService, PortsData, Project};
use crate::state::{detect_current_project, load_state, parse_target, project_path_display};
use anyhow::{anyhow, Result};
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::net::TcpListener;

pub fn run(cmd: PortsCommand) -> Result<()> {
    match cmd {
        PortsCommand::List => list(),
        PortsCommand::Assign {
            project,
            kind,
            force,
        } => assign(project, kind, force),
        PortsCommand::Check { project, all } => check(project, all),
        PortsCommand::Repair { project } => repair(project),
        PortsCommand::Release { project, kind } => release(project, kind),
        PortsCommand::Lock { project, service } => lock(project, service, true),
        PortsCommand::Unlock { project, service } => lock(project, service, false),
        PortsCommand::Shared { postgres, redis } => shared(postgres, redis),
    }
}

fn list() -> Result<()> {
    let ports = load_ports()?;
    print_shared_section(&ports);

    let rows = collect_rows(&ports, None);
    if rows.is_empty() {
        println!();
        println!("{}", "(no ports allocated)".dimmed());
        return Ok(());
    }

    println!();
    print_rows(&rows);
    Ok(())
}

fn print_shared_section(ports: &PortsData) {
    println!("{}", "SHARED".bold());
    println!(
        "  {:<10} {:<6} {}",
        "SERVICE".bold(),
        "PORT".bold(),
        "STATUS".bold()
    );
    print_shared_row("postgres", ports.shared.postgres_port);
    print_shared_row("redis", ports.shared.redis_port);
}

fn assign(project_name: Option<String>, kinds: Vec<PortKind>, force: bool) -> Result<()> {
    let (workspace, project, path) = resolve_project(project_name)?;
    let project_key = project_key(&workspace, &project.name);
    let kinds = normalize_kinds(kinds);

    if let Some(shared_kind) = kinds.iter().find(|k| k.is_shared()) {
        return Err(anyhow!(
            "{} is a shared kind and cannot be assigned per project. Use `pm ports shared` to view or update the shared local Postgres/Redis ports.",
            shared_kind.as_str()
        ));
    }

    let mut ports = load_ports()?;

    ensure_project_entry(&mut ports, &project_key, &workspace, &project, &path);

    for kind in kinds {
        let service_key = kind.service_key().to_string();
        let existing = ports
            .projects
            .get(&project_key)
            .and_then(|p| p.services.get(&service_key))
            .cloned();

        if let Some(service) = existing {
            if !force {
                println!(
                    "{} {}.{} keeps {}={}",
                    "✓".green(),
                    project.name.cyan(),
                    service_key.cyan(),
                    service.env,
                    service.port
                );
                continue;
            }

            if service.locked {
                println!(
                    "{} {}.{} is locked; skipping",
                    "!".yellow(),
                    project.name.cyan(),
                    service_key.cyan()
                );
                continue;
            }
        }

        let port = choose_port(&ports, kind, Some((&project_key, &service_key)))?;
        let service = PortService {
            kind,
            env: kind.env_key().to_string(),
            port,
            locked: false,
        };

        let entry = ports
            .projects
            .get_mut(&project_key)
            .ok_or_else(|| anyhow!("Port project entry disappeared"))?;
        entry.services.insert(service_key.clone(), service);

        println!(
            "{} Assigned {}.{} {}={}",
            "✓".green(),
            project.name.cyan(),
            service_key.cyan(),
            kind.env_key(),
            port
        );
    }

    save_ports(&ports)?;
    Ok(())
}

fn check(project_name: Option<String>, all: bool) -> Result<()> {
    let ports = load_ports()?;
    let filter = if all {
        None
    } else {
        let (workspace, project, _) = resolve_project(project_name)?;
        Some(project_key(&workspace, &project.name))
    };

    print_shared_section(&ports);
    let shared_bound = !is_port_available(ports.shared.postgres_port)
        || !is_port_available(ports.shared.redis_port);

    let rows = collect_rows(&ports, filter.as_deref());
    if rows.is_empty() {
        println!();
        println!("{}", "(no per-project ports allocated)".dimmed());
    } else {
        println!();
        print_rows(&rows);
    }

    let has_per_project_issue = rows
        .iter()
        .any(|row| row.status == "duplicate" || row.status == "bound");

    println!();
    if has_per_project_issue {
        println!(
            "{} {}",
            "!".yellow(),
            "Review duplicate rows. Bound means localhost already has a listener on that port."
        );
    } else if shared_bound {
        println!(
            "{} Shared infra port is bound (expected if your local Postgres/Redis is running).",
            "i".cyan()
        );
    } else {
        println!("{} No port conflicts found", "✓".green());
    }

    Ok(())
}

fn repair(project_name: Option<String>) -> Result<()> {
    let (workspace, project, _) = resolve_project(project_name)?;
    let project_key = project_key(&workspace, &project.name);
    let mut ports = load_ports()?;
    let duplicates = duplicate_ports(&ports);

    let service_keys: Vec<String> = ports
        .projects
        .get(&project_key)
        .map(|p| p.services.keys().cloned().collect())
        .unwrap_or_default();

    if service_keys.is_empty() {
        println!("{}", "(no ports allocated)".dimmed());
        return Ok(());
    }

    let mut repaired = 0;
    for service_key in service_keys {
        let Some(service) = ports
            .projects
            .get(&project_key)
            .and_then(|p| p.services.get(&service_key))
            .cloned()
        else {
            continue;
        };

        if service.kind.is_shared() {
            continue;
        }

        if duplicates.get(&service.port).copied().unwrap_or(0) <= 1 {
            continue;
        }

        if service.locked {
            println!(
                "{} {}.{} is locked; duplicate port {} was not changed",
                "!".yellow(),
                project.name.cyan(),
                service_key.cyan(),
                service.port
            );
            continue;
        }

        let new_port = choose_port(&ports, service.kind, Some((&project_key, &service_key)))?;
        if let Some(entry) = ports.projects.get_mut(&project_key) {
            if let Some(target) = entry.services.get_mut(&service_key) {
                target.port = new_port;
                repaired += 1;
                println!(
                    "{} Repaired {}.{} {} -> {}",
                    "✓".green(),
                    project.name.cyan(),
                    service_key.cyan(),
                    service.port,
                    new_port
                );
            }
        }
    }

    if repaired == 0 {
        println!("{} No duplicate ports repaired", "✓".green());
    }

    save_ports(&ports)?;
    Ok(())
}

fn release(project_name: Option<String>, kinds: Vec<PortKind>) -> Result<()> {
    let (workspace, project, _) = resolve_project(project_name)?;
    let project_key = project_key(&workspace, &project.name);
    let mut ports = load_ports()?;

    if kinds.is_empty() {
        if ports.projects.remove(&project_key).is_some() {
            save_ports(&ports)?;
            println!(
                "{} Released all ports for '{}'",
                "✓".green(),
                project.name.cyan()
            );
        } else {
            println!("{}", "(no ports allocated)".dimmed());
        }
        return Ok(());
    }

    let mut released = 0;
    if let Some(entry) = ports.projects.get_mut(&project_key) {
        for kind in kinds {
            if entry.services.remove(kind.service_key()).is_some() {
                released += 1;
                println!(
                    "{} Released {}.{}",
                    "✓".green(),
                    project.name.cyan(),
                    kind.service_key().cyan()
                );
            }
        }

        if entry.services.is_empty() {
            ports.projects.remove(&project_key);
        }
    }

    if released == 0 {
        println!("{}", "(no matching ports allocated)".dimmed());
    }

    save_ports(&ports)?;
    Ok(())
}

fn lock(project_name: Option<String>, service_key: String, locked: bool) -> Result<()> {
    let (workspace, project, _) = resolve_project(project_name)?;
    let project_key = project_key(&workspace, &project.name);
    let mut ports = load_ports()?;

    let service = ports
        .projects
        .get_mut(&project_key)
        .and_then(|p| p.services.get_mut(&service_key))
        .ok_or_else(|| {
            anyhow!(
                "Port service '{}' not found for '{}'",
                service_key,
                project.name
            )
        })?;

    service.locked = locked;
    save_ports(&ports)?;

    let action = if locked { "Locked" } else { "Unlocked" };
    println!(
        "{} {} {}.{}",
        "✓".green(),
        action,
        project.name.cyan(),
        service_key.cyan()
    );

    Ok(())
}

fn shared(postgres: Option<u16>, redis: Option<u16>) -> Result<()> {
    if let Some(0) = postgres {
        return Err(anyhow!("Invalid postgres port: 0"));
    }
    if let Some(0) = redis {
        return Err(anyhow!("Invalid redis port: 0"));
    }

    let mut ports = load_ports()?;

    if postgres.is_none() && redis.is_none() {
        println!(
            "  {:<10} {:<6} {}",
            "SERVICE".bold(),
            "PORT".bold(),
            "STATUS".bold()
        );
        print_shared_row("postgres", ports.shared.postgres_port);
        print_shared_row("redis", ports.shared.redis_port);
        return Ok(());
    }

    if let Some(port) = postgres {
        ports.shared.postgres_port = port;
        if !is_port_available(port) {
            println!(
                "{} shared postgres port {} is currently bound on 127.0.0.1",
                "!".yellow(),
                port
            );
        }
        println!("{} shared.postgres = {}", "✓".green(), port);
    }

    if let Some(port) = redis {
        ports.shared.redis_port = port;
        if !is_port_available(port) {
            println!(
                "{} shared redis port {} is currently bound on 127.0.0.1",
                "!".yellow(),
                port
            );
        }
        println!("{} shared.redis = {}", "✓".green(), port);
    }

    save_ports(&ports)?;
    Ok(())
}

fn print_shared_row(name: &str, port: u16) {
    let status = if is_port_available(port) {
        "free"
    } else {
        "bound"
    };
    println!("  {:<10} {:<6} {}", name, port, status);
}

fn normalize_kinds(kinds: Vec<PortKind>) -> Vec<PortKind> {
    if kinds.is_empty() {
        vec![PortKind::Backend]
    } else {
        kinds
    }
}

fn resolve_project(project_name: Option<String>) -> Result<(String, Project, String)> {
    let (config, manifest) = load_state()?;

    let project = match project_name {
        Some(target) => {
            let (workspace_name, project_name) = parse_target(target);
            if let Some(workspace_name) = &workspace_name {
                if !manifest
                    .workspaces
                    .iter()
                    .any(|workspace| &workspace.name == workspace_name)
                {
                    return Err(PmError::WorkspaceNotFound(workspace_name.clone()).into());
                }
            }

            manifest
                .projects
                .iter()
                .find(|project| {
                    project.name == project_name
                        && match &workspace_name {
                            Some(workspace) => project.workspace == *workspace,
                            None => true,
                        }
                })
                .cloned()
                .ok_or_else(|| PmError::ProjectNotFound(project_name.clone()))?
        }
        None => detect_current_project(&config, &manifest)
            .map(|(project, _)| project.clone())
            .ok_or_else(|| {
                anyhow!(
                    "No project specified and current directory is not inside a registered project"
                )
            })?,
    };

    let path = project_path_display(&config, &manifest, &project)?;
    Ok((project.workspace.clone(), project, path))
}

fn ensure_project_entry(
    ports: &mut PortsData,
    project_key: &str,
    workspace: &str,
    project: &Project,
    path: &str,
) {
    let entry = ports
        .projects
        .entry(project_key.to_string())
        .or_insert_with(|| PortProject {
            workspace: workspace.to_string(),
            project: project.name.clone(),
            path: path.to_string(),
            services: HashMap::new(),
        });

    entry.path = path.to_string();
}

fn project_key(workspace: &str, project: &str) -> String {
    format!("{workspace}/{project}")
}

fn choose_port(ports: &PortsData, kind: PortKind, skip: Option<(&str, &str)>) -> Result<u16> {
    let range = ports
        .ranges
        .get(&kind)
        .ok_or_else(|| anyhow!("No port range configured for {}", kind.as_str()))?;

    if range.start > range.end {
        return Err(anyhow!("Invalid port range for {}", kind.as_str()));
    }

    let used = allocated_ports(ports, skip);
    let span = u32::from(range.end) - u32::from(range.start) + 1;
    let seed = port_seed(kind, skip) % u64::from(span);

    for offset in 0..span {
        let step = (seed + u64::from(offset) * 7919) % u64::from(span);
        let candidate = range.start + step as u16;
        if !used.contains(&candidate) && is_port_available(candidate) {
            return Ok(candidate);
        }
    }

    Err(anyhow!("No available port in {} range", kind.as_str()))
}

fn allocated_ports(ports: &PortsData, skip: Option<(&str, &str)>) -> HashSet<u16> {
    let mut used = HashSet::new();

    for (project_key, project) in &ports.projects {
        for (service_key, service) in &project.services {
            if skip.is_some_and(|(skip_project, skip_service)| {
                skip_project == project_key && skip_service == service_key
            }) {
                continue;
            }
            used.insert(service.port);
        }
    }

    used
}

fn port_seed(kind: PortKind, skip: Option<(&str, &str)>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hasher);
    if let Some((project_key, service_key)) = skip {
        project_key.hash(&mut hasher);
        service_key.hash(&mut hasher);
    }
    chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

#[derive(Debug)]
struct PortRow {
    workspace: String,
    project: String,
    service: String,
    kind: String,
    env: String,
    port: u16,
    locked: bool,
    status: &'static str,
}

fn collect_rows(ports: &PortsData, filter: Option<&str>) -> Vec<PortRow> {
    let duplicates = duplicate_ports(ports);
    let mut rows = Vec::new();

    for (project_key, project) in &ports.projects {
        if filter.is_some_and(|target| target != project_key) {
            continue;
        }

        for (service_key, service) in &project.services {
            let duplicate = duplicates.get(&service.port).copied().unwrap_or(0) > 1;
            let status = if duplicate {
                "duplicate"
            } else if is_port_available(service.port) {
                "free"
            } else {
                "bound"
            };

            rows.push(PortRow {
                workspace: project.workspace.clone(),
                project: project.project.clone(),
                service: service_key.clone(),
                kind: service.kind.as_str().to_string(),
                env: service.env.clone(),
                port: service.port,
                locked: service.locked,
                status,
            });
        }
    }

    rows.sort_by(|a, b| {
        (
            a.workspace.as_str(),
            a.project.as_str(),
            a.service.as_str(),
            a.port,
        )
            .cmp(&(
                b.workspace.as_str(),
                b.project.as_str(),
                b.service.as_str(),
                b.port,
            ))
    });

    rows
}

fn duplicate_ports(ports: &PortsData) -> HashMap<u16, usize> {
    let mut counts = HashMap::new();

    for project in ports.projects.values() {
        for service in project.services.values() {
            *counts.entry(service.port).or_insert(0) += 1;
        }
    }

    counts
}

fn print_rows(rows: &[PortRow]) {
    println!(
        "  {:<12} {:<18} {:<8} {:<10} {:<22} {:<6} {:<8} STATUS",
        "WORKSPACE", "PROJECT", "SERVICE", "KIND", "ENV", "PORT", "LOCKED"
    );

    for row in rows {
        let locked = if row.locked { "yes" } else { "no" };
        println!(
            "  {:<12} {:<18} {:<8} {:<10} {:<22} {:<6} {:<8} {}",
            row.workspace,
            row.project,
            row.service,
            row.kind,
            row.env,
            row.port,
            locked,
            row.status
        );
    }
}
