use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;

/// Get the binary name for help messages (always "pm" now)
pub fn get_binary_name() -> &'static str {
    "pm"
}

pub fn get_last_git_commit_time(path: &Path) -> Result<Option<DateTime<Utc>>> {
    if !path.join(".git").exists() {
        return Ok(None);
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("log")
        .arg("-1")
        .arg("--format=%aI")
        .output()
        .context("Failed to execute git command")?;

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout)?;
        let timestamp_str = stdout.trim();
        if timestamp_str.is_empty() {
            Ok(None)
        } else {
            let datetime = DateTime::parse_from_rfc3339(timestamp_str)?;
            Ok(Some(datetime.with_timezone(&Utc)))
        }
    } else {
        let stderr = String::from_utf8(output.stderr)?;
        eprintln!("Error getting git commit time: {}", stderr);
        Ok(None)
    }
}

#[allow(dead_code)]
pub fn get_git_remote_url(path: &Path) -> Result<Option<String>> {
    if !path.join(".git").exists() {
        return Ok(None);
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()
        .context("Failed to execute git remote command")?;

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout)?;
        let url = stdout.trim();
        if url.is_empty() {
            Ok(None)
        } else {
            Ok(Some(url.to_string()))
        }
    } else {
        Ok(None)
    }
}

#[allow(dead_code)]
pub fn get_git_current_branch(path: &Path) -> Result<Option<String>> {
    if !path.join(".git").exists() {
        return Ok(None);
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("branch")
        .arg("--show-current")
        .output()
        .context("Failed to execute git branch command")?;

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout)?;
        let branch = stdout.trim();
        if branch.is_empty() {
            Ok(None)
        } else {
            Ok(Some(branch.to_string()))
        }
    } else {
        Ok(None)
    }
}

#[allow(dead_code)]
pub fn get_git_status(path: &Path) -> Result<Option<String>> {
    if !path.join(".git").exists() {
        return Ok(None);
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("status")
        .arg("--porcelain")
        .output()
        .context("Failed to execute git status command")?;

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout)?;
        let status = stdout.trim();
        if status.is_empty() {
            Ok(Some("clean".to_string()))
        } else {
            let lines: Vec<&str> = status.lines().collect();
            Ok(Some(format!("{} changes", lines.len())))
        }
    } else {
        Ok(None)
    }
}

#[allow(dead_code)]
pub fn is_git_repository(path: &Path) -> bool {
    path.join(".git").exists()
}


#[allow(dead_code)]
pub fn detect_project_language(path: &Path) -> Option<String> {
    let files = std::fs::read_dir(path).ok()?;
    let mut language_counts: std::collections::HashMap<&str, u32> =
        std::collections::HashMap::new();

    for entry in files.flatten() {
        if let Some(extension) = entry.path().extension() {
            if let Some(ext_str) = extension.to_str() {
                let language = match ext_str {
                    "rs" => "Rust",
                    "js" | "jsx" => "JavaScript",
                    "ts" | "tsx" => "TypeScript",
                    "py" => "Python",
                    "go" => "Go",
                    "java" => "Java",
                    "cpp" | "cc" | "cxx" => "C++",
                    "c" => "C",
                    "rb" => "Ruby",
                    "php" => "PHP",
                    "swift" => "Swift",
                    "kt" => "Kotlin",
                    "dart" => "Dart",
                    "scala" => "Scala",
                    "clj" => "Clojure",
                    "hs" => "Haskell",
                    "ml" => "OCaml",
                    "fs" => "F#",
                    "elm" => "Elm",
                    "ex" | "exs" => "Elixir",
                    "erl" => "Erlang",
                    "lua" => "Lua",
                    "r" => "R",
                    "jl" => "Julia",
                    "nim" => "Nim",
                    "zig" => "Zig",
                    "v" => "V",
                    "cr" => "Crystal",
                    "d" => "D",
                    _ => continue,
                };
                *language_counts.entry(language).or_insert(0) += 1;
            }
        }
    }

    language_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang.to_string())
}
