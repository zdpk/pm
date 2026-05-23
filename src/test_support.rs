//! Test harness for command-level integration tests.
//!
//! `pm` reads configuration from a directory pointed at by `PM_CONFIG_DIR`
//! and resolves project paths under `Config.base_root`. Both are process
//! globals, so command tests must (1) isolate their state in a tempdir and
//! (2) serialize so they don't trample each other.
//!
//! [`TestEnv`] sets up a fresh tempdir with a valid v4 manifest, points
//! `PM_CONFIG_DIR` at it, and holds a process-wide mutex until dropped. Drop
//! cleans up the env var and the tempdir.

#![cfg(test)]

use crate::models::{Config, HistoryData, Manifest};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::TempDir;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub struct TestEnv {
    _tempdir: TempDir,
    #[allow(dead_code)]
    pub config_dir: PathBuf,
    pub base_root: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnv {
    /// Build a fresh isolated PM config with a default v4 manifest.
    pub fn new() -> Self {
        let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
        // Recover from a poisoned lock — a previous panicking test still
        // dropped its TestEnv so the env is in a known state.
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());

        let tempdir = tempfile::tempdir().expect("create tempdir");
        let config_dir = tempdir.path().join("config");
        let base_root = tempdir.path().join("base");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::create_dir_all(&base_root).expect("create base_root");

        // SAFETY: setting env vars is unsafe because it is not thread-safe.
        // The ENV_LOCK above serializes TestEnv usage, so only one test at a
        // time touches PM_CONFIG_DIR.
        unsafe {
            std::env::set_var("PM_CONFIG_DIR", &config_dir);
        }

        let config = Config {
            base_root: base_root.to_string_lossy().into_owned(),
            ..Config::default()
        };
        std::fs::write(
            config_dir.join("config.json"),
            serde_json::to_string_pretty(&config).expect("serialize config"),
        )
        .expect("write config.json");
        std::fs::write(
            config_dir.join("manifest.json"),
            serde_json::to_string_pretty(&Manifest::default()).expect("serialize manifest"),
        )
        .expect("write manifest.json");
        std::fs::write(
            config_dir.join("history.json"),
            serde_json::to_string_pretty(&HistoryData::default()).expect("serialize history"),
        )
        .expect("write history.json");
        // Materialize the default workspace directory by convention.
        std::fs::create_dir_all(base_root.join("default")).expect("create default workspace dir");

        Self {
            _tempdir: tempdir,
            config_dir,
            base_root,
            _guard: guard,
        }
    }

    pub fn tempdir_path(&self) -> &std::path::Path {
        self._tempdir.path()
    }

    pub fn workspace_dir(&self, ws: &str) -> PathBuf {
        self.base_root.join(ws)
    }

    pub fn project_dir(&self, ws: &str, project: &str) -> PathBuf {
        self.workspace_dir(ws).join(project)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // SAFETY: see TestEnv::new — ENV_LOCK serializes access.
        unsafe {
            std::env::remove_var("PM_CONFIG_DIR");
        }
    }
}

/// Initialize a non-bare git repo at `path` with one commit, so it can be
/// referenced as a project source (move-into-place) or as a clone source.
pub fn init_git_repo(path: &std::path::Path) {
    std::fs::create_dir_all(path).expect("create repo path");
    run_git(path, &["init", "-q"]);
    // git requires identity for `commit`.
    run_git(path, &["config", "user.email", "test@example.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    std::fs::write(path.join("README.md"), "test\n").expect("write README");
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "init"]);
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("spawn git");
    assert!(
        status.success(),
        "git {:?} failed in {}",
        args,
        cwd.display()
    );
}
