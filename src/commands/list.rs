use crate::cli::{FilterType, SortField};
use crate::git::{get_status, is_git_repo};
use crate::state::{load_state, project_path, project_path_display};
use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;

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

pub fn run(
    all: bool,
    tags_filter: Option<String>,
    show_path: bool,
    no_status: bool,
    sort: SortField,
    reverse: bool,
    filter: Option<FilterType>,
) -> Result<()> {
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

        println!(
            "{}",
            format!(
                "  {:<name_w$}   {:<branch_w$}   {:<status_w$}   {:<time_w$}   {}",
                "NAME",
                "BRANCH",
                "STATUS",
                "LAST",
                "PATH",
                name_w = name_width,
                branch_w = branch_width,
                status_w = status_width,
                time_w = time_width,
            )
            .dimmed()
        );

        for project in projects {
            let path = project_path(&config, &manifest, project)?;
            let collapsed_path = project_path_display(&config, &manifest, project)?;
            let path_display = if show_path {
                path.display().to_string()
            } else {
                collapsed_path
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
                path_display.dimmed(),
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
