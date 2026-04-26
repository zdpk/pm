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

    // Fetch latest release tag from GitHub API
    println!("Checking for updates...");
    let latest = fetch_latest_version()?;

    if latest.tag == format!("v{current_version}") {
        println!("{} Already on the latest version.", "✓".green());
        return Ok(());
    }

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

fn fetch_latest_version() -> Result<ReleaseInfo> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"),
            "--jq",
            r#".tag_name as $tag | .assets | map({name: .name, url: .browser_download_url}) | {tag: $tag, assets: .}"#,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Fallback: try without gh (curl)
        return fetch_latest_version_curl().map_err(|_| {
            anyhow::anyhow!(
                "Failed to check for updates. Install `gh` CLI or check network.\n{stderr}"
            )
        });
    }

    let stdout = String::from_utf8(output.stdout)?;
    parse_release_json(&stdout)
}

fn fetch_latest_version_curl() -> Result<ReleaseInfo> {
    let output = Command::new("curl")
        .args([
            "-sL",
            &format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"),
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("curl failed"));
    }

    let stdout = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No tag_name in response"))?
        .to_string();

    let assets = json["assets"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|a| {
            Some(AssetInfo {
                name: a["name"].as_str()?.to_string(),
                url: a["browser_download_url"].as_str()?.to_string(),
            })
        })
        .collect();

    Ok(ReleaseInfo { tag, assets })
}

fn parse_release_json(json_str: &str) -> Result<ReleaseInfo> {
    let json: serde_json::Value = serde_json::from_str(json_str)?;

    let tag = json["tag"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No tag in response"))?
        .to_string();

    let assets = json["assets"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|a| {
            Some(AssetInfo {
                name: a["name"].as_str()?.to_string(),
                url: a["url"].as_str()?.to_string(),
            })
        })
        .collect();

    Ok(ReleaseInfo { tag, assets })
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
