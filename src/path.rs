use crate::error::PmError;
use std::path::{Path, PathBuf};

/// Expand ~ to home directory
pub fn expand_path(path: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(path).into_owned())
}

/// Collapse home directory to ~
pub fn collapse_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

/// Normalize a path: resolve relative paths and collapse to ~
pub fn normalize_path(path: &str) -> anyhow::Result<String> {
    let expanded = expand_path(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()?.join(&expanded)
    };
    let canonical = absolute.canonicalize()?;
    Ok(collapse_path(&canonical))
}

/// Validate that a string is safe to use as `Project.dir` under the strict path
/// convention: a single directory component, no traversal, no separators, no
/// absolute-path markers.
pub fn validate_project_dir(dir: &str) -> Result<(), PmError> {
    let err = |rule: &'static str| PmError::InvalidProjectDir {
        dir: dir.to_string(),
        rule,
    };

    if dir.is_empty() {
        return Err(err("must not be empty"));
    }
    if dir.contains('\0') {
        return Err(err("must not contain null bytes"));
    }
    if dir.contains('/') || dir.contains('\\') {
        return Err(err("must not contain path separators ('/' or '\\\\')"));
    }
    if dir == "." || dir == ".." {
        return Err(err("must not be '.' or '..'"));
    }
    // Absolute-path markers (already partially covered by separator check,
    // but Windows drive prefixes like `C:` are separator-free).
    let bytes = dir.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(err("must not start with a drive prefix (e.g. 'C:')"));
    }

    Ok(())
}

/// Create a symlink at `link` pointing at `target`, creating parent
/// directories as needed. Symlink creation is Unix-only; on other platforms
/// this returns `PmError::LinkNotSupported`. Shared by `pm link` and `pm sync`
/// link repair so the platform gate lives in exactly one place.
#[cfg(unix)]
pub(crate) fn create_symlink(target: &Path, link: &Path) -> anyhow::Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink(target, link).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create symlink {} → {}: {}",
            link.display(),
            target.display(),
            e
        )
    })?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn create_symlink(_target: &Path, _link: &Path) -> anyhow::Result<()> {
    Err(PmError::LinkNotSupported.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_path("~/test"), home.join("test"));
    }

    #[test]
    fn test_collapse_home() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(collapse_path(&home.join("test")), "~/test");
    }

    #[test]
    fn validator_accepts_single_name() {
        assert!(validate_project_dir("frontend").is_ok());
        assert!(validate_project_dir("my-app_2").is_ok());
        assert!(validate_project_dir(".trash").is_ok()); // dot-prefix is fine; only "."/".." rejected
    }

    #[test]
    fn validator_rejects_nested_paths() {
        assert!(validate_project_dir("web/frontend").is_err());
        assert!(validate_project_dir("a\\b").is_err());
    }

    #[test]
    fn validator_rejects_parent_traversal() {
        assert!(validate_project_dir(".").is_err());
        assert!(validate_project_dir("..").is_err());
    }

    #[test]
    fn validator_rejects_absolute_paths() {
        assert!(validate_project_dir("/abs/path").is_err()); // separator catches this
        assert!(validate_project_dir("C:").is_err()); // drive prefix
    }

    #[test]
    fn validator_rejects_empty_and_null() {
        assert!(validate_project_dir("").is_err());
        assert!(validate_project_dir("a\0b").is_err());
    }
}
