use crate::cli::ProjectCommand;
use crate::error::PmError;
use crate::models::ProjMeta;
use crate::project::{self as proj, FileStrategy, ProjConfig};
use crate::restore;
use crate::state::{self, find_project_mut};
use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};

pub fn run(cmd: ProjectCommand) -> Result<()> {
    match cmd {
        ProjectCommand::Update => cmd_update(),
        ProjectCommand::Init {
            language,
            framework,
            ci,
            docker,
            hooks,
            all,
            yes,
            no_services,
        } => cmd_init(
            language,
            framework,
            ci,
            docker,
            hooks,
            all,
            yes,
            no_services,
        ),
        ProjectCommand::Add {
            language,
            framework,
        } => cmd_add(language, framework),
        ProjectCommand::Sync { all, dry_run } => cmd_sync(all, dry_run),
        ProjectCommand::Check { all } => cmd_check(all),
        ProjectCommand::Diff => cmd_diff(),
        ProjectCommand::List => cmd_list(),
        ProjectCommand::Gitignore { diff, categories } => cmd_gitignore(diff, categories),
    }
}

fn ensure_repo() -> Result<PathBuf> {
    let (config, _) = state::load_state()?;

    // If custom config_repo is set, clone/use that
    if let Some(settings) = &config.config_repo {
        return proj::ensure_config_repo(&settings.url, &settings.cache_dir);
    }

    // Otherwise use bundled configs/ directory relative to the binary
    let bundled = bundled_configs_path();
    if bundled.join("manifest.yaml").exists() {
        return Ok(bundled);
    }

    Err(PmError::ConfigRepoNotConfigured.into())
}

fn bundled_configs_path() -> PathBuf {
    // Walk up from the binary location to find configs/
    if let Ok(exe) = std::env::current_exe() {
        // dev: binary is in target/debug/, configs is at repo root
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..5 {
            if let Some(ref d) = dir {
                let candidate = d.join("configs");
                if candidate.join("manifest.yaml").exists() {
                    return candidate;
                }
                dir = d.parent().map(|p| p.to_path_buf());
            }
        }
    }
    // Fallback: check relative to cwd
    PathBuf::from("configs")
}

fn stack_label(language: &str, framework: Option<&str>) -> String {
    format!("{}/{}", language, framework.unwrap_or("common"))
}

// ── pm project update ──

fn cmd_update() -> Result<()> {
    let (config, _) = state::load_state()?;
    let Some(settings) = &config.config_repo else {
        println!("Using bundled configs — no remote config repo configured.");
        println!("Set config_repo.url in config.json to use a remote repo.");
        return Ok(());
    };
    let repo_path = proj::update_config_repo(&settings.url, &settings.cache_dir)?;
    let head = proj::config_repo_head(&repo_path)?;
    println!("{} Config repo updated ({})", "✓".green(), head);
    Ok(())
}

// ── pm project init ──

#[allow(clippy::too_many_arguments)] // each flag corresponds to a CLI flag
fn cmd_init(
    language: Option<String>,
    framework: Option<String>,
    ci: bool,
    docker: bool,
    hooks: bool,
    all: bool,
    yes: bool,
    no_services: bool,
) -> Result<()> {
    let repo_path = ensure_repo()?;
    let cwd = std::env::current_dir()?;
    let global_manifest = proj::load_global_manifest(&repo_path)?;

    let lang = if let Some(l) = language {
        validate_language(&l, &global_manifest)?;
        l
    } else if yes {
        return Err(anyhow::anyhow!(
            "Language is required in non-interactive mode. Use --language / -l."
        ));
    } else {
        resolve_language_interactive(&cwd, &global_manifest)?
    };

    let fw = if let Some(f) = framework {
        validate_framework(&f, &lang, &global_manifest)?;
        Some(f)
    } else if yes {
        proj::detect_framework(&cwd, &lang)
    } else {
        resolve_framework_interactive(&cwd, &lang, &global_manifest)?
    };

    let includes = if all {
        vec!["ci".to_string(), "docker".to_string(), "hooks".to_string()]
    } else if ci || docker || hooks {
        let mut inc = Vec::new();
        if ci {
            inc.push("ci".to_string());
        }
        if docker {
            inc.push("docker".to_string());
        }
        if hooks {
            inc.push("hooks".to_string());
        }
        inc
    } else if !yes {
        resolve_includes_interactive()?
    } else {
        Vec::new()
    };

    // pnpm + Turbopack convention: warn if a competing lockfile is present
    // when initializing a Next.js project. We do not auto-delete; we surface
    // the issue and let the user clean up.
    if fw.as_deref() == Some("nextjs") {
        warn_on_competing_lockfiles(&cwd);
    }

    // Auto-add a default `services:` entry so `pm run` works out of the box.
    // Disable with --no-services. Currently we register a single service
    // named after the framework's primary kind.
    let services = if no_services {
        Default::default()
    } else {
        default_services_for_framework(fw.as_deref())
    };

    let proj_config = ProjConfig {
        language: lang.clone(),
        framework: fw.clone(),
        config_version: String::new(),
        includes,
        services,
    };
    let source_files = proj::collect_all_source_files(&repo_path, &proj_config)?;

    if source_files.is_empty() {
        println!(
            "{} No config files found for {}",
            "!".yellow(),
            stack_label(&lang, fw.as_deref())
        );
        return Ok(());
    }

    println!("\nApplying config files...");
    let mut applied = 0;
    for (source, entry) in &source_files {
        // Skip .gitignore here — it is synthesized separately from the
        // bundled github/gitignore templates after the manifest loop.
        if entry.path == ".gitignore" {
            continue;
        }
        let target = cwd.join(&entry.path);
        let changed = proj::apply_file(source, &target, entry.strategy)?;
        let status = if changed {
            applied += 1;
            match entry.strategy {
                FileStrategy::Managed | FileStrategy::Template => "created".green(),
                FileStrategy::Merged => "merged".green(),
            }
        } else {
            "unchanged".dimmed()
        };
        println!(
            "  {:<24} [{}]  {} {}",
            entry.path,
            entry.strategy.label().dimmed(),
            "✓".green(),
            status
        );
    }

    // Synthesized .gitignore from bundled github/gitignore templates.
    match apply_gitignore(&cwd, &lang, fw.as_deref(), None) {
        Ok(result) if result.wrote => {
            applied += 1;
            println!(
                "  {:<24} [{}]  {} {} ({} categories{})",
                ".gitignore",
                "synthesized".dimmed(),
                "✓".green(),
                "created".green(),
                result.categories,
                if result.framework_extra {
                    " + framework extras"
                } else {
                    ""
                },
            );
        }
        Ok(_) => {
            println!(
                "  {:<24} [{}]  {} {}",
                ".gitignore",
                "synthesized".dimmed(),
                "✓".green(),
                "unchanged".dimmed()
            );
        }
        Err(e) => {
            eprintln!("{} .gitignore synthesis failed: {}", "!".yellow(), e);
        }
    }

    let head = proj::config_repo_head(&repo_path)?;
    let final_config = ProjConfig {
        config_version: head.clone(),
        ..proj_config
    };
    proj::save_proj_config(&cwd, &final_config)?;

    try_update_manifest_proj_meta(None, &lang, fw.as_deref(), &head);

    println!(
        "\n{} Project initialized ({}) — {} files applied",
        "✓".green(),
        stack_label(&lang, fw.as_deref()),
        applied
    );
    println!("  Config version: {}", head);

    Ok(())
}

// ── pm project add ──

fn cmd_add(language: Option<String>, framework: Option<String>) -> Result<()> {
    let repo_path = ensure_repo()?;
    let cwd = std::env::current_dir()?;
    let global_manifest = proj::load_global_manifest(&repo_path)?;

    let lang = if let Some(l) = language {
        validate_language(&l, &global_manifest)?;
        l
    } else {
        proj::detect_language(&cwd, &global_manifest)
            .ok_or_else(|| anyhow::anyhow!("Could not detect language. Use --language / -l."))?
    };

    let fw = if let Some(f) = framework {
        validate_framework(&f, &lang, &global_manifest)?;
        Some(f)
    } else {
        proj::detect_framework(&cwd, &lang)
    };

    let head = proj::config_repo_head(&repo_path)?;
    let proj_config = ProjConfig {
        language: lang.clone(),
        framework: fw.clone(),
        config_version: head.clone(),
        includes: Vec::new(),
        services: Default::default(),
    };
    proj::save_proj_config(&cwd, &proj_config)?;

    try_update_manifest_proj_meta(None, &lang, fw.as_deref(), &head);

    println!(
        "{} Registered for config management ({})",
        "✓".green(),
        stack_label(&lang, fw.as_deref())
    );
    println!("  Created .project.yaml");
    println!(
        "  Run '{}' to apply config files.",
        "pm project sync".bold()
    );
    Ok(())
}

// ── pm project sync ──

fn cmd_sync(all: bool, dry_run: bool) -> Result<()> {
    let repo_path = ensure_repo()?;

    if all {
        return sync_all(&repo_path, dry_run);
    }

    let cwd = std::env::current_dir()?;
    sync_project(&repo_path, &cwd, None, dry_run)?;
    Ok(())
}

fn sync_project(
    repo_path: &Path,
    project_dir: &Path,
    project_name_override: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let proj_config = proj::load_proj_config(project_dir)?;
    let head = proj::config_repo_head(repo_path)?;
    let source_files = proj::collect_all_source_files(repo_path, &proj_config)?;

    let project_name = project_name_override.unwrap_or_else(|| {
        project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    });
    let stack = stack_label(&proj_config.language, proj_config.framework.as_deref());

    if dry_run {
        println!("{} ({}) — dry run:", project_name.bold(), stack);
    } else {
        println!("Syncing {} ({}):", project_name.bold(), stack);
    }

    let mut updated = 0;
    let mut unchanged = 0;

    for (source, entry) in &source_files {
        if entry.strategy == FileStrategy::Template {
            continue;
        }
        // Skip .gitignore here — handled by the synthesizer below.
        if entry.path == ".gitignore" {
            continue;
        }
        let target = project_dir.join(&entry.path);

        if proj::is_file_outdated(source, &target, entry.strategy) {
            if dry_run {
                println!(
                    "  {} {} ({})",
                    "~".yellow(),
                    entry.path,
                    entry.strategy.label()
                );
            } else {
                proj::apply_file(source, &target, entry.strategy)?;
                println!("  {} {} {}", "✓".green(), entry.path, "updated".green());
            }
            updated += 1;
        } else {
            unchanged += 1;
        }
    }

    // Re-synthesize .gitignore from bundled templates.
    if dry_run {
        // Show whether sync would change .gitignore.
        match synthesize_gitignore_preview(
            project_dir,
            &proj_config.language,
            proj_config.framework.as_deref(),
            None,
        ) {
            Ok(new_content) => {
                let existing =
                    std::fs::read_to_string(project_dir.join(".gitignore")).unwrap_or_default();
                if existing != new_content {
                    println!("  {} {} ({})", "~".yellow(), ".gitignore", "synthesized");
                    updated += 1;
                } else {
                    unchanged += 1;
                }
            }
            Err(_) => unchanged += 1,
        }
    } else {
        match apply_gitignore(
            project_dir,
            &proj_config.language,
            proj_config.framework.as_deref(),
            None,
        ) {
            Ok(result) if result.wrote => {
                println!("  {} {} {}", "✓".green(), ".gitignore", "updated".green());
                updated += 1;
            }
            Ok(_) => unchanged += 1,
            Err(e) => {
                eprintln!("{} .gitignore synthesis failed: {}", "!".yellow(), e);
                unchanged += 1;
            }
        }
    }

    if !dry_run && updated > 0 {
        let mut config = proj_config;
        config.config_version = head.clone();
        proj::save_proj_config(project_dir, &config)?;
    }

    println!(
        "\n{} updated, {} unchanged.",
        if updated > 0 {
            format!("{updated}").green().to_string()
        } else {
            format!("{updated}")
        },
        unchanged
    );

    Ok(())
}

fn sync_all(repo_path: &Path, dry_run: bool) -> Result<()> {
    let (config, mut manifest) = state::load_state()?;

    let proj_projects: Vec<_> = manifest
        .projects
        .iter()
        .filter(|p| p.proj.is_some())
        .map(|p| (p.name.clone(), p.proj.clone().unwrap()))
        .collect();

    if proj_projects.is_empty() {
        println!("No projects with config management enabled.");
        return Ok(());
    }

    let head = proj::config_repo_head(repo_path)?;
    let mut manifest_changed = false;

    for (name, _meta) in &proj_projects {
        let project = state::find_project(&manifest, name)?;
        let project_path = state::project_path(&config, &manifest, project)?;
        if !project_path.exists() {
            println!(
                "{} {} — project missing, skipping",
                "!".yellow(),
                name.bold()
            );
            continue;
        }
        match sync_project(repo_path, &project_path, Some(name), dry_run) {
            Ok(()) => {
                if !dry_run {
                    if let Ok(p) = find_project_mut(&mut manifest, name) {
                        if let Some(ref mut proj_meta) = p.proj {
                            proj_meta.config_version = head.clone();
                            manifest_changed = true;
                        }
                    }
                }
            }
            Err(e) => {
                println!("{} {} — {}", "✗".red(), name.bold(), e);
            }
        }
        println!();
    }

    if manifest_changed {
        let _ = state::save_state(&config, &manifest);
    }

    Ok(())
}

// ── pm project check ──

fn cmd_check(all: bool) -> Result<()> {
    let repo_path = ensure_repo()?;

    if all {
        return check_all(&repo_path);
    }

    let cwd = std::env::current_dir()?;
    check_project(&repo_path, &cwd, None)
}

fn check_project(repo_path: &Path, project_dir: &Path, name_override: Option<&str>) -> Result<()> {
    let proj_config = proj::load_proj_config(project_dir)?;
    let head = proj::config_repo_head(repo_path)?;
    let source_files = proj::collect_all_source_files(repo_path, &proj_config)?;

    let project_name = name_override.unwrap_or_else(|| {
        project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    });
    let stack = stack_label(&proj_config.language, proj_config.framework.as_deref());

    let outdated_files: Vec<&str> = source_files
        .iter()
        .filter(|(source, entry)| {
            entry.strategy != FileStrategy::Template
                && proj::is_file_outdated(source, &project_dir.join(&entry.path), entry.strategy)
        })
        .map(|(_, entry)| entry.path.as_str())
        .collect();

    let version_outdated = proj_config.config_version != head;

    if outdated_files.is_empty() && !version_outdated {
        println!(
            "{} {} ({}) — up to date",
            "✓".green(),
            project_name.bold(),
            stack
        );
    } else {
        let count = outdated_files.len();
        println!(
            "{} {} ({}) — outdated ({} file{} changed)",
            "✗".red(),
            project_name.bold(),
            stack,
            count,
            if count == 1 { "" } else { "s" }
        );
        for file in &outdated_files {
            println!("  - {}", file);
        }
    }

    Ok(())
}

fn check_all(repo_path: &Path) -> Result<()> {
    let (config, manifest) = state::load_state()?;

    let proj_projects: Vec<_> = manifest
        .projects
        .iter()
        .filter(|p| p.proj.is_some())
        .collect();

    if proj_projects.is_empty() {
        println!("No projects with config management enabled.");
        return Ok(());
    }

    for project in &proj_projects {
        let project_path = state::project_path(&config, &manifest, project)?;
        if !project_path.exists() {
            println!("{} {} — project missing", "!".yellow(), project.name.bold());
            continue;
        }
        if let Err(e) = check_project(repo_path, &project_path, Some(&project.name)) {
            println!("{} {} — {}", "✗".red(), project.name.bold(), e);
        }
    }

    Ok(())
}

// ── pm project diff ──

fn cmd_diff() -> Result<()> {
    let repo_path = ensure_repo()?;
    let cwd = std::env::current_dir()?;
    let proj_config = proj::load_proj_config(&cwd)?;
    let source_files = proj::collect_all_source_files(&repo_path, &proj_config)?;

    let mut has_diff = false;

    for (source, entry) in &source_files {
        if entry.strategy == FileStrategy::Template {
            continue;
        }
        let target = cwd.join(&entry.path);
        if let Some(diff_output) = proj::diff_file(source, &target) {
            has_diff = true;
            println!(
                "{} {}",
                "---".red(),
                format!("upstream: {}", entry.path).red()
            );
            println!(
                "{} {}",
                "+++".green(),
                format!("local: {}", entry.path).green()
            );
            for line in diff_output.lines() {
                if line.starts_with('+') {
                    println!("{}", line.green());
                } else if line.starts_with('-') {
                    println!("{}", line.red());
                } else {
                    println!("{}", line);
                }
            }
            println!();
        }
    }

    if !has_diff {
        println!("{} All config files are up to date.", "✓".green());
    }

    Ok(())
}

// ── pm project list ──

fn cmd_list() -> Result<()> {
    let (config, manifest) = state::load_state()?;

    let proj_projects: Vec<_> = manifest
        .projects
        .iter()
        .filter(|p| p.proj.is_some())
        .collect();

    if proj_projects.is_empty() {
        println!("No projects with config management enabled.");
        println!(
            "Run '{}' in a project to get started.",
            "pm project init".bold()
        );
        return Ok(());
    }

    println!(
        "{:<16} {:<16} {:<32} {}",
        "NAME".bold(),
        "STACK".bold(),
        "PATH".bold(),
        "CONFIG".bold()
    );

    for project in &proj_projects {
        let meta = project.proj.as_ref().unwrap();
        let stack = stack_label(&meta.language, meta.framework.as_deref());
        let path = state::project_path_display(&config, &manifest, project)
            .unwrap_or_else(|_| "?".to_string());

        println!(
            "{:<16} {:<16} {:<32} {}",
            project.name,
            stack,
            path,
            meta.config_version.dimmed()
        );
    }

    Ok(())
}

// ── Interactive helpers ──

fn resolve_language_interactive(
    project_dir: &Path,
    global_manifest: &proj::GlobalManifest,
) -> Result<String> {
    if !restore::can_prompt() {
        return Err(anyhow::anyhow!(
            "Interactive mode requires a TTY. Use --language / -l and --no-interactive / -y."
        ));
    }

    if let Some(detected) = proj::detect_language(project_dir, global_manifest) {
        let lang_entry = global_manifest
            .languages
            .iter()
            .find(|l| l.id == detected)
            .ok_or_else(|| anyhow::anyhow!("Detected language '{}' not in manifest", detected))?;
        println!("Detected: {} ({})", lang_entry.name.bold(), detected);

        let confirm = dialoguer::Confirm::new()
            .with_prompt("Use detected language?")
            .default(true)
            .interact()?;

        if confirm {
            return Ok(detected);
        }
    }

    let items: Vec<String> = global_manifest
        .languages
        .iter()
        .map(|l| format!("{} ({})", l.name, l.id))
        .collect();

    let selection = dialoguer::Select::new()
        .with_prompt("Language")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(global_manifest.languages[selection].id.clone())
}

fn resolve_framework_interactive(
    project_dir: &Path,
    language: &str,
    global_manifest: &proj::GlobalManifest,
) -> Result<Option<String>> {
    let lang_entry = global_manifest.languages.iter().find(|l| l.id == language);

    let Some(lang_entry) = lang_entry else {
        return Ok(None);
    };

    if lang_entry.frameworks.is_empty() {
        return Ok(None);
    }

    let detected = proj::detect_framework(project_dir, language);

    let mut items: Vec<String> = lang_entry
        .frameworks
        .iter()
        .map(|f| {
            if detected.as_deref() == Some(f.as_str()) {
                format!("{f} (detected)")
            } else {
                f.clone()
            }
        })
        .collect();
    items.push("None".to_string());

    let default_idx = if let Some(ref det) = detected {
        lang_entry
            .frameworks
            .iter()
            .position(|f| f == det)
            .unwrap_or(0)
    } else {
        0
    };

    let selection = dialoguer::Select::new()
        .with_prompt("Framework")
        .items(&items)
        .default(default_idx)
        .interact()?;

    if selection == items.len() - 1 {
        Ok(None)
    } else {
        Ok(Some(lang_entry.frameworks[selection].clone()))
    }
}

fn resolve_includes_interactive() -> Result<Vec<String>> {
    let mut includes = Vec::new();

    if dialoguer::Confirm::new()
        .with_prompt("Include CI/CD?")
        .default(true)
        .interact()?
    {
        includes.push("ci".to_string());
    }
    if dialoguer::Confirm::new()
        .with_prompt("Include Dockerfile?")
        .default(true)
        .interact()?
    {
        includes.push("docker".to_string());
    }
    if dialoguer::Confirm::new()
        .with_prompt("Include pre-commit hooks?")
        .default(true)
        .interact()?
    {
        includes.push("hooks".to_string());
    }

    Ok(includes)
}

// ── Validation helpers ──

fn validate_language(lang: &str, global_manifest: &proj::GlobalManifest) -> Result<()> {
    if global_manifest.languages.iter().any(|l| l.id == lang) {
        Ok(())
    } else {
        Err(PmError::UnsupportedLanguage(lang.to_string()).into())
    }
}

fn validate_framework(fw: &str, lang: &str, global_manifest: &proj::GlobalManifest) -> Result<()> {
    let lang_entry = global_manifest
        .languages
        .iter()
        .find(|l| l.id == lang)
        .ok_or_else(|| PmError::UnsupportedLanguage(lang.to_string()))?;

    if lang_entry.frameworks.iter().any(|f| f == fw) {
        Ok(())
    } else {
        Err(PmError::UnsupportedFramework(fw.to_string(), lang.to_string()).into())
    }
}

// ── Manifest integration ──

/// Update ProjMeta in manifest.json. If project_name is None, detect from cwd.
fn try_update_manifest_proj_meta(
    project_name: Option<&str>,
    language: &str,
    framework: Option<&str>,
    config_version: &str,
) {
    let Ok((config, mut manifest)) = state::load_state() else {
        return;
    };

    let name = if let Some(n) = project_name {
        n.to_string()
    } else {
        let Some((project, _)) = state::detect_current_project(&config, &manifest) else {
            return;
        };
        project.name.clone()
    };

    let Ok(project) = find_project_mut(&mut manifest, &name) else {
        return;
    };
    project.proj = Some(ProjMeta {
        language: language.to_string(),
        framework: framework.map(|s| s.to_string()),
        config_version: config_version.to_string(),
    });
    if let Err(e) = state::save_state(&config, &manifest) {
        eprintln!("{} Failed to update manifest: {}", "!".yellow(), e);
    }
}

// ── pm project gitignore ──

/// Outcome of a gitignore synthesis pass.
pub struct GitignoreApplyResult {
    pub wrote: bool,
    /// Number of v0.4.x legacy lines moved into the managed block.
    /// Currently logged via stderr inside `apply_gitignore`; exposed here
    /// for future programmatic use (e.g. tests / `pm project diff`).
    #[allow(dead_code)]
    pub migrated_legacy: usize,
    pub categories: usize,
    pub framework_extra: bool,
}

/// Synthesize and write `.gitignore` for the given project directory.
/// Used by `pm project init`, `pm project sync`, and `pm project gitignore`.
///
/// `categories_override` lets the caller force a category set; pass `None`
/// to use the language-derived defaults.
pub fn apply_gitignore(
    project_dir: &Path,
    language: &str,
    framework: Option<&str>,
    categories_override: Option<&[crate::templates::Category]>,
) -> Result<GitignoreApplyResult> {
    use crate::templates::{default_categories, legacy, marker, synthesize, Category};

    let resolved: Vec<Category> = match categories_override {
        Some(list) => list.to_vec(),
        None => default_categories(language, framework),
    };

    let framework_extra =
        framework.and_then(|fw| read_framework_extra(language, fw).ok().flatten());

    let body =
        synthesize::synthesize_managed_body(&resolved, framework, framework_extra.as_deref());

    let target = project_dir.join(".gitignore");
    let existing = std::fs::read_to_string(&target).unwrap_or_default();

    // Migrate v0.4.x lines from the user region (only when no marker block
    // is present yet — once we have a managed block, those lines are
    // already inside it).
    let parsed = marker::parse(&existing);
    let (cleaned_existing, migrated) = if parsed.managed.is_none() {
        let (cleaned_user, n) = legacy::strip_legacy_patterns(&parsed.before);
        if n > 0 {
            (cleaned_user, n)
        } else {
            (existing.clone(), 0)
        }
    } else {
        (existing.clone(), 0)
    };

    let new_content = marker::merge_into_existing(&cleaned_existing, &body);

    if existing == new_content {
        return Ok(GitignoreApplyResult {
            wrote: false,
            migrated_legacy: 0,
            categories: resolved.len(),
            framework_extra: framework_extra.is_some(),
        });
    }

    std::fs::write(&target, &new_content)?;
    legacy::emit_migration_notice(migrated);
    Ok(GitignoreApplyResult {
        wrote: true,
        migrated_legacy: migrated,
        categories: resolved.len(),
        framework_extra: framework_extra.is_some(),
    })
}

/// Render the synthesized `.gitignore` content without writing — used by
/// `pm project gitignore --diff`.
pub fn synthesize_gitignore_preview(
    project_dir: &Path,
    language: &str,
    framework: Option<&str>,
    categories_override: Option<&[crate::templates::Category]>,
) -> Result<String> {
    use crate::templates::{default_categories, legacy, marker, synthesize, Category};

    let resolved: Vec<Category> = match categories_override {
        Some(list) => list.to_vec(),
        None => default_categories(language, framework),
    };
    let framework_extra =
        framework.and_then(|fw| read_framework_extra(language, fw).ok().flatten());
    let body =
        synthesize::synthesize_managed_body(&resolved, framework, framework_extra.as_deref());

    let target = project_dir.join(".gitignore");
    let existing = std::fs::read_to_string(&target).unwrap_or_default();
    let parsed = marker::parse(&existing);
    let cleaned = if parsed.managed.is_none() {
        let (cleaned_user, _) = legacy::strip_legacy_patterns(&parsed.before);
        cleaned_user
    } else {
        existing
    };
    Ok(marker::merge_into_existing(&cleaned, &body))
}

fn cmd_gitignore(diff: bool, categories: Option<String>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let proj_config = proj::load_proj_config(&cwd).ok();
    let language = proj_config
        .as_ref()
        .map(|c| c.language.as_str())
        .unwrap_or("");
    let framework_str = proj_config.as_ref().and_then(|c| c.framework.clone());
    let framework = framework_str.as_deref();

    let categories_override = match categories.as_deref() {
        Some(list) => {
            let parsed = parse_categories_list(list)?;
            if parsed.is_empty() {
                return Err(anyhow::anyhow!(
                    "no valid categories selected; valid keys: {}",
                    valid_category_list(),
                ));
            }
            Some(parsed)
        }
        None => None,
    };

    if diff {
        let new_content = synthesize_gitignore_preview(
            &cwd,
            language,
            framework,
            categories_override.as_deref(),
        )?;
        let target = cwd.join(".gitignore");
        let existing = std::fs::read_to_string(&target).unwrap_or_default();
        let diff_text = render_unified_diff(&existing, &new_content, ".gitignore");
        if diff_text.trim().is_empty() || existing == new_content {
            println!("{}", "(no changes)".dimmed());
        } else {
            print!("{diff_text}");
        }
        return Ok(());
    }

    let result = apply_gitignore(&cwd, language, framework, categories_override.as_deref())?;

    if !result.wrote {
        println!("{}", ".gitignore is up to date".dimmed());
        return Ok(());
    }

    println!(
        "{} .gitignore ({} categories{})",
        "✓".green(),
        result.categories,
        if result.framework_extra {
            " + framework extras"
        } else {
            ""
        },
    );
    Ok(())
}

fn parse_categories_list(list: &str) -> Result<Vec<crate::templates::Category>> {
    use crate::templates::lookup_category;
    let mut out = Vec::new();
    for tok in list.split(',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let cat = lookup_category(tok).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown category '{}'. Valid: {}",
                tok,
                valid_category_list(),
            )
        })?;
        if !out.contains(&cat) {
            out.push(cat);
        }
    }
    Ok(out)
}

fn valid_category_list() -> String {
    crate::templates::Category::ALL
        .iter()
        .map(|c| c.key())
        .collect::<Vec<_>>()
        .join(", ")
}

fn read_framework_extra(language: &str, framework: &str) -> Result<Option<String>> {
    if language.is_empty() || framework.is_empty() {
        return Ok(None);
    }
    let repo_path = ensure_repo()?;
    let path = repo_path
        .join(language)
        .join(framework)
        .join(".gitignore.extra");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(Some(content))
}

/// Render a minimal unified diff. We use the same `similar` crate that
/// powers `pm project diff` for consistency.
fn render_unified_diff(old: &str, new: &str, name: &str) -> String {
    use similar::TextDiff;
    let diff = TextDiff::from_lines(old, new);
    let mut out = format!("--- a/{name}\n+++ b/{name}\n");
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        out.push_str(sign);
        out.push_str(change.value());
    }
    out
}

// ── Next.js / pnpm convention helpers ──

/// Warn (do not abort) when a non-pnpm lockfile is found in a Next.js init.
/// pm enforces pnpm + Turbopack as the project convention.
fn warn_on_competing_lockfiles(project_dir: &Path) {
    for (lockfile, package_manager) in [
        ("package-lock.json", "npm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
    ] {
        if project_dir.join(lockfile).exists() {
            eprintln!(
                "{} {} found — pm projects expect pnpm. \
                 Remove `{}` and run `pnpm install` to switch.",
                "!".yellow(),
                lockfile,
                lockfile,
            );
            let _ = package_manager;
        }
    }
}

/// Default `services:` block to write into `.proj.yaml` when the user runs
/// `pm proj init` without `--no-services`.
///
/// The intent is "minimum viable orchestrator" — a single service named
/// after the framework's role so `pm run` works out of the box. The user
/// is expected to extend the section (`back`, additional `front`s, etc.)
/// as needed.
pub fn default_services_for_framework(
    framework: Option<&str>,
) -> std::collections::HashMap<String, proj::ServiceDef> {
    let mut map = std::collections::HashMap::new();
    let key = match framework {
        Some("nextjs") | Some("vite") | Some("flutter") => "front",
        Some("nestjs") | Some("axum") | Some("fastapi") => "back",
        // Unknown / generic frameworks: skip auto-add to avoid creating a
        // service entry the user did not ask for.
        _ => return map,
    };
    let def = proj::ServiceDef {
        framework: framework.map(|s| s.to_string()),
        ..Default::default()
    };
    map.insert(key.to_string(), def);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_services_nextjs_creates_front() {
        let m = default_services_for_framework(Some("nextjs"));
        assert!(m.contains_key("front"));
        assert_eq!(
            m["front"].framework.as_deref(),
            Some("nextjs"),
            "service framework should match project framework",
        );
    }

    #[test]
    fn default_services_axum_creates_back() {
        let m = default_services_for_framework(Some("axum"));
        assert!(m.contains_key("back"));
    }

    #[test]
    fn default_services_unknown_returns_empty() {
        let m = default_services_for_framework(Some("rocket"));
        assert!(m.is_empty());
    }

    #[test]
    fn default_services_none_returns_empty() {
        let m = default_services_for_framework(None);
        assert!(m.is_empty());
    }
}
