//! Size-based log file rotation for orchestrator service logs.
//!
//! v0.4.0 scope: rotation happens at service-spawn time. The orchestrator
//! checks whether the existing `.log` file exceeds `MAX_BYTES` and, if so,
//! shifts:
//!
//! ```text
//! file.log.3 → (deleted)
//! file.log.2 → file.log.3
//! file.log.1 → file.log.2
//! file.log   → file.log.1
//! file.log   ← (new, empty)
//! ```
//!
//! Long-running services that exceed the threshold mid-run keep writing to
//! the same `.log` until the next `pm run` re-spawns them. This is a
//! deliberate simplification — implementing online rotation would require
//! interposing on the child's stdio (e.g. via a pipe pump goroutine).

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// 10 MiB. Matches the design.md / tasks.md target.
pub const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Number of historical files to retain (`.log.1` … `.log.<KEEP>`).
pub const KEEP: usize = 3;

/// Inspect `path` and rotate it in place if its size exceeds `max_bytes`.
/// No-op when the file does not exist or is below the threshold.
pub fn rotate_if_needed(path: &Path, max_bytes: u64, keep: usize) -> Result<bool> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if meta.len() <= max_bytes {
        return Ok(false);
    }
    rotate(path, keep)?;
    Ok(true)
}

/// Force a rotation regardless of size. Used by tests and explicit
/// administrative tooling (none in v0.4.0).
pub fn rotate(path: &Path, keep: usize) -> Result<()> {
    // Remove the oldest survivor.
    let oldest = numbered(path, keep);
    if oldest.exists() {
        fs::remove_file(&oldest)?;
    }
    // Shift higher numbers down.
    for n in (1..keep).rev() {
        let from = numbered(path, n);
        let to = numbered(path, n + 1);
        if from.exists() {
            fs::rename(&from, &to)?;
        }
    }
    // Move the live file to .1.
    let one = numbered(path, 1);
    fs::rename(path, &one)?;
    Ok(())
}

fn numbered(path: &Path, n: usize) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_bytes(path: &Path, n: usize) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&vec![b'.'; n]).unwrap();
    }

    #[test]
    fn no_rotation_below_threshold() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.log");
        write_bytes(&p, 100);
        assert!(!rotate_if_needed(&p, 1024, 3).unwrap());
        assert!(p.exists());
    }

    #[test]
    fn rotation_above_threshold() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.log");
        write_bytes(&p, 2048);
        assert!(rotate_if_needed(&p, 1024, 3).unwrap());
        assert!(!p.exists());
        assert!(numbered(&p, 1).exists());
    }

    #[test]
    fn rotation_chain_keeps_only_n_files() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("a.log");

        // First rotation -> .log → .log.1
        write_bytes(&p, 100);
        rotate(&p, 3).unwrap();
        // Second
        write_bytes(&p, 100);
        rotate(&p, 3).unwrap();
        // Third
        write_bytes(&p, 100);
        rotate(&p, 3).unwrap();
        // Fourth — should drop the oldest (would-be .log.4 doesn't exist;
        // .log.3 gets overwritten and .log.1..3 remain).
        write_bytes(&p, 100);
        rotate(&p, 3).unwrap();

        assert!(!p.exists());
        assert!(numbered(&p, 1).exists());
        assert!(numbered(&p, 2).exists());
        assert!(numbered(&p, 3).exists());
        assert!(!numbered(&p, 4).exists());
    }

    #[test]
    fn missing_file_is_noop() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("ghost.log");
        assert!(!rotate_if_needed(&p, 1024, 3).unwrap());
    }
}
