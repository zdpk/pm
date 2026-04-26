use crate::error::PmError;
use crate::git::{clone_repo, is_git_repo, remote_matches};
use crate::restore::{can_prompt, prompt_yes_no};
use crate::state::{find_workspace, load_state, project_path};
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
struct SyncTask {
    name: String,
    remote: String,
    path: PathBuf,
}

pub fn run(workspace: Option<String>, yes: bool, jobs: usize) -> Result<()> {
    let (config, manifest) = load_state()?;

    if let Some(ref workspace_name) = workspace {
        let _ = find_workspace(&manifest, workspace_name)?;
    }

    let mut tasks = Vec::new();
    for project in &manifest.projects {
        if workspace
            .as_ref()
            .is_some_and(|name| &project.workspace != name)
        {
            continue;
        }

        let path = project_path(&config, &manifest, project)?;
        if !path.exists() {
            if let Some(remote) = &project.remote {
                tasks.push(SyncTask {
                    name: project.name.clone(),
                    remote: remote.clone(),
                    path,
                });
            } else {
                println!(
                    "{} '{}' is missing and cannot be restored automatically (no remote)",
                    "✗".red(),
                    project.name
                );
            }
            continue;
        }

        if is_git_repo(&path.display().to_string()) {
            if let Some(remote) = &project.remote {
                if !remote_matches(&path, remote) {
                    println!(
                        "{} '{}' has remote mismatch at {}",
                        "✗".red(),
                        project.name,
                        path.display()
                    );
                }
            }
        } else {
            println!(
                "{} '{}' conflicts with a non-git directory at {}",
                "✗".red(),
                project.name,
                path.display()
            );
        }
    }

    if tasks.is_empty() {
        println!("No missing projects to restore.");
        return Ok(());
    }

    if !yes && !can_prompt() {
        return Err(PmError::NonInteractiveRestore(tasks[0].name.clone()).into());
    }

    if !yes {
        let should_restore = prompt_yes_no(
            &format!(
                "Found {} missing project(s). Restore them now?",
                tasks.len()
            ),
            true,
        )?;
        if !should_restore {
            println!("Skipped restore.");
            return Ok(());
        }
    }

    let queue = Arc::new(Mutex::new(tasks));
    let worker_count = jobs.max(1);
    let mut handles = Vec::new();

    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        handles.push(thread::spawn(move || -> Vec<String> {
            let mut messages = Vec::new();
            loop {
                let task = {
                    let mut queue = queue.lock().expect("queue lock poisoned");
                    queue.pop()
                };

                let Some(task) = task else {
                    break;
                };

                if let Some(parent) = task.path.parent() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        messages.push(format!("✗ '{}' failed: {}", task.name, err));
                        continue;
                    }
                }

                match clone_repo(&task.remote, &task.path) {
                    Ok(()) => messages.push(format!(
                        "✓ Restored '{}' to {}",
                        task.name,
                        task.path.display()
                    )),
                    Err(err) => messages.push(format!("✗ '{}' failed: {}", task.name, err)),
                }
            }
            messages
        }));
    }

    for handle in handles {
        for message in handle
            .join()
            .map_err(|_| anyhow::anyhow!("sync worker panicked"))?
        {
            println!("{}", message);
        }
    }

    Ok(())
}
