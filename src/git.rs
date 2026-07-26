use crate::path::expand_path;
use anyhow::Result;
use git2::{Repository, StatusOptions};
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

    let mut status_options = StatusOptions::new();
    status_options
        .include_untracked(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut status_options)).ok()?;
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
    let trimmed = remote.split(['?', '#']).next()?.trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
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

/// Redact URL credentials before printing a remote URL. Git credentials
/// should never be echoed into a terminal or CI log.
pub fn redact_remote_url(remote: &str) -> String {
    let mut redacted = match remote.split_once("://") {
        Some((scheme, rest)) => {
            let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            let authority = &rest[..authority_end];
            let sensitive_userinfo = authority.rfind('@').filter(|at| {
                let userinfo = &authority[..*at];
                scheme.eq_ignore_ascii_case("http")
                    || scheme.eq_ignore_ascii_case("https")
                    || userinfo.contains(':')
            });

            match sensitive_userinfo {
                Some(at) => format!(
                    "{}://***@{}{}",
                    scheme,
                    &authority[at + 1..],
                    &rest[authority_end..]
                ),
                None => remote.to_string(),
            }
        }
        None => remote.to_string(),
    };

    if let Some(query_or_fragment) = redacted.find(['?', '#']) {
        redacted.truncate(query_or_fragment + 1);
        redacted.push_str("***");
    }
    redacted
}

/// Remotes with embedded secrets must not be copied from local Git config
/// into the portable manifest. HTTP(S) userinfo is always considered
/// sensitive; other URL schemes are rejected when they contain password-like
/// userinfo. Query strings and fragments are rejected because their semantics
/// are provider-specific and they frequently carry access tokens.
pub fn remote_has_embedded_credentials(remote: &str) -> bool {
    if remote.contains(['?', '#']) {
        return true;
    }
    let Some((scheme, rest)) = remote.split_once("://") else {
        return false;
    };

    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let Some(at) = authority.rfind('@') else {
        return false;
    };
    scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || authority[..at].contains(':')
}

/// Return whether a remote can be replayed by `git clone` without depending
/// on the caller's working directory.
///
/// Supported network transports and scp-style SSH locations are independent
/// of the local cwd. Absolute filesystem paths (including `file:///...`) are
/// also stable on the machine that owns them. Relative local paths and remote
/// helper syntaxes are deliberately excluded from portable manifest metadata.
pub fn remote_is_cwd_independent(remote: &str) -> bool {
    if Path::new(remote).is_absolute() {
        return true;
    }

    if remote
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
        && !remote
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file://"))
    {
        return false;
    }

    if let Some((scheme, rest)) = remote.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return Path::new(rest).is_absolute();
        }

        if !["http", "https", "ssh", "git"]
            .iter()
            .any(|supported| scheme.eq_ignore_ascii_case(supported))
        {
            return false;
        }

        let mut parts = rest.splitn(2, '/');
        let authority = parts.next().unwrap_or_default();
        let path = parts.next().unwrap_or_default();
        return !authority.is_empty()
            && !authority.ends_with('@')
            && !path.is_empty()
            && path != "/";
    }

    is_scp_style_remote(remote)
}

fn is_scp_style_remote(remote: &str) -> bool {
    let separator = remote
        .find("]:")
        .map(|close_bracket| close_bracket + 1)
        .or_else(|| remote.find(':'));
    let Some(separator) = separator else {
        return false;
    };

    let location = &remote[..separator];
    let path = &remote[separator + 1..];
    if location.is_empty()
        || location.contains(['/', '\\'])
        || location.chars().any(char::is_whitespace)
        || path.is_empty()
        || path.starts_with(':')
    {
        return false;
    }

    let host = location.rsplit_once('@').map_or(location, |(_, host)| host);
    !host.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::init_git_repo;

    #[test]
    fn derives_slug_after_trailing_slash_and_before_query() {
        assert_eq!(
            repo_slug_from_remote("https://github.com/acme/widgets.git/"),
            Some("widgets".to_string())
        );
        assert_eq!(
            repo_slug_from_remote("https://github.com/acme/widgets.git?token=secret"),
            Some("widgets".to_string())
        );
    }

    #[test]
    fn redacts_url_credentials_but_not_plain_ssh_username() {
        assert_eq!(
            redact_remote_url("https://token@github.com/acme/widgets.git"),
            "https://***@github.com/acme/widgets.git"
        );
        assert_eq!(
            redact_remote_url("ssh://git:secret@github.com/acme/widgets.git"),
            "ssh://***@github.com/acme/widgets.git"
        );
        assert_eq!(
            redact_remote_url("https://github.com/acme/widgets.git?access_token=secret"),
            "https://github.com/acme/widgets.git?***"
        );
        assert_eq!(
            redact_remote_url("git@github.com:acme/widgets.git?access_token=secret"),
            "git@github.com:acme/widgets.git?***"
        );
        assert_eq!(
            redact_remote_url("ssh://git@github.com/acme/widgets.git"),
            "ssh://git@github.com/acme/widgets.git"
        );
        assert!(remote_has_embedded_credentials(
            "https://token@github.com/acme/widgets.git"
        ));
        assert!(!remote_has_embedded_credentials(
            "ssh://git@github.com/acme/widgets.git"
        ));
        assert!(remote_has_embedded_credentials(
            "ssh://git:secret@github.com/acme/widgets.git"
        ));
        assert!(remote_has_embedded_credentials(
            "https://github.com/acme/widgets.git?access_token=secret"
        ));
        assert!(remote_has_embedded_credentials(
            "git@github.com:acme/widgets.git?access_token=secret"
        ));
    }

    #[test]
    fn cwd_independent_remotes_include_supported_network_forms() {
        for remote in [
            "https://github.com/acme/widgets.git",
            "http://git.example.com/acme/widgets.git",
            "ssh://git@example.com/acme/widgets.git",
            "git://example.com/acme/widgets.git",
            "git@example.com:acme/widgets.git",
            "example.com:/srv/git/widgets.git",
            "git@[2001:db8::1]:acme/widgets.git",
        ] {
            assert!(
                remote_is_cwd_independent(remote),
                "{remote} should be safe to replay from any cwd"
            );
        }
    }

    #[test]
    fn cwd_independent_remotes_allow_only_absolute_local_paths() {
        assert!(remote_is_cwd_independent("/srv/git/widgets.git"));
        assert!(remote_is_cwd_independent(
            "file:///srv/git/widgets.git"
        ));

        for remote in [
            "widgets.git",
            "./widgets.git",
            "../widgets.git",
            "~/src/widgets",
            "file:../widgets.git",
            "file://relative/widgets.git",
            "ext::ssh -i key example.com %S widgets.git",
            "ftp://example.com/acme/widgets.git",
        ] {
            assert!(
                !remote_is_cwd_independent(remote),
                "{remote} should not be persisted as recovery metadata"
            );
        }
    }

    #[test]
    fn status_excludes_ignored_files_but_keeps_untracked_files() {
        let temp = tempfile::tempdir().unwrap();
        init_git_repo(temp.path());

        std::fs::write(
            temp.path().join(".gitignore"),
            ".env\n.next/\nnode_modules/\n",
        )
        .unwrap();
        for args in [
            ["add", ".gitignore"].as_slice(),
            ["commit", "-q", "-m", "add ignores"].as_slice(),
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(temp.path())
                .status()
                .unwrap();
            assert!(status.success());
        }

        std::fs::write(temp.path().join(".env"), "SECRET=ignored\n").unwrap();
        std::fs::create_dir(temp.path().join(".next")).unwrap();
        std::fs::write(temp.path().join(".next/cache"), "ignored\n").unwrap();
        std::fs::create_dir(temp.path().join("node_modules")).unwrap();
        std::fs::write(
            temp.path().join("node_modules/package.json"),
            "{\"private\":true}\n",
        )
        .unwrap();

        let clean = get_status(temp.path().to_str().unwrap()).unwrap();
        assert!(clean.is_clean);
        assert_eq!(clean.changed_count, 0);

        std::fs::write(temp.path().join("visible.txt"), "untracked\n").unwrap();
        let changed = get_status(temp.path().to_str().unwrap()).unwrap();
        assert!(!changed.is_clean);
        assert_eq!(changed.changed_count, 1);
    }
}
