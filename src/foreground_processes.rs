//! Registry for foreground commands started through `pm run -- <command>`.
//!
//! Unlike orchestrated services, these processes stay attached to the calling
//! terminal and do not receive a PM route. The short-lived registry lets the
//! read-only dashboard show them while they are running without inspecting
//! process environments or command arguments.

use crate::config::foreground_processes_path;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const LOCK_RETRIES: usize = 300;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(10);
const INCOMPLETE_LOCK_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForegroundProcessState {
    pub workspace: String,
    pub project: String,
    pub pid: u32,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Executable name only. Arguments are intentionally not persisted because
    /// command lines may contain credentials or other sensitive values.
    pub program: String,
    /// OS-reported process start value used to reject a recycled PID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_token: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForegroundProcessesData {
    #[serde(default)]
    pub processes: HashMap<String, ForegroundProcessState>,
}

pub fn load() -> Result<ForegroundProcessesData> {
    let path = foreground_processes_path();
    if !path.exists() {
        return Ok(ForegroundProcessesData::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

fn save(data: &ForegroundProcessesData) -> Result<()> {
    let path = foreground_processes_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(data)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn upsert(state: ForegroundProcessState) -> Result<()> {
    let _lock = RegistryLock::acquire()?;
    let mut data = load()?;
    data.processes.retain(|_, process| is_running(process));
    data.processes.insert(state.pid.to_string(), state);
    save(&data)
}

pub fn remove(pid: u32) -> Result<()> {
    let _lock = RegistryLock::acquire()?;
    let mut data = load()?;
    if data.processes.remove(&pid.to_string()).is_some() {
        save(&data)?;
    }
    Ok(())
}

pub fn is_running(process: &ForegroundProcessState) -> bool {
    if !pid_alive(process.pid) {
        return false;
    }
    match &process.start_token {
        Some(expected) => process_start_token(process.pid).as_ref() == Some(expected),
        None => true,
    }
}

#[cfg(unix)]
pub fn process_start_token(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

#[cfg(not(unix))]
pub fn process_start_token(_pid: u32) -> Option<String> {
    None
}

#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(not(unix))]
pub fn pid_alive(_pid: u32) -> bool {
    false
}

struct RegistryLock {
    path: PathBuf,
    token: String,
}

impl RegistryLock {
    fn acquire() -> Result<Self> {
        let path = foreground_processes_path().with_extension("lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }

        for _ in 0..LOCK_RETRIES {
            let token = lock_token();
            match try_install_lock(&path, &token) {
                Ok(true) => {
                    return Ok(Self { path, token });
                }
                Ok(false) => {
                    let owner = fs::read_to_string(&path)
                        .ok()
                        .and_then(|owner| owner.split_whitespace().next()?.parse::<u32>().ok());
                    let incomplete_lock_is_old = owner.is_none()
                        && fs::metadata(&path)
                            .and_then(|metadata| metadata.modified())
                            .ok()
                            .and_then(|modified| modified.elapsed().ok())
                            .is_some_and(|age| age >= INCOMPLETE_LOCK_GRACE);
                    if owner.is_some_and(|pid| !pid_alive(pid)) || incomplete_lock_is_old {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    std::thread::sleep(LOCK_RETRY_DELAY);
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("creating registry lock {}", path.display()));
                }
            }
        }

        Err(anyhow!(
            "timed out waiting for foreground process registry lock {}",
            path.display()
        ))
    }
}

fn lock_token() -> String {
    format!(
        "{} {}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn try_install_lock(path: &std::path::Path, token: &str) -> std::io::Result<bool> {
    let unique = token.replace(' ', ".");
    let candidate = path.with_extension(format!("lock.{unique}.tmp"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&candidate)?;
    if let Err(error) = writeln!(file, "{token}") {
        let _ = fs::remove_file(&candidate);
        return Err(error);
    }
    drop(file);

    let installed = match fs::hard_link(&candidate, path) {
        Ok(()) => true,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => false,
        Err(error) => {
            let _ = fs::remove_file(&candidate);
            return Err(error);
        }
    };
    let _ = fs::remove_file(candidate);
    Ok(installed)
}

impl Drop for RegistryLock {
    fn drop(&mut self) {
        let still_ours = fs::read_to_string(&self.path)
            .ok()
            .is_some_and(|value| value.trim() == self.token);
        if still_ours {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestEnv;
    use chrono::{TimeZone, Utc};

    fn process(pid: u32) -> ForegroundProcessState {
        ForegroundProcessState {
            workspace: "personal".into(),
            project: "demo".into(),
            pid,
            started_at: Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap(),
            program: "pnpm".into(),
            start_token: None,
        }
    }

    #[test]
    fn registry_round_trip_and_remove() {
        let _env = TestEnv::new();
        let state = process(std::process::id());

        upsert(state.clone()).unwrap();
        let loaded = load().unwrap();
        assert_eq!(loaded.processes[&state.pid.to_string()].program, "pnpm");

        remove(state.pid).unwrap();
        assert!(load().unwrap().processes.is_empty());
    }

    #[test]
    fn upsert_prunes_dead_entries() {
        let _env = TestEnv::new();
        let mut data = ForegroundProcessesData::default();
        data.processes.insert("999999".into(), process(999_999));
        save(&data).unwrap();

        upsert(process(std::process::id())).unwrap();
        let loaded = load().unwrap();
        assert_eq!(loaded.processes.len(), 1);
        assert!(
            loaded
                .processes
                .contains_key(&std::process::id().to_string())
        );
    }

    #[test]
    fn process_identity_rejects_recycled_pid() {
        let mut state = process(std::process::id());
        state.start_token = process_start_token(state.pid);
        assert!(is_running(&state));

        state.start_token = Some("not the actual start time".into());
        assert!(!is_running(&state));
    }

    #[test]
    fn stale_registry_lock_is_reclaimed() {
        let _env = TestEnv::new();
        let lock_path = foreground_processes_path().with_extension("lock");
        fs::write(&lock_path, "999999\n").unwrap();

        upsert(process(std::process::id())).unwrap();

        assert!(!lock_path.exists());
    }

    #[test]
    fn guard_does_not_remove_a_replaced_lock() {
        let _env = TestEnv::new();
        let path = foreground_processes_path().with_extension("lock");
        fs::write(&path, "replacement").unwrap();
        let guard = RegistryLock {
            path: path.clone(),
            token: "original".into(),
        };

        drop(guard);

        assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
    }

    #[test]
    fn lock_install_is_atomic_and_never_replaces_owner() {
        let _env = TestEnv::new();
        let path = foreground_processes_path().with_extension("lock");

        assert!(try_install_lock(&path, "123 first").unwrap());
        assert!(!try_install_lock(&path, "456 second").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "123 first\n");
    }
}
