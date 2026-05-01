//! `pm logs <service> [project]` — tail the log file of a running service.
//!
//! Implementation strategy: `tail -f` over a regular file. We open the file
//! at its current end, then poll for size growth on a 100ms cadence. The
//! tail output streams to stdout. SIGINT exits cleanly.

use crate::commands::run::resolve_project;
use crate::services as svc_state;
use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

pub fn run(service: Option<String>, project: Option<String>) -> Result<()> {
    let (workspace, project_obj, _project_dir) = resolve_project(project)?;
    let services = svc_state::list_project(&workspace, &project_obj.name)?;

    if services.is_empty() {
        return Err(anyhow!(
            "no services running for {}/{}",
            workspace,
            project_obj.name
        ));
    }

    let target = match service {
        Some(s) => s,
        None => {
            if services.len() == 1 {
                services.keys().next().unwrap().clone()
            } else {
                let mut keys: Vec<&String> = services.keys().collect();
                keys.sort();
                return Err(anyhow!(
                    "multiple services running ({}); specify which to tail",
                    keys.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    };

    let state = services
        .get(&target)
        .ok_or_else(|| anyhow!("service '{}' is not running", target))?;

    eprintln!(
        "==> tailing {} (pid {}, port {})",
        state.log_path.display(),
        state.pid,
        state.port
    );
    tail_follow(&state.log_path)
}

fn tail_follow(path: &Path) -> Result<()> {
    let mut file = File::open(path).with_context(|| format!("opening log {}", path.display()))?;
    let mut pos = file.seek(SeekFrom::End(0))?;
    let mut buf = [0u8; 4096];
    let stdout = std::io::stdout();

    loop {
        let len = file.metadata()?.len();
        if len < pos {
            // File was rotated/truncated; reopen from the start.
            file = File::open(path)?;
            pos = 0;
        }

        if len > pos {
            file.seek(SeekFrom::Start(pos))?;
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                let mut handle = stdout.lock();
                handle.write_all(&buf[..n])?;
                handle.flush()?;
                pos += n as u64;
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
