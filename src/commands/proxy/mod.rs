//! Local-dev-orchestrator daemon.
//!
//! See [`design.md`] (D1–D6) for the architectural rationale. In short:
//!
//! - **Single binary, two modes**: `pm __daemon` runs the daemon body; the
//!   regular CLI commands (`pm run`, `pm proxy ...`) auto-spawn it.
//! - **Reverse proxy on `127.0.0.1:7100`** routes by `Host` header to the
//!   per-service upstream port, using `routes.json` as the registry.
//! - **Control plane on `127.0.0.1:7101`** offers `/health`, `/status`,
//!   `/reload`, `/stop` for explicit management.
//!
//! [`design.md`]: ../../../openspec/changes/local-dev-orchestrator/design.md

#[cfg(unix)]
pub mod control;
#[cfg(unix)]
pub mod daemon;
#[cfg(unix)]
mod reverse;

use crate::cli::ProxyCommand;
use anyhow::Result;

#[cfg(unix)]
pub fn run(cmd: ProxyCommand) -> Result<()> {
    match cmd {
        ProxyCommand::Status => control::cmd_status(),
        ProxyCommand::Start { foreground } => {
            if foreground {
                daemon::run_foreground()
            } else {
                daemon::ensure_running().map(|_| ())
            }
        }
        ProxyCommand::Stop => control::cmd_stop(),
    }
}

#[cfg(unix)]
pub fn run_daemon(foreground: bool) -> Result<()> {
    if foreground {
        daemon::run_foreground()
    } else {
        // The detached spawn already happened before we reached this code path
        // (the parent CLI invoked `current_exe() __daemon` with stdio closed).
        // From the daemon's perspective, "not foreground" simply means it was
        // started by the auto-spawn path; behavior is identical to foreground
        // once we are running.
        daemon::run_foreground()
    }
}

// ── Windows / non-Unix stubs ──

#[cfg(not(unix))]
pub fn run(_cmd: ProxyCommand) -> Result<()> {
    Err(anyhow::anyhow!(
        "pm proxy / orchestrator daemon is Unix-only in v0.4.0 (macOS/Linux). \
         Windows support is planned for a future release."
    ))
}

#[cfg(not(unix))]
pub fn run_daemon(_foreground: bool) -> Result<()> {
    Err(anyhow::anyhow!(
        "The orchestrator daemon is Unix-only in v0.4.0."
    ))
}
