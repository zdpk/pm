use crate::error::PmError;
use crate::git::{get_remote_url, is_git_repo, repo_slug_from_remote};
use crate::models::Project;
use crate::path::{expand_path, validate_project_dir};
use crate::state::{find_workspace, load_state, save_state, workspace_dir};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;

/// Link an existing external git repository into the convention path.
///
/// Creates a symlink at `<base_root>/<workspace>/<dir>` pointing at the
/// canonicalized `<path>` and registers the project in the manifest with a
/// `link` target. The external repository is never moved or copied — pm
/// manages only the symlink and the manifest entry. Symlink creation is
/// Unix-only; on other platforms the command fails before touching state.
pub fn run(
    path: String,
    name: Option<String>,
    workspace: Option<String>,
    tags: Option<String>,
    note: Option<String>,
) -> Result<()> {
    let (mut config, mut manifest) = load_state()?;

    let workspace_name = workspace.unwrap_or_else(|| config.current_workspace.clone());
    let _ = find_workspace(&manifest, &workspace_name)?;

    // Resolve the target to an absolute canonical path. It must exist and be a
    // git repository. We canonicalize (rather than collapse to `~`) so the
    // stored target matches what `scan_base_root` observes when resolving the
    // symlink — both go through `fs::canonicalize`.
    let expanded = expand_path(&path);
    if !expanded.exists() {
        return Err(PmError::PathNotFound(path).into());
    }
    let target = std::fs::canonicalize(&expanded)
        .map_err(|e| anyhow::anyhow!("Could not resolve '{}': {}", expanded.display(), e))?;
    let target_str = target.display().to_string();
    if !is_git_repo(&target_str) {
        return Err(PmError::NotAGitRepo(target_str).into());
    }

    // Derive name + dir.
    let project_name = match name {
        Some(n) => n,
        None => target
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not determine project name from '{}'",
                    target.display()
                )
            })?,
    };
    let name_regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$")?;
    if !name_regex.is_match(&project_name) {
        return Err(PmError::InvalidProjectName(project_name).into());
    }
    let dir = project_name.clone();
    validate_project_dir(&dir)?;

    // Name collision (project names are global across workspaces).
    if manifest.projects.iter().any(|p| p.name == project_name) {
        return Err(PmError::ProjectExists(project_name).into());
    }

    // Abort if anything (real dir, file, or existing symlink) occupies the
    // convention path. `symlink_metadata` does not follow symlinks, so a
    // dangling symlink also counts as occupied.
    let convention = workspace_dir(&config.base_root, &workspace_name).join(&dir);
    if convention.symlink_metadata().is_ok() {
        return Err(PmError::LinkPathOccupied(convention.display().to_string()).into());
    }

    // Create the symlink (Unix-only). Failing here leaves the manifest
    // untouched because we persist state only afterwards.
    crate::path::create_symlink(&target, &convention)?;

    let remote = get_remote_url(&target_str);
    let repo_slug = remote
        .as_deref()
        .and_then(repo_slug_from_remote)
        .unwrap_or_else(|| project_name.clone());

    let mut project = Project::new(project_name.clone(), workspace_name.clone(), repo_slug, dir);
    project.remote = remote;
    project.note = note;
    project.link = Some(target_str);
    if let Some(tags_str) = tags {
        project.tags = tags_str
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
    }

    manifest.projects.push(project);
    config.current_project = Some(project_name.clone());
    save_state(&config, &manifest)?;

    println!(
        "{} Linked {} → {}",
        "✓".green(),
        convention.display(),
        target.display()
    );
    println!(
        "{} Added '{}' to workspace '{}'",
        "✓".green(),
        project_name.cyan(),
        workspace_name.cyan()
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::config::load_manifest;
    use crate::state::{load_state, save_state};
    use crate::test_support::{init_git_repo, TestEnv};

    fn external_repo(env: &TestEnv, name: &str) -> std::path::PathBuf {
        let target = env.tempdir_path().join("external").join(name);
        init_git_repo(&target);
        target
    }

    #[test]
    fn link_creates_symlink_and_registers_without_moving_target() {
        let env = TestEnv::new();
        let target = external_repo(&env, "widgets");

        run(
            target.to_string_lossy().into_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("link should succeed");

        let convention = env.project_dir("default", "widgets");
        let meta = std::fs::symlink_metadata(&convention).expect("convention path should exist");
        assert!(
            meta.file_type().is_symlink(),
            "convention path must be a symlink"
        );
        assert!(
            target.join(".git").exists(),
            "external target must remain in place"
        );

        let manifest = load_manifest().unwrap();
        let p = manifest
            .projects
            .iter()
            .find(|p| p.name == "widgets")
            .expect("project should be registered");
        assert_eq!(p.workspace, "default");
        let canonical = std::fs::canonicalize(&target)
            .unwrap()
            .display()
            .to_string();
        assert_eq!(
            p.link.as_deref(),
            Some(canonical.as_str()),
            "link target must be stored canonically"
        );
    }

    #[test]
    fn link_honors_name_and_workspace_flags() {
        let env = TestEnv::new();
        // Declare a second workspace.
        {
            let (config, mut manifest) = load_state().unwrap();
            manifest
                .workspaces
                .push(crate::models::Workspace::new("labs".to_string()));
            save_state(&config, &manifest).unwrap();
        }
        let target = external_repo(&env, "widgets");

        run(
            target.to_string_lossy().into_owned(),
            Some("gizmo".to_string()),
            Some("labs".to_string()),
            None,
            None,
        )
        .expect("link should succeed");

        assert!(std::fs::symlink_metadata(env.project_dir("labs", "gizmo")).is_ok());
        let manifest = load_manifest().unwrap();
        let p = manifest
            .projects
            .iter()
            .find(|p| p.name == "gizmo")
            .unwrap();
        assert_eq!(p.workspace, "labs");
    }

    #[test]
    fn link_rejects_non_git_target() {
        let env = TestEnv::new();
        let target = env.tempdir_path().join("external").join("notes");
        std::fs::create_dir_all(&target).unwrap();

        let err = run(
            target.to_string_lossy().into_owned(),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("not a git repository"),
            "got: {err}"
        );
        assert!(load_manifest().unwrap().projects.is_empty());
    }

    #[test]
    fn link_rejects_missing_target() {
        let env = TestEnv::new();
        let ghost = env.tempdir_path().join("ghost");
        let err = run(ghost.to_string_lossy().into_owned(), None, None, None, None).unwrap_err();
        assert!(err.to_string().contains("does not exist"), "got: {err}");
    }

    #[test]
    fn link_aborts_when_convention_path_occupied() {
        let env = TestEnv::new();
        let target = external_repo(&env, "widgets");
        let convention = env.project_dir("default", "widgets");
        std::fs::create_dir_all(&convention).unwrap();

        let err = run(
            target.to_string_lossy().into_owned(),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("occupied"), "got: {err}");
        // The pre-existing real dir is untouched (not replaced by a symlink),
        // and nothing was registered.
        assert!(!std::fs::symlink_metadata(&convention)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!load_manifest()
            .unwrap()
            .projects
            .iter()
            .any(|p| p.name == "widgets"));
    }

    #[test]
    fn link_aborts_on_name_collision() {
        let env = TestEnv::new();
        let target = external_repo(&env, "widgets");
        {
            let (config, mut manifest) = load_state().unwrap();
            manifest.projects.push(crate::models::Project::new(
                "widgets".to_string(),
                "default".to_string(),
                "widgets".to_string(),
                "widgets".to_string(),
            ));
            save_state(&config, &manifest).unwrap();
        }

        let err = run(
            target.to_string_lossy().into_owned(),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("already exists"), "got: {err}");
    }
}
