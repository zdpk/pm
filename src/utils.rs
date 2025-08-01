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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_files(temp_dir: &Path, files: &[(&str, &str)]) {
        for (filename, content) in files {
            let file_path = temp_dir.join(filename);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(file_path, content).unwrap();
        }
    }

    #[test]
    fn test_get_binary_name() {
        assert_eq!(get_binary_name(), "pm");
    }

    #[test]
    fn test_is_git_repository_true() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        assert!(is_git_repository(temp_dir.path()));
    }

    #[test]
    fn test_is_git_repository_false() {
        let temp_dir = TempDir::new().unwrap();
        assert!(!is_git_repository(temp_dir.path()));
    }

    #[test]
    fn test_detect_project_language_rust() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("main.rs", "fn main() {}"),
                ("lib.rs", "pub fn test() {}"),
                ("config.rs", "use serde::*;"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, Some("Rust".to_string()));
    }

    #[test]
    fn test_detect_project_language_javascript() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("index.js", "console.log('hello');"),
                ("app.js", "const x = 1;"),
                ("component.jsx", "export default function() {}"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, Some("JavaScript".to_string()));
    }

    #[test]
    fn test_detect_project_language_typescript() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("index.ts", "const x: number = 1;"),
                ("types.ts", "interface Test {}"),
                ("component.tsx", "export default function() {}"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, Some("TypeScript".to_string()));
    }

    #[test]
    fn test_detect_project_language_python() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("main.py", "print('hello')"),
                ("utils.py", "def test(): pass"),
                ("config.py", "import os"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, Some("Python".to_string()));
    }

    #[test]
    fn test_detect_project_language_mixed() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("main.rs", "fn main() {}"),
                ("script.js", "console.log('hello');"),
                ("helper.py", "print('world')"),
                ("another.rs", "pub fn test() {}"),
                ("third.rs", "use std::*;"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, Some("Rust".to_string()));
    }

    #[test]
    fn test_detect_project_language_none() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("README.md", "# Test"),
                ("config.txt", "some config"),
                ("data.json", "{}"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, None);
    }

    #[test]
    fn test_detect_project_language_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let language = detect_project_language(temp_dir.path());
        assert_eq!(language, None);
    }

    #[test]
    fn test_get_last_git_commit_time_no_git() {
        let temp_dir = TempDir::new().unwrap();
        let result = get_last_git_commit_time(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_get_git_remote_url_no_git() {
        let temp_dir = TempDir::new().unwrap();
        let result = get_git_remote_url(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_get_git_current_branch_no_git() {
        let temp_dir = TempDir::new().unwrap();
        let result = get_git_current_branch(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_get_git_status_no_git() {
        let temp_dir = TempDir::new().unwrap();
        let result = get_git_status(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_language_priority_with_equal_counts() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("main.rs", "fn main() {}"),
                ("script.js", "console.log('hello');"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert!(language.is_some());
        let lang = language.unwrap();
        assert!(lang == "Rust" || lang == "JavaScript");
    }

    #[test]
    fn test_special_languages() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(
            temp_dir.path(),
            &[
                ("test.go", "package main"),
                ("example.java", "public class Test {}"),
                ("demo.cpp", "#include <iostream>"),
            ],
        );

        let language = detect_project_language(temp_dir.path());
        assert!(language.is_some());
        let lang = language.unwrap();
        assert!(["Go", "Java", "C++"].contains(&lang.as_str()));
    }
}
