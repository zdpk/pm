//! Daemon entrypoint and detached-spawn helpers.
//!
//! The daemon is implemented as a `tokio::main` async runtime that owns:
//! - the reverse proxy on `dev.proxy_port` (default 7100), and
//! - the control-plane HTTP server on `dev.control_port` (default 7101).
//!
//! Both servers run as concurrent tasks; SIGTERM/SIGINT or a control-plane
//! `/stop` cleanly cancels them.

use crate::commands::proxy::control;
use crate::commands::proxy::reverse;
use crate::config::{config_dir, daemon_pid_path, load_config, logs_dir};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const READY_TIMEOUT_MS: u64 = 5000;

/// Ensure the daemon is running. Spawns one if not. Returns the PID.
///
/// Used by `pm run` (orchestrator mode) and `pm proxy start` (without
/// `--foreground`).
pub fn ensure_running() -> Result<u32> {
    if let Some(pid) = check_alive()? {
        return Ok(pid);
    }
    spawn_detached()?;
    wait_until_ready(Duration::from_millis(READY_TIMEOUT_MS))?;
    let pid = check_alive()?
        .context("daemon failed to start within the readiness window")?;
    Ok(pid)
}

/// Foreground entrypoint: blocks until the daemon exits.
pub fn run_foreground() -> Result<()> {
    write_pid_file(std::process::id())?;
    let result = run_inner();
    let _ = fs::remove_file(daemon_pid_path());
    result
}

fn run_inner() -> Result<()> {
    let config = load_config()?;
    let proxy_port = config.dev.proxy_port;
    let control_port = config.dev.control_port;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let shutdown = Arc::new(Notify::new());
        let proxy_task = reverse::serve(proxy_port, shutdown.clone());
        let control_task = control::serve(control_port, shutdown.clone());

        tokio::select! {
            r = proxy_task   => r?,
            r = control_task => r?,
            _ = wait_for_signal(shutdown.clone()) => {
                eprintln!("pm-daemon: shutdown signal received");
            }
        }

        Ok::<(), anyhow::Error>(())
    })
}

#[cfg(unix)]
async fn wait_for_signal(shutdown: Arc<Notify>) {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! {
        _ = sigterm.recv() => shutdown.notify_waiters(),
        _ = sigint.recv()  => shutdown.notify_waiters(),
        _ = shutdown.notified() => {}
    }
}

/// Returns Some(pid) if the daemon's pid file exists and the process is alive
/// AND the control plane responds. Otherwise None and (best effort) cleans
/// up the stale pid file.
pub fn check_alive() -> Result<Option<u32>> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&pid_path).unwrap_or_default();
    let pid: u32 = match content.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            let _ = fs::remove_file(&pid_path);
            return Ok(None);
        }
    };

    if !pid_alive(pid) {
        let _ = fs::remove_file(&pid_path);
        return Ok(None);
    }

    if !control::ping().unwrap_or(false) {
        // Process exists but daemon isn't responding — treat as dead.
        let _ = fs::remove_file(&pid_path);
        return Ok(None);
    }

    Ok(Some(pid))
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool {
    false
}

fn write_pid_file(pid: u32) -> Result<()> {
    let path = daemon_pid_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&path, format!("{pid}\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Spawn the daemon as a detached child process: same binary, `__daemon`
/// subcommand, no inherited stdio. The new process becomes its own session
/// leader (`setsid`), so a parent shell exit does not propagate SIGHUP.
#[cfg(unix)]
fn spawn_detached() -> Result<()> {
    let exe = std::env::current_exe().context("locating current exe")?;
    let log_path = open_daemon_log()?;
    let log_file_for_stdout = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_file_for_stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut cmd = Command::new(exe);
    cmd.arg("__daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file_for_stdout))
        .stderr(Stdio::from(log_file_for_stderr));
    unsafe {
        cmd.pre_exec(|| {
            // Detach from controlling tty so parent shell exit doesn't
            // SIGHUP us. nix's setsid is a thin wrapper around libc.
            nix::unistd::setsid()
                .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
            Ok(())
        });
    }
    cmd.spawn().context("spawning daemon")?;
    Ok(())
}

#[cfg(not(unix))]
fn spawn_detached() -> Result<()> {
    Err(anyhow::anyhow!(
        "daemon spawn requires Unix; Windows is not supported in v0.4.0"
    ))
}

fn open_daemon_log() -> Result<PathBuf> {
    let dir = logs_dir();
    fs::create_dir_all(&dir).ok();
    let path = dir.join("daemon.log");
    Ok(path)
}

fn wait_until_ready(timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    let mut delay_ms = 25;
    while start.elapsed() < timeout {
        if control::ping().unwrap_or(false) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(delay_ms));
        delay_ms = (delay_ms * 2).min(250);
    }
    Err(anyhow::anyhow!(
        "daemon did not become ready within {}ms",
        timeout.as_millis()
    ))
}

/// Convenience for tests / debugging.
#[allow(dead_code)]
pub fn config_dir_path() -> PathBuf {
    config_dir()
}

// ── Internal: tiny stdio bridge used by spawn_detached when the user runs
// `pm proxy start` without `--foreground`. We avoid leaking dangling FDs
// by calling Write::flush before fd inheritance. (Currently a no-op: the
// shell stdio is already null-redirected via Stdio::null/Stdio::from.) ──

#[allow(dead_code)]
fn _flush_self_stdio() {
    let _ = std::io::stdout().lock().flush();
    let _ = std::io::stderr().lock().flush();
}
