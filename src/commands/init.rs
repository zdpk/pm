use crate::config::{
    config_dir, config_path, history_path, is_initialized, load_config, manifest_path, ports_path,
    repo_specs_dir,
};
use crate::error::PmError;
use crate::models::{Config, HistoryData, Manifest, PortsData, RepoSpec};
use crate::path::expand_path;
use crate::state::workspace_dir;
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

const FALLBACK_BASE_ROOT: &str = "~/.ws";

pub fn run(force: bool) -> Result<()> {
    let initialized = is_initialized();

    if initialized && !force {
        ensure_repo_specs_dir()?;
        ensure_ports_file()?;
        return Err(PmError::AlreadyInitialized.into());
    }

    let owned_root = if initialized {
        load_config()
            .ok()
            .map(|config| expand_path(&config.base_root))
    } else {
        None
    };

    if initialized && force {
        print!(
            "{} Existing configuration will be overwritten.\nContinue? [y/N] ",
            "!".yellow()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let mut config = Config::default();
    let manifest = Manifest::default();
    let default_root = expand_path(&config.base_root);
    let fallback_root = expand_path(FALLBACK_BASE_ROOT);
    let selected = {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut input = stdin.lock();
        let mut output = stdout.lock();
        select_base_root(
            &mut config,
            &default_root,
            &fallback_root,
            owned_root.as_deref(),
            &mut input,
            &mut output,
        )?
    };
    if !selected {
        println!("Aborted.");
        return Ok(());
    }

    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    println!("{} Created {}", "✓".green(), dir.display());

    ensure_workspace_directories(&config, &manifest)?;

    fs::write(config_path(), serde_json::to_string_pretty(&config)?)?;
    println!("{} Created config.json", "✓".green());

    fs::write(manifest_path(), serde_json::to_string_pretty(&manifest)?)?;
    println!("{} Created manifest.json", "✓".green());

    fs::write(
        history_path(),
        serde_json::to_string_pretty(&HistoryData::default())?,
    )?;
    println!("{} Created history.json", "✓".green());

    ensure_repo_specs_dir()?;
    ensure_ports_file()?;

    println!("\n{}", "PM initialized successfully!".green().bold());
    Ok(())
}

fn select_base_root<R: BufRead, W: Write>(
    config: &mut Config,
    default_root: &Path,
    fallback_root: &Path,
    owned_root: Option<&Path>,
    input: &mut R,
    output: &mut W,
) -> Result<bool> {
    let default_is_owned = owned_root.is_some_and(|owned| owned == default_root);
    if !path_is_occupied(default_root) || default_is_owned {
        return Ok(true);
    }

    writeln!(
        output,
        "{} Default base root '{}' already exists; the path is already in use.",
        "!".yellow(),
        config.base_root
    )?;
    write!(
        output,
        "Change base_root to '{}' and create it instead? [y/N] ",
        FALLBACK_BASE_ROOT
    )?;
    output.flush()?;

    let mut answer = String::new();
    input.read_line(&mut answer)?;
    if !answer.trim().eq_ignore_ascii_case("y") {
        return Ok(false);
    }

    let fallback_is_owned = owned_root.is_some_and(|owned| owned == fallback_root);
    if path_is_occupied(fallback_root) && !fallback_is_owned {
        return Err(anyhow::anyhow!(
            "Fallback base root '{}' also already exists; choose a different base_root",
            FALLBACK_BASE_ROOT
        ));
    }

    config.base_root = FALLBACK_BASE_ROOT.to_string();
    Ok(true)
}

fn path_is_occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn ensure_workspace_directories(config: &Config, manifest: &Manifest) -> Result<()> {
    let root = expand_path(&config.base_root);
    ensure_directory(&root, "base root")?;

    for workspace in &manifest.workspaces {
        let path = workspace_dir(&config.base_root, &workspace.name);
        ensure_directory(&path, &format!("workspace '{}'", workspace.name))?;
    }

    Ok(())
}

fn ensure_directory(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) if path.is_dir() => Ok(()),
        Ok(_) => Err(anyhow::anyhow!(
            "{} path '{}' is occupied by a non-directory",
            label,
            path.display()
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            println!("{} Created {} at {}", "✓".green(), label, path.display());
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

fn ensure_repo_specs_dir() -> Result<()> {
    let specs_dir = repo_specs_dir();
    fs::create_dir_all(&specs_dir)?;

    let default_spec_path = specs_dir.join("rust-axum-sqlx-backend.json");
    if !default_spec_path.exists() {
        let spec = RepoSpec {
            id: "rust-axum-sqlx-backend".to_string(),
            version: "0.1.0".to_string(),
            name: "Rust Axum SQLx Backend".to_string(),
            description: "Rust + Axum + SQLx + PostgreSQL backend template standard".to_string(),
            source: "BACKEND_TEMPLATE_STANDARD.md".to_string(),
        };
        fs::write(default_spec_path, serde_json::to_string_pretty(&spec)?)?;
        println!(
            "{} Created repo-specs/rust-axum-sqlx-backend.json",
            "✓".green()
        );
    }

    Ok(())
}

fn ensure_ports_file() -> Result<()> {
    let path = ports_path();
    if !path.exists() {
        let content = serde_json::to_string_pretty(&PortsData::default())?;
        fs::write(path, content)?;
        println!("{} Created ports.json", "✓".green());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn default_base_root_is_ws() {
        assert_eq!(Config::default().base_root, "~/ws");
    }

    #[test]
    fn selects_default_root_when_it_is_available() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("ws");
        let fallback = temp.path().join(".ws");
        let mut config = Config::default();
        let mut input = Cursor::new(Vec::<u8>::new());
        let mut output = Vec::new();

        let selected = select_base_root(
            &mut config,
            &default,
            &fallback,
            None,
            &mut input,
            &mut output,
        )
        .unwrap();

        assert!(selected);
        assert_eq!(config.base_root, "~/ws");
        assert!(output.is_empty());
    }

    #[test]
    fn existing_default_root_can_switch_to_hidden_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("ws");
        let fallback = temp.path().join(".ws");
        fs::create_dir_all(&default).unwrap();
        let mut config = Config::default();
        let mut input = Cursor::new(b"y\n");
        let mut output = Vec::new();

        let selected = select_base_root(
            &mut config,
            &default,
            &fallback,
            None,
            &mut input,
            &mut output,
        )
        .unwrap();

        assert!(selected);
        assert_eq!(config.base_root, "~/.ws");
        let message = String::from_utf8(output).unwrap();
        assert!(message.contains("already exists"));
        assert!(message.contains("~/.ws"));
    }

    #[test]
    fn existing_root_owned_by_current_config_is_reused() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("ws");
        let fallback = temp.path().join(".ws");
        fs::create_dir_all(&default).unwrap();
        let mut config = Config::default();
        let mut input = Cursor::new(Vec::<u8>::new());
        let mut output = Vec::new();

        let selected = select_base_root(
            &mut config,
            &default,
            &fallback,
            Some(&default),
            &mut input,
            &mut output,
        )
        .unwrap();

        assert!(selected);
        assert_eq!(config.base_root, "~/ws");
        assert!(output.is_empty());
    }

    #[test]
    fn rejecting_hidden_fallback_aborts_selection() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("ws");
        let fallback = temp.path().join(".ws");
        fs::create_dir_all(&default).unwrap();
        let mut config = Config::default();
        let mut input = Cursor::new(b"n\n");
        let mut output = Vec::new();

        let selected = select_base_root(
            &mut config,
            &default,
            &fallback,
            None,
            &mut input,
            &mut output,
        )
        .unwrap();

        assert!(!selected);
        assert_eq!(config.base_root, "~/ws");
    }

    #[test]
    fn occupied_hidden_fallback_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("ws");
        let fallback = temp.path().join(".ws");
        fs::create_dir_all(&default).unwrap();
        fs::create_dir_all(&fallback).unwrap();
        let mut config = Config::default();
        let mut input = Cursor::new(b"y\n");
        let mut output = Vec::new();

        let error = select_base_root(
            &mut config,
            &default,
            &fallback,
            None,
            &mut input,
            &mut output,
        )
        .unwrap_err();

        assert!(error.to_string().contains("also already exists"));
        assert_eq!(config.base_root, "~/ws");
    }

    #[test]
    fn materializes_base_root_and_default_workspaces() {
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            base_root: temp.path().join("ws").to_string_lossy().into_owned(),
            ..Config::default()
        };
        let manifest = Manifest::default();

        ensure_workspace_directories(&config, &manifest).unwrap();

        let root = expand_path(&config.base_root);
        assert!(root.is_dir());
        assert!(root.join("default").is_dir());
        assert!(root.join(".trash").is_dir());
    }

    #[test]
    fn refuses_non_directory_workspace_collision() {
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            base_root: temp.path().join("ws").to_string_lossy().into_owned(),
            ..Config::default()
        };
        let root = expand_path(&config.base_root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("default"), "occupied").unwrap();

        let error = ensure_workspace_directories(&config, &Manifest::default()).unwrap_err();

        assert!(error.to_string().contains("occupied by a non-directory"));
    }
}
