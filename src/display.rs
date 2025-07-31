use crate::constants::*;
use crate::Project;
use chrono::{DateTime, Utc};

pub fn format_relative_time(time: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(time);

    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        let minutes = duration.num_minutes();
        if minutes == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", minutes)
        }
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if duration.num_days() == 1 {
        "yesterday".to_string()
    } else if duration.num_days() < 7 {
        let days = duration.num_days();
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else if duration.num_weeks() == 1 {
        "1 week ago".to_string()
    } else if duration.num_weeks() < 4 {
        let weeks = duration.num_weeks();
        format!("{} weeks ago", weeks)
    } else if duration.num_days() < 365 {
        let months = duration.num_days() / 30;
        if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", months)
        }
    } else {
        let years = duration.num_days() / 365;
        if years == 1 {
            "1 year ago".to_string()
        } else {
            format!("{} years ago", years)
        }
    }
}

pub fn display_project_simple(project: &Project, access_time: Option<DateTime<Utc>>) {
    let tags_display = if project.tags.is_empty() {
        "".to_string()
    } else {
        format!("[{}]", project.tags.join(", "))
    };

    let git_status = if project.is_git_repository {
        "📁"
    } else {
        "❌"
    };

    let last_updated_display = if let Some(git_time) = project.git_updated_at {
        format!("Git: {}", format_relative_time(git_time))
    } else {
        format!("PM: {}", format_relative_time(project.updated_at))
    };

    let access_display = if let Some(access_time) = access_time {
        format!(" (accessed: {})", format_relative_time(access_time))
    } else {
        "".to_string()
    };

    println!(
        "{:<width_name$} {:<width_path$} {:<width_git$} {:<width_tags$} {:<width_time$}{}",
        project.name,
        project.path.display().to_string(),
        git_status,
        tags_display,
        last_updated_display,
        access_display,
        width_name = PROJECT_NAME_WIDTH,
        width_path = PROJECT_PATH_WIDTH,
        width_git = PROJECT_GIT_WIDTH,
        width_tags = PROJECT_TAGS_WIDTH,
        width_time = PROJECT_TIME_WIDTH
    );
}

pub fn display_project_detailed(
    project: &Project,
    access_time: Option<DateTime<Utc>>,
    access_count: u32,
) {
    println!("\n{}", project.name);
    if !project.tags.is_empty() {
        println!("  Tags: {}", project.tags.join(", "));
    }
    println!("  Path: {}", project.path.display());
    if let Some(desc) = &project.description {
        println!("  Description: {}", desc);
    }
    println!("  ID: {}", project.id);
    println!(
        "  Created: {}",
        project.created_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "  Updated: {}",
        project.updated_at.format("%Y-%m-%d %H:%M:%S")
    );
    if let Some(git_time) = project.git_updated_at {
        println!(
            "  Git Updated: {} ({})",
            git_time.format("%Y-%m-%d %H:%M:%S"),
            format_relative_time(git_time)
        );
    }
    if let Some(access_time) = access_time {
        println!(
            "  Last Accessed: {} ({})",
            access_time.format("%Y-%m-%d %H:%M:%S"),
            format_relative_time(access_time)
        );
    }
    if access_count > 0 {
        println!("  Access Count: {}", access_count);
    }
}

pub fn display_project_list_header(count: usize) {
    println!("📋 Active Projects ({} found)", count);
    println!();
    println!(
        "{:<width_name$} {:<width_path$} {:<width_git$} {:<width_tags$} {:<width_time$}",
        "NAME",
        "PATH",
        "GIT",
        "TAGS",
        "TIME",
        width_name = PROJECT_NAME_WIDTH,
        width_path = PROJECT_PATH_WIDTH,
        width_git = PROJECT_GIT_WIDTH,
        width_tags = PROJECT_TAGS_WIDTH,
        width_time = PROJECT_TIME_WIDTH
    );
}

pub fn display_no_projects() {
    println!("📋 No projects found");
    println!("\n💡 {}", crate::constants::get_suggestion_add_first_project());
}

pub fn display_no_matches() {
    let binary = crate::utils::get_binary_name();
    println!("📋 No projects match your filters");
    println!("\n💡 Try:");
    println!("  - No filters: {} ls", binary);
    println!("  - Longer time period: {} ls -r 30d", binary);
    println!("  - Different tags: {} ls --tags-any frontend,backend", binary);
}

pub fn display_switch_info(
    project_name: &str,
    access_count: u32,
    last_accessed: Option<DateTime<Utc>>,
) {
    println!("🔄 Switching to project: {}", project_name);
    println!("📊 Access count: {} times", access_count);

    if let Some(last_time) = last_accessed {
        println!("⏰ Last accessed: {}", format_relative_time(last_time));
    }
}

pub fn display_switch_success(project_path: &std::path::Path) {
    println!("📂 Working directory: {}", project_path.display());
    println!("✅ Project switched");
}

pub fn display_suggestions(suggestions: &[String]) {
    if !suggestions.is_empty() {
        println!("\n💡 Did you mean one of these?");
        for suggestion in suggestions.iter().take(3) {
            println!("  - {}", suggestion);
        }
    } else {
        println!("\n💡 {}", crate::constants::get_suggestion_use_pm_ls());
    }
}

pub fn display_error(context: &str, error: &str) {
    eprintln!("❌ {}: {}", context, error);
}

pub fn display_warning(message: &str) {
    eprintln!("⚠️  {}", message);
}

pub fn display_success(message: &str) {
    println!("✅ {}", message);
}

pub fn display_info(message: &str) {
    println!("💡 {}", message);
}

#[allow(dead_code)]
pub fn display_project_added(project_name: &str, tags: &[String]) {
    println!("✅ Project '{}' added successfully!", project_name);
    if !tags.is_empty() {
        println!("🏷️  Tags: {}", tags.join(", "));
    }
}


pub fn display_init_success(
    config_dir: &std::path::Path,
    config_file_path: &std::path::Path,
) {
    println!("\n✅ {}", SUCCESS_PM_INITIALIZED);
    println!("📂 Config directory: {}", config_dir.display());
    println!("⚙️  Config file: {}", config_file_path.display());
}
