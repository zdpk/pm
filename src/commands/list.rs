use crate::cli::{FilterType, SortField};
use crate::config::{load_projects, load_workspaces};
use crate::git::{get_status, is_git_repo};
use crate::path::{expand_path, path_exists};
use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;

/// Format a timestamp as relative time (e.g., "2h ago", "3d ago")
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
    let projects_data = load_projects()?;
    let workspaces_data = load_workspaces()?;

    // Determine which workspace(s) to show
    let workspace_names: Vec<&str> = if all {
        workspaces_data
            .workspaces
            .iter()
            .filter(|w| !w.is_system()) // Hide .trash
            .map(|w| w.name.as_str())
            .collect()
    } else {
        vec![workspaces_data.current.as_str()]
    };

    // Parse tag filter
    let tag_filters: Vec<String> = tags_filter
        .map(|t| t.split(',').map(|s| s.trim().to_lowercase()).collect())
        .unwrap_or_default();

    for ws_name in workspace_names {
        let ws = workspaces_data
            .workspaces
            .iter()
            .find(|w| w.name == ws_name);

        let ws = match ws {
            Some(w) => w,
            None => continue,
        };

        // Get projects for this workspace
        let mut projects: Vec<_> = projects_data
            .projects
            .iter()
            .filter(|p| ws.projects.contains(&p.name))
            .collect();

        // Apply tag filter
        if !tag_filters.is_empty() {
            projects.retain(|p| {
                p.tags
                    .iter()
                    .any(|t| tag_filters.contains(&t.to_lowercase()))
            });
        }

        // Apply type filter
        if let Some(ref f) = filter {
            projects.retain(|p| match f {
                FilterType::Git => is_git_repo(&p.path),
                FilterType::NonGit => !is_git_repo(&p.path),
                FilterType::Orphan => !path_exists(&p.path),
            });
        }

        // Sort
        match sort {
            SortField::Accessed => {
                projects.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
            }
            SortField::Name => {
                projects.sort_by(|a, b| a.name.cmp(&b.name));
            }
            SortField::Path => {
                projects.sort_by(|a, b| a.path.cmp(&b.path));
            }
            SortField::Added => {
                projects.sort_by(|a, b| b.added_at.cmp(&a.added_at));
            }
            SortField::Frequency => {
                projects.sort_by(|a, b| b.access_count.cmp(&a.access_count));
            }
            SortField::Status => {
                projects.sort_by(|a, b| {
                    let a_status = get_status(&a.path);
                    let b_status = get_status(&b.path);
                    let a_clean = a_status.as_ref().is_some_and(|s| s.is_clean);
                    let b_clean = b_status.as_ref().is_some_and(|s| s.is_clean);
                    a_clean.cmp(&b_clean)
                });
            }
        }

        if reverse {
            projects.reverse();
        }

        // Print workspace header with project count
        let project_count = projects.len();
        let count_text = if project_count == 1 {
            "1 project".to_string()
        } else {
            format!("{} projects", project_count)
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

        // Calculate column widths (add 2 for "* " prefix)
        let name_width = projects.iter().map(|p| p.name.len()).max().unwrap_or(4).max(4);
        let branch_width = 12; // Fixed width for branch column
        let status_width = 12; // Fixed width for status column
        let time_width = 8;    // Fixed width for time column

        // Get current project
        let current_project = workspaces_data.current_project.as_deref();

        // Print header (with 2-char prefix column for marker)
        println!(
            "{}",
            format!(
                "  {:<name_w$}   {:<branch_w$}   {:<status_w$}   {:<time_w$}   {}",
                "NAME", "BRANCH", "STATUS", "LAST", "PATH",
                name_w = name_width,
                branch_w = branch_width,
                status_w = status_width,
                time_w = time_width,
            ).dimmed()
        );

        // Print each project as a table row
        for p in &projects {
            let path_display = if show_path {
                expand_path(&p.path).display().to_string()
            } else {
                p.path.clone()
            };

            // Get git info (plain strings for proper padding)
            let (branch, status_text, status_color) = if !path_exists(&p.path) {
                ("-".to_string(), "not found".to_string(), "red")
            } else if no_status {
                ("-".to_string(), "-".to_string(), "dimmed")
            } else if let Some(git_status) = get_status(&p.path) {
                let branch = git_status
                    .branch
                    .clone()
                    .unwrap_or_else(|| "detached".to_string());

                if git_status.is_clean {
                    (branch, "clean".to_string(), "green")
                } else if git_status.has_conflict {
                    (branch, "conflict".to_string(), "red")
                } else {
                    let status = git_status.display();
                    (branch, status, "yellow")
                }
            } else {
                ("-".to_string(), "not git".to_string(), "dimmed")
            };

            // Format relative time
            let time_ago = format_relative_time(&p.last_accessed);

            // Tags suffix
            let tags_suffix = if p.tags.is_empty() {
                String::new()
            } else {
                format!(
                    "  {}",
                    p.tags.iter()
                        .map(|t| format!("#{}", t))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };

            // Check if this is the current project
            let is_current = current_project == Some(p.name.as_str());

            // Pad plain strings first, then colorize
            let name_padded = format!("{:<width$}", p.name, width = name_width);
            let branch_padded = format!("{:<width$}", branch, width = branch_width);
            let status_padded = format!("{:<width$}", status_text, width = status_width);
            let time_padded = format!("{:<width$}", time_ago, width = time_width);

            // Prefix marker: "* " for current, "  " for others
            let marker = if is_current { "* " } else { "  " };

            // Apply status color
            let status_colored = match status_color {
                "red" => status_padded.red().to_string(),
                "green" => status_padded.green().to_string(),
                "yellow" => status_padded.yellow().to_string(),
                _ => status_padded.dimmed().to_string(),
            };

            if is_current {
                // Current project: marker + name bold/cyan, rest normal
                println!(
                    "{}{}   {}   {}   {}   {}{}",
                    marker.cyan().bold(),
                    name_padded.cyan().bold(),
                    branch_padded.dimmed(),
                    status_colored,
                    time_padded.dimmed(),
                    path_display.dimmed(),
                    tags_suffix.dimmed(),
                );
            } else {
                // Non-current project: all normal
                println!(
                    "{}{}   {}   {}   {}   {}{}",
                    marker,
                    name_padded,
                    branch_padded.dimmed(),
                    status_colored,
                    time_padded.dimmed(),
                    path_display.dimmed(),
                    tags_suffix.dimmed(),
                );
            }
        }

        println!();
    }

    Ok(())
}
