use crate::config::{config_dir, projects_path, workspaces_path};
use crate::state::{detect_current_project, load_state, project_path};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub language: PluginLanguage,
    pub entry: PathBuf,
    pub aliases: Vec<String>,
    pub dir: PathBuf,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLanguage {
    Python,
    Shell,
}

impl PluginLanguage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Shell => "shell",
        }
    }

    fn default_entry(self) -> &'static str {
        match self {
            Self::Python => "main.py",
            Self::Shell => "main.sh",
        }
    }

    fn runner(self) -> &'static str {
        match self {
            Self::Python => "python3",
            Self::Shell => "sh",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PluginToml {
    plugin: PluginMeta,
    #[serde(default)]
    command: CommandMeta,
}

#[derive(Debug, Deserialize, Serialize)]
struct PluginMeta {
    name: String,
    version: String,
    description: String,
    language: PluginLanguageToml,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct CommandMeta {
    #[serde(default)]
    usage: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum PluginLanguageToml {
    Py,
    Sh,
}

impl From<PluginLanguageToml> for PluginLanguage {
    fn from(value: PluginLanguageToml) -> Self {
        match value {
            PluginLanguageToml::Py => Self::Python,
            PluginLanguageToml::Sh => Self::Shell,
        }
    }
}

impl From<PluginLanguage> for PluginLanguageToml {
    fn from(value: PluginLanguage) -> Self {
        match value {
            PluginLanguage::Python => Self::Py,
            PluginLanguage::Shell => Self::Sh,
        }
    }
}

fn default_enabled() -> bool {
    true
}

pub fn plugins_dir() -> PathBuf {
    config_dir().join("plugins").join("commands")
}

pub fn discover_plugins() -> Vec<Plugin> {
    let root = plugins_dir();
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut plugins = entries
        .flatten()
        .filter_map(|entry| load_plugin(&entry.path()).ok())
        .collect::<Vec<_>>();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

pub fn find_plugin(name: &str) -> Option<Plugin> {
    discover_plugins().into_iter().find(|plugin| {
        plugin.enabled && (plugin.name == name || plugin.aliases.iter().any(|alias| alias == name))
    })
}

pub fn get_plugin(name: &str) -> Option<Plugin> {
    discover_plugins()
        .into_iter()
        .find(|plugin| plugin.name == name || plugin.aliases.iter().any(|alias| alias == name))
}

pub fn run_plugin(plugin: &Plugin, args: &[String]) -> Result<()> {
    let mut command = Command::new(plugin.language.runner());
    command
        .arg(&plugin.entry)
        .args(args)
        .current_dir(std::env::current_dir()?)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("PM_CONFIG_DIR", config_dir())
        .env("PM_PROJECTS_FILE", projects_path())
        .env("PM_WORKSPACES_FILE", workspaces_path())
        .env("PM_MANIFEST_FILE", crate::config::manifest_path())
        .env("PM_PLUGIN_DIR", &plugin.dir);

    if let Ok((config, manifest)) = load_state() {
        if let Some((project, project_root)) = detect_current_project(&config, &manifest) {
            command
                .env("PM_PROJECT", &project.name)
                .env("PM_PROJECT_PATH", project_root)
                .env("PM_WORKSPACE", &project.workspace)
                .env("PM_PROJECT_TAGS", project.tags.join(","));
        } else {
            command.env("PM_WORKSPACE", &config.current_workspace);
        }

        if std::env::var_os("PM_PROJECT").is_none() {
            if let Some(project_name) = &config.current_project {
                if let Ok(project) = crate::state::find_project(&manifest, project_name) {
                    if let Ok(path) = project_path(&config, &manifest, project) {
                        command
                            .env("PM_CURRENT_PROJECT", &project.name)
                            .env("PM_CURRENT_PROJECT_PATH", path);
                    }
                }
            }
        }
    }

    let status = command
        .status()
        .with_context(|| format!("Failed to run plugin '{}'", plugin.name))?;

    if status.success() {
        return Ok(());
    }

    std::process::exit(status.code().unwrap_or(1));
}

pub fn set_plugin_enabled(name: &str, enabled: bool) -> Result<Plugin> {
    let plugin = get_plugin(name).with_context(|| format!("Plugin '{name}' not found"))?;
    let manifest_path = plugin.dir.join("plugin.toml");
    let raw = fs::read_to_string(&manifest_path)?;
    let mut parsed: PluginToml = toml::from_str(&raw)?;
    parsed.plugin.enabled = enabled;
    fs::write(&manifest_path, toml::to_string_pretty(&parsed)?)?;
    load_plugin(&plugin.dir)
}

fn load_plugin(dir: &Path) -> Result<Plugin> {
    let manifest_path = dir.join("plugin.toml");
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let parsed: PluginToml = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
    let language: PluginLanguage = parsed.plugin.language.into();
    let entry = dir.join(
        parsed
            .plugin
            .entry
            .clone()
            .unwrap_or_else(|| language.default_entry().to_string()),
    );

    Ok(Plugin {
        name: parsed.plugin.name,
        version: parsed.plugin.version,
        description: parsed.plugin.description,
        language,
        entry,
        aliases: parsed.command.aliases,
        dir: dir.to_path_buf(),
        enabled: parsed.plugin.enabled,
    })
}
