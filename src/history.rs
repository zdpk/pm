use crate::config::{load_history, save_history};
use crate::models::{HistoryAction, HistoryData, HistoryEntry, HistoryProjectSnapshot, Project};
use crate::path::collapse_path;
use chrono::Utc;
use std::path::Path;

pub fn record_project_event(
    project: &Project,
    resolved_path: &Path,
    action: HistoryAction,
) -> anyhow::Result<()> {
    let mut history = load_history()?;
    history.entries.push(HistoryEntry {
        timestamp: Utc::now(),
        action,
        project: HistoryProjectSnapshot {
            name: project.name.clone(),
            workspace: project.workspace.clone(),
            repo_slug: project.repo_slug.clone(),
            dir: project.dir.clone(),
            remote: project.remote.clone(),
            path: collapse_path(resolved_path),
        },
    });
    trim_history(&mut history);
    save_history(&history)?;
    Ok(())
}

fn trim_history(history: &mut HistoryData) {
    const MAX_ENTRIES: usize = 1000;
    if history.entries.len() > MAX_ENTRIES {
        let overflow = history.entries.len() - MAX_ENTRIES;
        history.entries.drain(0..overflow);
    }
}
