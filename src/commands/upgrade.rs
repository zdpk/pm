use anyhow::Result;
use colored::Colorize;
use std::env;
use std::fs;
use std::process::Command;

const REPO_OWNER: &str = "zdpk";
const REPO_NAME: &str = "pm";

pub fn run() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: v{current_version}");
    let current_version = Version::parse(current_version)
        .ok_or_else(|| anyhow::anyhow!("Invalid current version: {current_version}"))?;

    println!("Checking for updates...");
    let releases = fetch_releases()?;

    let Some(latest) = select_latest_update(releases, current_version) else {
        println!("{} No newer release found.", "✓".green());
        return Ok(());
    };

    println!("New version available: {}", latest.tag.bold());

    // Determine current platform
    let target = detect_target()?;
    let bin_name = current_bin_name();
    let archive_name = format!("{bin_name}-{}-{target}.tar.gz", latest.tag);

    // Find the matching asset URL
    let asset_url = latest
        .assets
        .iter()
        .find(|a| a.name == archive_name)
        .map(|a| a.url.clone())
        .ok_or_else(|| anyhow::anyhow!("No release asset found for {target}"))?;

    // Download to temp location
    let tmp_dir = env::temp_dir().join(format!("pm-upgrade-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir)?;
    let archive_path = tmp_dir.join(&archive_name);

    println!("Downloading {archive_name}...");
    download_file(&asset_url, &archive_path)?;

    // Extract
    let status = Command::new("tar")
        .arg("xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&tmp_dir)
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to extract archive"));
    }

    // Replace current binary
    let new_bin = &tmp_dir.join(&bin_name);
    let current_exe = env::current_exe()?;

    if !new_bin.exists() {
        return Err(anyhow::anyhow!(
            "Expected binary '{}' not found in archive",
            bin_name
        ));
    }

    // Atomic-ish replace: rename old, copy new, remove old
    let backup = current_exe.with_extension("old");
    fs::rename(&current_exe, &backup)?;
    match fs::copy(&new_bin, &current_exe) {
        Ok(_) => {
            let _ = fs::remove_file(&backup);
            // Preserve execute permission
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&current_exe, fs::Permissions::from_mode(0o755))?;
            }
        }
        Err(e) => {
            // Rollback
            let _ = fs::rename(&backup, &current_exe);
            return Err(e.into());
        }
    }

    let _ = fs::remove_dir_all(&tmp_dir);

    println!(
        "{} Upgraded to {} ({})",
        "✓".green(),
        latest.tag.bold(),
        target
    );
    Ok(())
}

struct ReleaseInfo {
    tag: String,
    assets: Vec<AssetInfo>,
}

struct AssetInfo {
    name: String,
    url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    fn parse(value: &str) -> Option<Self> {
        let value = value.strip_prefix('v').unwrap_or(value);
        let value = value.split_once('+').map_or(value, |(version, _)| version);
        if value.contains('-') {
            return None;
        }

        let mut parts = value.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }

        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

fn select_latest_update(
    releases: Vec<ReleaseInfo>,
    current_version: Version,
) -> Option<ReleaseInfo> {
    releases
        .into_iter()
        .filter_map(|release| {
            let version = Version::parse(&release.tag)?;
            (version > current_version).then_some((version, release))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, release)| release)
}

fn fetch_releases() -> Result<Vec<ReleaseInfo>> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{REPO_OWNER}/{REPO_NAME}/releases?per_page=100"),
            "--jq",
            r#"map(select((.draft | not) and (.prerelease | not)) | {tag: .tag_name, assets: (.assets | map({name: .name, url: .browser_download_url}))})"#,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return fetch_releases_curl().map_err(|_| {
            anyhow::anyhow!(
                "Failed to check for updates. Install `gh` CLI or check network.\n{stderr}"
            )
        });
    }

    let stdout = String::from_utf8(output.stdout)?;
    parse_releases_json(&stdout)
}

fn fetch_releases_curl() -> Result<Vec<ReleaseInfo>> {
    let output = Command::new("curl")
        .args([
            "-sL",
            &format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases?per_page=100"),
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("curl failed"));
    }

    let stdout = String::from_utf8(output.stdout)?;
    parse_releases_json(&stdout)
}

fn parse_releases_json(json_str: &str) -> Result<Vec<ReleaseInfo>> {
    let json: serde_json::Value = serde_json::from_str(json_str)?;
    let releases = json
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Expected release list response"))?;

    let releases = releases
        .iter()
        .filter(|release| {
            !release["draft"].as_bool().unwrap_or(false)
                && !release["prerelease"].as_bool().unwrap_or(false)
        })
        .filter_map(|release| {
            let tag = release["tag"]
                .as_str()
                .or_else(|| release["tag_name"].as_str())?
                .to_string();

            let assets = release["assets"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|asset| {
                    Some(AssetInfo {
                        name: asset["name"].as_str()?.to_string(),
                        url: asset["url"]
                            .as_str()
                            .or_else(|| asset["browser_download_url"].as_str())?
                            .to_string(),
                    })
                })
                .collect();

            Some(ReleaseInfo { tag, assets })
        })
        .collect();

    Ok(releases)
}

fn detect_target() -> Result<String> {
    let arch = env::consts::ARCH;
    let os = env::consts::OS;

    let target = match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return Err(anyhow::anyhow!("Unsupported platform: {os}/{arch}")),
    };
    Ok(target.to_string())
}

fn current_bin_name() -> String {
    env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "pm".to_string())
}

fn download_file(url: &str, dest: &std::path::Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-sL", "-o"])
        .arg(dest)
        .arg(url)
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!("Download failed: {url}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> ReleaseInfo {
        ReleaseInfo {
            tag: tag.to_string(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn parses_stable_versions() {
        assert_eq!(
            Version::parse("0.2.0"),
            Some(Version {
                major: 0,
                minor: 2,
                patch: 0
            })
        );
        assert_eq!(
            Version::parse("v1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
        assert_eq!(
            Version::parse("v1.2.3+build.1"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
        assert_eq!(Version::parse("v1.2.3-beta.1"), None);
    }

    #[test]
    fn ignores_releases_not_newer_than_current() {
        let current = Version::parse("0.2.0").unwrap();
        let selected = select_latest_update(vec![release("v0.1.0"), release("v0.2.0")], current);

        assert!(selected.is_none());
    }

    #[test]
    fn selects_highest_newer_stable_release() {
        let current = Version::parse("0.2.0").unwrap();
        let selected = select_latest_update(
            vec![
                release("v0.2.1"),
                release("v0.3.0-beta.1"),
                release("v0.1.9"),
                release("v0.2.2"),
            ],
            current,
        )
        .unwrap();

        assert_eq!(selected.tag, "v0.2.2");
    }
}
