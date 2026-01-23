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

/// Check if path exists
pub fn path_exists(path: &str) -> bool {
    expand_path(path).exists()
}

/// Check if path is a directory
pub fn is_directory(path: &str) -> bool {
    expand_path(path).is_dir()
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
}
