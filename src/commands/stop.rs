//! `pm stop [service] [project]` — terminate spawned services.
//!
//! Sends SIGTERM to the recorded PID(s) and removes the corresponding
//! entries from `services.json` and `routes.json`. If the service is
//! already gone, treats the operation as a successful no-op.

use crate::commands::run::resolve_project;
use crate::routes;
use crate::services as svc_state;
#[cfg(not(unix))]
use anyhow::anyhow;
use anyhow::Result;
use colored::Colorize;
use std::time::Duration;

pub fn run(service: Option<String>, project: Option<String>) -> Result<()> {
    let (workspace, project_obj, _project_dir) = resolve_project(project)?;
    let project_name = project_obj.name.clone();

    let targets = match service {
        Some(s) => match svc_state::remove(&workspace, &project_name, &s)? {
            Some(state) => vec![(s, state)],
            None => {
                println!(
                    "{} no running service '{}' in {}/{}",
                    "—".dimmed(),
                    s,
                    workspace,
                    project_name
                );
                return Ok(());
            }
        },
        None => {
            let removed = svc_state::remove_project(&workspace, &project_name)?;
            if removed.is_empty() {
                println!(
                    "{} no running services in {}/{}",
                    "—".dimmed(),
                    workspace,
                    project_name
                );
                return Ok(());
            }
            removed.into_iter().collect()
        }
    };

    let count = targets.len();
    for (key, state) in targets {
        terminate(state.pid)?;
        // Remove the route entries (canonical + default-workspace alias).
        let _ = routes::unregister_service(&workspace, &project_name, &key);
        println!(
            "{} stopped {}/{}/{} (pid {})",
            "✓".green(),
            workspace,
            project_name,
            key,
            state.pid
        );
    }
    if count > 1 {
        println!("({count} services stopped)");
    }

    Ok(())
}

#[cfg(unix)]
fn terminate(pid: u32) -> Result<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let nix_pid = Pid::from_raw(pid as i32);
    if !svc_state::pid_alive(pid) {
        // Already gone.
        return Ok(());
    }

    // SIGTERM first; give it 2s to land before escalating to SIGKILL.
    let _ = kill(nix_pid, Signal::SIGTERM);
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(2000) {
        if !svc_state::pid_alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Last resort.
    let _ = kill(nix_pid, Signal::SIGKILL);
    Ok(())
}

#[cfg(not(unix))]
fn terminate(_pid: u32) -> Result<()> {
    Err(anyhow!("pm stop signaling is Unix-only in v0.4.0"))
}
