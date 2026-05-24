use crate::error::PmError;
use crate::git::{clone_repo, get_remote_url, is_git_repo, repo_slug_from_remote};
use crate::models::Project;
use crate::path::{expand_path, normalize_path, validate_project_dir};
use crate::state::{find_workspace, load_state, save_state, workspace_dir};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::path::PathBuf;

/// Add a project to the current workspace under the strict path convention.
///
/// The destination is always `<base_root>/<workspace>/<dir>`. Two input modes:
/// - **git URL** → clone the repository into the destination.
/// - **local path** → move the directory into the destination.
///
/// External paths are no longer registered in-place; they are materialized at
/// the convention path. If the destination already exists the command aborts
/// without touching the manifest. On move failure the manifest is left
/// unchanged.
pub fn run(
    source: String,
    name: Option<String>,
    tags: Option<String>,
    note: Option<String>,
    force: bool,
) -> Result<()> {
    let (mut config, mut manifest) = load_state()?;
    let current_workspace = config.current_workspace.clone();
    let _ = find_workspace(&manifest, &current_workspace)?;

    let is_url = looks_like_git_url(&source);

    // Derive project name.
    let project_name = if let Some(name) = name {
        name
    } else if is_url {
        repo_slug_from_remote(&source)
            .ok_or_else(|| anyhow::anyhow!("Could not derive project name from URL '{}'", source))?
    } else {
        let expanded = expand_path(&source);
        expanded
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not determine project name from path"))?
    };

    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&project_name) {
        return Err(PmError::InvalidProjectName(project_name).into());
    }

    // Under the convention, the on-disk directory name equals the project
    // name. The project-name regex is stricter than the dir validator, so
    // this should always pass; validate explicitly for defense in depth.
    let dir = project_name.clone();
    validate_project_dir(&dir)?;

    let destination = workspace_dir(&config.base_root, &current_workspace).join(&dir);

    // Detect a no-op `pm add .` where the source is already at the convention
    // path (used to register an in-place clone after `git clone` directly into
    // base_root).
    let in_place_source: Option<PathBuf> = if is_url {
        None
    } else {
        let resolved = resolve_local_source(&source)?;
        Some(resolved)
    };
    let in_place = in_place_source.as_deref() == Some(destination.as_path());

    if destination.exists() && !in_place {
        return Err(anyhow::anyhow!(
            "Destination '{}' already exists; refusing to add '{}'",
            destination.display(),
            project_name
        ));
    }

    // Check manifest collisions.
    let name_collision = manifest.projects.iter().any(|p| p.name == project_name);
    let dir_collision = manifest
        .projects
        .iter()
        .any(|p| p.workspace == current_workspace && p.dir == dir);

    if (name_collision || dir_collision) && !force {
        if name_collision {
            return Err(PmError::ProjectExists(project_name).into());
        }
        return Err(anyhow::anyhow!(
            "Directory '{}' is already registered in workspace '{}'. Use --force to overwrite the manifest entry.",
            dir,
            current_workspace
        ));
    }

    // Materialize the working copy at the convention path.
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let remote: Option<String>;
    let materialization;
    if is_url {
        clone_repo(&source, &destination)?;
        remote = Some(source.clone());
        materialization = format!("Cloned {} → {}", source, destination.display());
    } else {
        let source_path = in_place_source.expect("resolved above for non-URL inputs");
        if !source_path.is_dir() {
            return Err(PmError::NotADirectory(source.clone()).into());
        }
        let was_git = is_git_repo(&source_path.display().to_string());
        if !was_git {
            println!(
                "{} Source is not a git repository; adding anyway",
                "⚠".yellow()
            );
        }
        remote = if was_git {
            get_remote_url(&source_path.display().to_string())
        } else {
            None
        };

        if in_place {
            materialization = format!("Registered in place at {}", destination.display());
        } else {
            fs::rename(&source_path, &destination).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to move '{}' to '{}': {}. Manifest left unchanged.",
                    source_path.display(),
                    destination.display(),
                    e
                )
            })?;
            materialization = format!(
                "Moved {} → {}",
                source_path.display(),
                destination.display()
            );
        }
    }

    let repo_slug = remote
        .as_deref()
        .and_then(repo_slug_from_remote)
        .unwrap_or_else(|| project_name.clone());

    if force {
        manifest.projects.retain(|p| {
            !(p.name == project_name || (p.workspace == current_workspace && p.dir == dir))
        });
    }

    let mut project = Project::new(
        project_name.clone(),
        current_workspace.clone(),
        repo_slug,
        dir,
    );
    project.remote = remote;
    project.note = note;
    if let Some(tags_str) = tags {
        project.tags = tags_str
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }

    manifest.projects.push(project);
    config.current_project = Some(project_name.clone());
    save_state(&config, &manifest)?;

    println!("{} {}", "✓".green(), materialization);
    println!(
        "{} Added '{}' to workspace '{}'",
        "✓".green(),
        project_name.cyan(),
        current_workspace.cyan()
    );

    Ok(())
}

/// Heuristic: treat the argument as a git URL when it uses a known scheme,
/// looks like an scp-style git URL, or ends in `.git`.
fn looks_like_git_url(s: &str) -> bool {
    if s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("ssh://")
        || s.starts_with("git://")
        || s.starts_with("git@")
        || s.ends_with(".git")
    {
        return true;
    }
    false
}

fn resolve_local_source(source: &str) -> Result<PathBuf> {
    let normalized = normalize_path(source)
        .map_err(|e| anyhow::anyhow!("Source path '{}' could not be resolved: {}", source, e))?;
    let expanded = expand_path(&normalized);
    if !expanded.exists() {
        return Err(PmError::PathNotFound(source.to_string()).into());
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_manifest;
    use crate::test_support::{init_git_repo, TestEnv};

    #[test]
    fn add_moves_local_repo_into_convention_path() {
        let env = TestEnv::new();
        let src = env.tempdir_path().join("external").join("myapp");
        init_git_repo(&src);

        run(src.to_string_lossy().into_owned(), None, None, None, false)
            .expect("add should succeed");

        let dest = env.project_dir("default", "myapp");
        assert!(
            dest.exists(),
            "destination should exist at {}",
            dest.display()
        );
        assert!(!src.exists(), "source should have been moved away");

        let manifest = load_manifest().expect("load manifest");
        let project = manifest
            .projects
            .iter()
            .find(|p| p.name == "myapp")
            .expect("manifest should have 'myapp'");
        assert_eq!(project.workspace, "default");
        assert_eq!(project.dir, "myapp");
    }

    #[test]
    fn add_clones_url_into_convention_path() {
        let env = TestEnv::new();
        // A non-bare repo at a `.git`-suffixed path is treated as a URL by
        // the heuristic and cloned with `git clone <path>`.
        let origin = env.tempdir_path().join("origin.git");
        init_git_repo(&origin);

        let source = origin.to_string_lossy().into_owned();
        run(source, Some("origin".to_string()), None, None, false)
            .expect("clone-into-place should succeed");

        let dest = env.project_dir("default", "origin");
        assert!(dest.join(".git").exists(), "cloned repo should have .git");

        let manifest = load_manifest().expect("load manifest");
        assert!(manifest.projects.iter().any(|p| p.name == "origin"));
    }

    #[test]
    fn add_aborts_when_destination_exists() {
        let env = TestEnv::new();
        let src = env.tempdir_path().join("external").join("myapp");
        init_git_repo(&src);

        // Pre-create the destination so `pm add` must refuse.
        std::fs::create_dir_all(env.project_dir("default", "myapp")).unwrap();

        let err = run(src.to_string_lossy().into_owned(), None, None, None, false).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "error should mention destination exists: {err}"
        );

        // Source remains, manifest unchanged.
        assert!(src.exists(), "source must remain on disk after abort");
        let manifest = load_manifest().expect("load manifest");
        assert!(
            !manifest.projects.iter().any(|p| p.name == "myapp"),
            "manifest must be untouched on abort"
        );
    }
}
