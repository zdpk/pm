use crate::path::expand_path;
use anyhow::Result;
use git2::Repository;
use std::path::Path;
use std::process::Command;

/// Check if path is a git repository
pub fn is_git_repo(path: &str) -> bool {
    let expanded = expand_path(path);
    Repository::open(&expanded).is_ok()
}

/// Get git remote origin URL
pub fn get_remote_url(path: &str) -> Option<String> {
    let expanded = expand_path(path);
    let repo = Repository::open(&expanded).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().map(|s| s.to_string())
}

/// Git repository status
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub is_clean: bool,
    pub changed_count: usize,
    pub ahead: usize,
    pub behind: usize,
    pub has_conflict: bool,
}

impl GitStatus {
    pub fn display(&self) -> String {
        if self.has_conflict {
            return "conflict".to_string();
        }

        let mut parts = Vec::new();

        if self.ahead > 0 {
            parts.push(format!("{}↑", self.ahead));
        }
        if self.behind > 0 {
            parts.push(format!("{}↓", self.behind));
        }

        if self.changed_count > 0 {
            parts.push(format!("{} changed", self.changed_count));
        } else if parts.is_empty() {
            return "clean".to_string();
        }

        parts.join(" ")
    }
}

/// Get detailed git status
pub fn get_status(path: &str) -> Option<GitStatus> {
    let expanded = expand_path(path);
    let repo = Repository::open(&expanded).ok()?;

    // Get branch name, handling unborn branches (no commits yet)
    let branch = match repo.head() {
        Ok(head) => head.shorthand().map(|s| s.to_string()),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // No commits yet, try to get branch name from HEAD reference
            repo.find_reference("HEAD")
                .ok()
                .and_then(|r| r.symbolic_target().map(|s| s.to_string()))
                .map(|s| s.trim_start_matches("refs/heads/").to_string())
        }
        Err(_) => None,
    };

    let statuses = repo.statuses(None).ok()?;
    let changed_count = statuses.len();
    let has_conflict = statuses.iter().any(|s| s.status().is_conflicted());

    // Get ahead/behind counts
    let (ahead, behind) = get_ahead_behind(&repo).unwrap_or((0, 0));

    Some(GitStatus {
        branch,
        is_clean: changed_count == 0,
        changed_count,
        ahead,
        behind,
        has_conflict,
    })
}

fn get_ahead_behind(repo: &Repository) -> Option<(usize, usize)> {
    let head = repo.head().ok()?;
    let local_oid = head.target()?;

    let branch_name = head.shorthand()?;
    let upstream_name = format!("origin/{}", branch_name);

    let upstream = repo
        .find_reference(&format!("refs/remotes/{}", upstream_name))
        .ok()?;
    let upstream_oid = upstream.target()?;

    repo.graph_ahead_behind(local_oid, upstream_oid).ok()
}

/// Set git config for repository
pub fn set_git_config(path: &str, key: &str, value: &str) -> Result<()> {
    let expanded = expand_path(path);
    let repo = Repository::open(&expanded)?;
    let mut config = repo.config()?;
    config.set_str(key, value)?;
    Ok(())
}

pub fn repo_slug_from_remote(remote: &str) -> Option<String> {
    let trimmed = remote.trim_end_matches(".git").trim_end_matches('/');
    let slug = trimmed
        .rsplit(['/', ':'])
        .next()
        .filter(|value| !value.is_empty())?;
    Some(slug.to_string())
}

pub fn remote_matches(path: &Path, expected: &str) -> bool {
    let Ok(repo) = Repository::open(path) else {
        return false;
    };
    let Ok(remote) = repo.find_remote("origin") else {
        return false;
    };
    remote.url().is_some_and(|url| url == expected)
}

pub fn clone_repo(remote: &str, target: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("clone")
        .arg(remote)
        .arg(target)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("git clone failed for {}", remote))
    }
}
