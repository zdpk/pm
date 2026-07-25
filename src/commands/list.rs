use crate::cli::{FilterType, SortField};
use crate::git::{get_remote_url, get_status, is_git_repo, redact_remote_url};
use crate::path::expand_path;
use crate::state::{load_state, project_path, project_path_display};
use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteState {
    Current,
    Unrecorded,
    Changed,
    Saved,
    Missing,
}

#[derive(Debug, PartialEq, Eq)]
struct RemoteValue {
    url: Option<String>,
    state: RemoteState,
}

pub struct ListOptions {
    pub all: bool,
    pub tags_filter: Option<String>,
    pub show_path: bool,
    pub show_remote: bool,
    pub no_status: bool,
    pub sort: SortField,
    pub reverse: bool,
    pub filter: Option<FilterType>,
}

fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);
    let seconds = duration.num_seconds();
    if seconds < 0 {
        return "just now".to_string();
    }
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();
    let weeks = days / 7;
    let months = days / 30;
    let years = days / 365;

    if seconds < 60 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days < 7 {
        format!("{}d ago", days)
    } else if weeks < 4 {
        format!("{}w ago", weeks)
    } else if months < 12 {
        format!("{}mo ago", months)
    } else {
        format!("{}y ago", years)
    }
}

pub fn run(options: ListOptions) -> Result<()> {
    let ListOptions {
        all,
        tags_filter,
        show_path,
        show_remote,
        no_status,
        sort,
        reverse,
        filter,
    } = options;
    let (config, manifest) = load_state()?;
    let workspace_names: Vec<&str> = if all {
        manifest
            .workspaces
            .iter()
            .filter(|ws| !ws.is_system())
            .map(|ws| ws.name.as_str())
            .collect()
    } else {
        vec![config.current_workspace.as_str()]
    };

    let tag_filters: Vec<String> = tags_filter
        .map(|t| t.split(',').map(|s| s.trim().to_lowercase()).collect())
        .unwrap_or_default();

    for ws_name in workspace_names {
        let mut projects: Vec<_> = manifest
            .projects
            .iter()
            .filter(|project| project.workspace == ws_name)
            .collect();

        if !tag_filters.is_empty() {
            projects.retain(|project| {
                project
                    .tags
                    .iter()
                    .any(|tag| tag_filters.contains(&tag.to_lowercase()))
            });
        }

        if let Some(ref filter) = filter {
            projects.retain(|project| {
                let path = project_path(&config, &manifest, project).ok();
                match filter {
                    FilterType::Git => path
                        .as_ref()
                        .is_some_and(|path| is_git_repo(&path.display().to_string())),
                    FilterType::NonGit => path.as_ref().is_some_and(|path| {
                        path.exists() && !is_git_repo(&path.display().to_string())
                    }),
                    FilterType::Orphan => path.as_ref().is_some_and(|path| !path.exists()),
                }
            });
        }

        match sort {
            SortField::Accessed => projects.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed)),
            SortField::Name => projects.sort_by(|a, b| a.name.cmp(&b.name)),
            SortField::Path => projects.sort_by(|a, b| a.dir.cmp(&b.dir)),
            SortField::Added => projects.sort_by(|a, b| b.added_at.cmp(&a.added_at)),
            SortField::Frequency => projects.sort_by(|a, b| b.access_count.cmp(&a.access_count)),
            SortField::Status => projects.sort_by(|a, b| {
                let a_path = project_path(&config, &manifest, a).ok();
                let b_path = project_path(&config, &manifest, b).ok();
                let a_status = a_path
                    .as_ref()
                    .and_then(|path| get_status(&path.display().to_string()));
                let b_status = b_path
                    .as_ref()
                    .and_then(|path| get_status(&path.display().to_string()));
                let a_clean = a_status.as_ref().is_some_and(|s| s.is_clean);
                let b_clean = b_status.as_ref().is_some_and(|s| s.is_clean);
                a_clean.cmp(&b_clean)
            }),
        }

        if reverse {
            projects.reverse();
        }

        let count_text = if projects.len() == 1 {
            "1 project".to_string()
        } else {
            format!("{} projects", projects.len())
        };
        println!(
            "{} {} {}",
            "●".cyan(),
            ws_name.cyan().bold(),
            format!("({})", count_text).dimmed()
        );

        if projects.is_empty() {
            println!();
            continue;
        }

        println!();
        let name_width = projects
            .iter()
            .map(|p| p.name.len())
            .max()
            .unwrap_or(4)
            .max(4);
        let branch_width = 12;
        let status_width = 16;
        let time_width = 8;
        let tail_header = if show_remote { "REMOTE" } else { "PATH" };

        println!(
            "{}",
            format!(
                "  {:<name_w$}   {:<branch_w$}   {:<status_w$}   {:<time_w$}   {}",
                "NAME",
                "BRANCH",
                "STATUS",
                "LAST",
                tail_header,
                name_w = name_width,
                branch_w = branch_width,
                status_w = status_width,
                time_w = time_width,
            )
            .dimmed()
        );

        for project in projects {
            let path = project_path(&config, &manifest, project)?;
            let tail_display = if show_remote {
                let remote_path = project
                    .link
                    .as_deref()
                    .map(expand_path)
                    .unwrap_or_else(|| path.clone());
                format_remote_value(resolve_remote_value(
                    &remote_path,
                    project.remote.as_deref(),
                ))
            } else {
                let collapsed_path = project_path_display(&config, &manifest, project)?;
                let path_display = if show_path {
                    path.display().to_string()
                } else {
                    collapsed_path
                };
                path_display.dimmed().to_string()
            };

            let (branch, status_text, status_color) = if !path.exists() {
                ("-".to_string(), "missing".to_string(), "red")
            } else if no_status {
                ("-".to_string(), "-".to_string(), "dimmed")
            } else if let Some(git_status) = get_status(&path.display().to_string()) {
                let branch = git_status
                    .branch
                    .clone()
                    .unwrap_or_else(|| "detached".to_string());
                if git_status.is_clean {
                    (branch, "clean".to_string(), "green")
                } else if git_status.has_conflict {
                    (branch, "conflict".to_string(), "red")
                } else {
                    (branch, git_status.display(), "yellow")
                }
            } else {
                ("-".to_string(), "not git".to_string(), "dimmed")
            };

            let time_ago = format_relative_time(&project.last_accessed);
            let tags_suffix = if project.tags.is_empty() {
                String::new()
            } else {
                format!(
                    "  {}",
                    project
                        .tags
                        .iter()
                        .map(|tag| format!("#{}", tag))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            let is_current = config.current_project.as_deref() == Some(project.name.as_str());
            let marker = if is_current { "* " } else { "  " };
            let status_colored = match status_color {
                "red" => format!("{:<width$}", status_text, width = status_width)
                    .red()
                    .to_string(),
                "green" => format!("{:<width$}", status_text, width = status_width)
                    .green()
                    .to_string(),
                "yellow" => format!("{:<width$}", status_text, width = status_width)
                    .yellow()
                    .to_string(),
                _ => format!("{:<width$}", status_text, width = status_width)
                    .dimmed()
                    .to_string(),
            };

            println!(
                "{}{:<name_w$}   {:<branch_w$}   {}   {:<time_w$}   {}{}",
                if is_current {
                    marker.cyan().bold().to_string()
                } else {
                    marker.to_string()
                },
                if is_current {
                    project.name.cyan().bold().to_string()
                } else {
                    project.name.clone()
                },
                branch,
                status_colored,
                time_ago,
                tail_display,
                tags_suffix.dimmed(),
                name_w = name_width,
                branch_w = branch_width,
                time_w = time_width,
            );
        }

        println!();
    }

    Ok(())
}

fn resolve_remote_value(path: &Path, saved: Option<&str>) -> RemoteValue {
    match get_remote_url(&path.display().to_string()).filter(|url| !url.trim().is_empty()) {
        Some(actual) => {
            let state = match saved {
                None => RemoteState::Unrecorded,
                Some(saved) if actual == saved => RemoteState::Current,
                Some(_) => RemoteState::Changed,
            };
            RemoteValue {
                url: Some(actual),
                state,
            }
        }
        None => match saved {
            Some(saved) => RemoteValue {
                url: Some(saved.to_string()),
                state: RemoteState::Saved,
            },
            None => RemoteValue {
                url: None,
                state: RemoteState::Missing,
            },
        },
    }
}

fn format_remote_value(remote: RemoteValue) -> String {
    let Some(url) = remote.url else {
        return "-".dimmed().to_string();
    };
    let url = redact_remote_url(&url);
    match remote.state {
        RemoteState::Current => url,
        RemoteState::Unrecorded => {
            format!("{} {}", url, "(unrecorded)".yellow())
        }
        RemoteState::Changed => format!("{} {}", url, "(changed)".yellow()),
        RemoteState::Saved => format!("{} {}", url, "(saved)".dimmed()),
        RemoteState::Missing => unreachable!("missing remote has no URL"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Project;
    use crate::state::{load_state, save_state};
    use crate::test_support::{TestEnv, init_git_repo};
    use git2::Repository;

    fn repo_with_origin(path: &Path, remote: &str) {
        let repo = Repository::init(path).expect("initialize git repository");
        repo.remote("origin", remote).expect("set origin");
    }

    #[test]
    fn live_origin_takes_precedence_over_saved_remote() {
        let temp = tempfile::tempdir().unwrap();
        repo_with_origin(temp.path(), "git@github.com:acme/current.git");

        let value = resolve_remote_value(temp.path(), Some("https://github.com/acme/previous.git"));

        assert_eq!(
            value,
            RemoteValue {
                url: Some("git@github.com:acme/current.git".to_string()),
                state: RemoteState::Changed,
            }
        );
    }

    #[test]
    fn exact_live_origin_is_current() {
        let temp = tempfile::tempdir().unwrap();
        repo_with_origin(temp.path(), "https://github.com/acme/widgets.git");

        let value = resolve_remote_value(temp.path(), Some("https://github.com/acme/widgets.git"));

        assert_eq!(value.state, RemoteState::Current);
    }

    #[test]
    fn transport_change_is_reported_as_changed() {
        let temp = tempfile::tempdir().unwrap();
        repo_with_origin(temp.path(), "git@github.com:acme/widgets.git");

        let value = resolve_remote_value(temp.path(), Some("https://github.com/acme/widgets.git"));

        assert_eq!(value.state, RemoteState::Changed);
    }

    #[test]
    fn saved_remote_is_fallback_when_live_origin_is_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");

        let value = resolve_remote_value(&missing, Some("https://github.com/acme/widgets.git"));

        assert_eq!(
            value,
            RemoteValue {
                url: Some("https://github.com/acme/widgets.git".to_string()),
                state: RemoteState::Saved,
            }
        );
    }

    #[test]
    fn missing_live_and_saved_remote_displays_dash() {
        let temp = tempfile::tempdir().unwrap();
        let value = resolve_remote_value(&temp.path().join("missing"), None);

        assert_eq!(value.state, RemoteState::Missing);
        assert_eq!(format_remote_value(value), "-".dimmed().to_string());
    }

    #[test]
    fn remote_listing_does_not_change_manifest_or_git_origin() {
        let env = TestEnv::new();
        let path = env.project_dir("default", "widgets");
        init_git_repo(&path);
        repo_with_origin(&path, "git@github.com:acme/widgets.git");

        let (config, mut manifest) = load_state().unwrap();
        let mut project = Project::new(
            "widgets".to_string(),
            "default".to_string(),
            "widgets".to_string(),
            "widgets".to_string(),
        );
        project.remote = Some("https://github.com/acme/old.git".to_string());
        manifest.projects.push(project);
        save_state(&config, &manifest).unwrap();

        let manifest_path = env.config_dir.join("manifest.json");
        let before_manifest = std::fs::read(&manifest_path).unwrap();
        let before_origin = get_remote_url(&path.display().to_string());

        run(ListOptions {
            all: false,
            tags_filter: None,
            show_path: false,
            show_remote: true,
            no_status: true,
            sort: SortField::Accessed,
            reverse: false,
            filter: None,
        })
        .expect("remote listing should succeed");

        assert_eq!(std::fs::read(manifest_path).unwrap(), before_manifest);
        assert_eq!(
            get_remote_url(&path.display().to_string()),
            before_origin,
            "listing must not modify origin"
        );
    }
}
