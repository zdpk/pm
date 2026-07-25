use crate::error::PmError;
use crate::git::{clone_repo, get_remote_url, is_git_repo, repo_slug_from_remote};
use crate::models::Project;
use crate::path::{expand_path, normalize_path, validate_project_dir};
use crate::state::{find_workspace, load_state, save_state, workspace_dir};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Add a project to the current workspace under the strict path convention.
///
/// The destination is always `<base_root>/<workspace>/<dir>`. Two input modes:
/// - **git URL** → clone the repository into the destination.
/// - **local path** → move an existing directory into the destination, or
///   create the destination directly when the source is missing.
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

    if destination_is_occupied(&destination)? && !in_place {
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
        let source_exists = local_source_exists(&source_path, &source)?;

        if !source_exists {
            println!(
                "{} Source directory does not exist; creating an empty project directory",
                "⚠".yellow()
            );
            fs::create_dir(&destination).map_err(|error| {
                anyhow::anyhow!(
                    "Failed to create project directory '{}': {}. Manifest left unchanged.",
                    destination.display(),
                    error
                )
            })?;
            remote = None;
            materialization = format!("Created and registered {}", destination.display());
        } else {
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
    let expanded = expand_path(source);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Source path '{}' could not be resolved from the current directory: {}",
                    source,
                    e
                )
            })?
            .join(expanded)
    };

    if absolute.exists() {
        let normalized = normalize_path(source).map_err(|e| {
            anyhow::anyhow!("Source path '{}' could not be resolved: {}", source, e)
        })?;
        Ok(expand_path(&normalized))
    } else {
        Ok(absolute)
    }
}

fn destination_is_occupied(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(anyhow::anyhow!(
            "Failed to inspect destination '{}': {}",
            path.display(),
            error
        )),
    }
}

fn local_source_exists(path: &Path, source: &str) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(PmError::NotADirectory(source.to_string()).into()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(anyhow::anyhow!(
            "Failed to inspect source path '{}': {}",
            path.display(),
            error
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_manifest;
    use crate::test_support::{TestEnv, init_git_repo};

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
    fn add_creates_and_registers_missing_directory() {
        let env = TestEnv::new();
        let dir = env.project_dir("default", "newapp");
        assert!(!dir.exists(), "test project directory should start missing");

        run(dir.to_string_lossy().into_owned(), None, None, None, false)
            .expect("add should create and register a missing directory");

        assert!(dir.is_dir(), "missing project directory should be created");

        let manifest = load_manifest().expect("load manifest");
        let project = manifest
            .projects
            .iter()
            .find(|p| p.name == "newapp")
            .expect("manifest should contain 'newapp'");
        assert_eq!(project.workspace, "default");
        assert_eq!(project.dir, "newapp");
        assert_eq!(project.remote, None);
    }

    #[test]
    fn add_missing_external_path_creates_destination_directly() {
        let env = TestEnv::new();
        let src = env.tempdir_path().join("external").join("newapp");
        let dest = env.project_dir("default", "newapp");
        assert!(!src.exists());
        assert!(!dest.exists());

        run(src.to_string_lossy().into_owned(), None, None, None, false)
            .expect("add should create the managed project directory");

        assert!(
            !src.exists(),
            "a missing external source should not be created just to move it"
        );
        assert!(dest.is_dir(), "managed destination should be created");
        let manifest = load_manifest().expect("load manifest");
        assert!(manifest.projects.iter().any(|p| p.name == "newapp"));
    }

    #[test]
    fn add_does_not_create_missing_source_when_destination_exists() {
        let env = TestEnv::new();
        let src = env.tempdir_path().join("external").join("myapp");
        let dest = env.project_dir("default", "myapp");
        std::fs::create_dir_all(&dest).unwrap();

        let err = run(src.to_string_lossy().into_owned(), None, None, None, false).unwrap_err();

        assert!(err.to_string().contains("already exists"));
        assert!(
            !src.exists(),
            "source directory should not be created when add aborts"
        );
        let manifest = load_manifest().expect("load manifest");
        assert!(!manifest.projects.iter().any(|p| p.name == "myapp"));
    }

    #[cfg(unix)]
    #[test]
    fn add_aborts_when_destination_is_dangling_symlink() {
        let env = TestEnv::new();
        let src = env.tempdir_path().join("external").join("myapp");
        init_git_repo(&src);

        let dest = env.project_dir("default", "myapp");
        let missing_target = env.tempdir_path().join("missing-target");
        std::os::unix::fs::symlink(&missing_target, &dest).unwrap();

        let err = run(src.to_string_lossy().into_owned(), None, None, None, false).unwrap_err();

        assert!(err.to_string().contains("already exists"));
        assert!(src.exists(), "source must not be moved");
        assert!(
            std::fs::symlink_metadata(&dest)
                .unwrap()
                .file_type()
                .is_symlink(),
            "destination symlink must remain untouched"
        );
        let manifest = load_manifest().expect("load manifest");
        assert!(!manifest.projects.iter().any(|p| p.name == "myapp"));
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
