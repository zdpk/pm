use crate::history::record_project_event;
use crate::models::HistoryAction;
use crate::state::{find_project, load_state, project_path, save_state};
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};

pub fn run(project: String, yes: bool, force: bool, recursive: bool) -> Result<()> {
    if recursive && !force {
        return Err(anyhow::anyhow!("-r requires -f flag"));
    }

    let (mut config, mut manifest) = load_state()?;
    let project_data = find_project(&manifest, &project)?.clone();
    let path = project_path(&config, &manifest, &project_data)?;

    // Linked projects: pm owns only the symlink pointer, never the external
    // target. Every removal mode removes the symlink node (if present) and the
    // manifest entry; the linked directory is never deleted.
    if project_data.is_linked() {
        return remove_linked(
            &mut config,
            &mut manifest,
            &project,
            &project_data,
            &path,
            yes,
        );
    }

    if !confirm_removal(&project, yes, force, recursive, &path)? {
        return Ok(());
    }

    if recursive {
        if !force {
            return Err(anyhow::anyhow!("-r requires -f flag"));
        }

        record_project_event(&project_data, &path, HistoryAction::Deleted)?;
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        manifest
            .projects
            .retain(|candidate| candidate.name != project);
        clear_current_project(&mut config, &project);
        save_state(&config, &manifest)?;
        println!("{} Deleted '{}' and its files", "✓".green(), project.cyan());
    } else if force {
        record_project_event(&project_data, &path, HistoryAction::Trashed)?;
        if let Some(project_data) = manifest
            .projects
            .iter_mut()
            .find(|candidate| candidate.name == project)
        {
            project_data.workspace = ".trash".to_string();
        }
        clear_current_project(&mut config, &project);
        save_state(&config, &manifest)?;
        println!("{} Moved '{}' to trash", "✓".green(), project.cyan());
    } else {
        record_project_event(&project_data, &path, HistoryAction::Unregistered)?;
        manifest
            .projects
            .retain(|candidate| candidate.name != project);
        clear_current_project(&mut config, &project);
        save_state(&config, &manifest)?;
        println!(
            "{} Unregistered '{}' (files kept at {})",
            "✓".green(),
            project.cyan(),
            path.display()
        );
    }

    Ok(())
}

fn confirm_removal(
    project: &str,
    yes: bool,
    force: bool,
    recursive: bool,
    path: &std::path::Path,
) -> Result<bool> {
    if recursive {
        println!("{} This will permanently delete:", "⚠".yellow());
        println!("  {}", path.display());

        if path.exists() {
            let file_count = walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_file())
                .count();
            println!("  ({} files)", file_count);
        }
    } else if force {
        println!(
            "{} '{}' will be moved to trash",
            "⚠".yellow(),
            project.cyan()
        );
        println!("  {}", path.display());
    } else {
        println!(
            "{} '{}' will be unregistered (files kept)",
            "⚠".yellow(),
            project.cyan()
        );
        println!("  {}", path.display());
    }

    if !yes {
        print!("\nType 'y' to continue: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim() != "y" {
            println!("Aborted.");
            return Ok(false);
        }
    }

    Ok(true)
}

/// Remove a linked project. Deletes only the symlink node at the convention
/// path (never the external target) and the manifest entry, regardless of the
/// `-f`/`-r` flags. A `remove_dir_all` is deliberately never used here.
fn remove_linked(
    config: &mut crate::models::Config,
    manifest: &mut crate::models::Manifest,
    project: &str,
    project_data: &crate::models::Project,
    path: &std::path::Path,
    yes: bool,
) -> Result<()> {
    let target = project_data.link.as_deref().unwrap_or("<unknown>");
    println!(
        "{} '{}' is a link → {}",
        "⚠".yellow(),
        project.cyan(),
        target
    );
    println!("  The symlink is removed; the linked directory is left untouched.");

    if !yes {
        print!("\nType 'y' to continue: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim() != "y" {
            println!("Aborted.");
            return Ok(());
        }
    }

    record_project_event(project_data, path, HistoryAction::Unregistered)?;

    // Remove ONLY the symlink node. `symlink_metadata` does not follow the
    // link, so this never reaches into the target. If the path is a real
    // directory instead (link-replaced-by-dir drift), refuse to delete it.
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(path)?,
        Ok(_) => println!(
            "{} {} is not a symlink (link drift); leaving it on disk",
            "⚠".yellow(),
            path.display()
        ),
        Err(_) => { /* nothing on disk — just unregister */ }
    }

    manifest
        .projects
        .retain(|candidate| candidate.name != project);
    clear_current_project(config, project);
    save_state(config, manifest)?;
    println!(
        "{} Unlinked '{}' (target preserved at {})",
        "✓".green(),
        project.cyan(),
        target
    );
    Ok(())
}

fn clear_current_project(config: &mut crate::models::Config, project: &str) {
    if config.current_project.as_deref() == Some(project) {
        config.current_project = None;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::config::load_manifest;
    use crate::test_support::{init_git_repo, TestEnv};

    /// Link an external git repo as project `widgets` in `default`; return the
    /// external target path.
    fn link_widgets(env: &TestEnv) -> std::path::PathBuf {
        let target = env.tempdir_path().join("external").join("widgets");
        init_git_repo(&target);
        crate::commands::link::run(
            target.to_string_lossy().into_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("link should succeed");
        target
    }

    #[test]
    fn rm_link_removes_symlink_and_preserves_target() {
        let env = TestEnv::new();
        let target = link_widgets(&env);
        let convention = env.project_dir("default", "widgets");
        assert!(std::fs::symlink_metadata(&convention)
            .unwrap()
            .file_type()
            .is_symlink());

        run("widgets".to_string(), true, false, false).expect("rm should succeed");

        assert!(
            std::fs::symlink_metadata(&convention).is_err(),
            "symlink should be removed"
        );
        assert!(
            target.join(".git").exists(),
            "external target must survive plain rm"
        );
        let manifest = load_manifest().unwrap();
        assert!(!manifest.projects.iter().any(|p| p.name == "widgets"));
    }

    #[test]
    fn rm_recursive_link_still_preserves_target() {
        let env = TestEnv::new();
        let target = link_widgets(&env);
        let convention = env.project_dir("default", "widgets");

        // `-r -f` must NOT delete through the link into the target.
        run("widgets".to_string(), true, true, true).expect("rm -rf should succeed");

        assert!(
            std::fs::symlink_metadata(&convention).is_err(),
            "symlink should be removed"
        );
        assert!(
            target.join(".git").exists(),
            "external target must survive recursive rm of a link"
        );
    }
}
