use anyhow::Result;
use pm::{Project, config::{Config, ConfigSettings}};
use std::collections::HashMap;
use tempfile::TempDir;
use uuid::Uuid;

/// Integration tests for PM core functionality
/// These tests verify that different components work together correctly

#[test]
fn test_project_lifecycle() -> Result<()> {
    // Create a temporary directory for testing
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config");
    
    // Initialize a new config
    let mut config = Config {
        version: "1.0".to_string(),
        config_path: config_path.clone(),
        settings: Default::default(),
        projects: HashMap::new(),
        machine_metadata: HashMap::new(),
    };

    // Create a test project
    let project = Project {
        id: Uuid::new_v4(),
        name: "integration-test-project".to_string(),
        path: temp_dir.path().join("test-project"),
        tags: vec!["test".to_string(), "integration".to_string()],
        description: Some("A test project for integration testing".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        git_updated_at: None,
        is_git_repository: false,
    };

    let project_id = project.id;

    // Test adding a project
    config.add_project(project);
    assert!(config.projects.contains_key(&project_id));

    // Test tracking access
    config.record_project_access(project_id);
    let (last_accessed, access_count) = config.get_project_access_info(project_id);
    assert!(last_accessed.is_some());
    assert_eq!(access_count, 1);

    // Test multiple access tracking
    config.record_project_access(project_id);
    config.record_project_access(project_id);
    let (_, access_count) = config.get_project_access_info(project_id);
    assert_eq!(access_count, 3);

    // Test project removal
    config.remove_project(project_id)?;
    assert!(!config.projects.contains_key(&project_id));

    // Verify access data is also cleaned up
    let (last_accessed, access_count) = config.get_project_access_info(project_id);
    assert!(last_accessed.is_none());
    assert_eq!(access_count, 0);

    Ok(())
}

#[test]
fn test_config_persistence_simulation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.yml");

    // Create initial config
    let mut config = Config {
        version: "1.0".to_string(),
        config_path: config_path.clone(),
        settings: Default::default(),
        projects: HashMap::new(),
        machine_metadata: HashMap::new(),
    };

    // Add multiple projects
    for i in 0..5 {
        let project = Project {
            id: Uuid::new_v4(),
            name: format!("project-{}", i),
            path: temp_dir.path().join(format!("project-{}", i)),
            tags: vec![format!("tag-{}", i % 3)],
            description: Some(format!("Test project {}", i)),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            git_updated_at: None,
            is_git_repository: i % 2 == 0,
        };
        config.add_project(project);
    }

    assert_eq!(config.projects.len(), 5);

    // Simulate tracking access for different projects
    let project_ids: Vec<Uuid> = config.projects.keys().copied().collect();
    for (i, &project_id) in project_ids.iter().enumerate() {
        for _ in 0..=i {
            config.record_project_access(project_id);
        }
    }

    // Verify access counts
    for (i, &project_id) in project_ids.iter().enumerate() {
        let (_, access_count) = config.get_project_access_info(project_id);
        assert_eq!(access_count, (i + 1) as u32);
    }

    Ok(())
}

#[test]
fn test_project_filtering_by_tags() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut config = Config {
        version: "1.0".to_string(),
        config_path: temp_dir.path().join("config"),
        settings: Default::default(),
        projects: HashMap::new(),
        machine_metadata: HashMap::new(),
    };

    // Create projects with different tag combinations
    let test_cases = vec![
        ("rust-project", vec!["rust", "backend"]),
        ("js-project", vec!["javascript", "frontend"]),
        ("fullstack-project", vec!["rust", "javascript", "fullstack"]),
        ("python-project", vec!["python", "backend"]),
        ("react-project", vec!["javascript", "react", "frontend"]),
    ];

    for (name, tags) in test_cases {
        let project = Project {
            id: Uuid::new_v4(),
            name: name.to_string(),
            path: temp_dir.path().join(name),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            description: Some(format!("Test project: {}", name)),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            git_updated_at: None,
            is_git_repository: false,
        };
        config.add_project(project);
    }

    // Test filtering by single tag
    let backend_projects: Vec<_> = config
        .projects
        .values()
        .filter(|p| p.tags.contains(&"backend".to_string()))
        .collect();
    assert_eq!(backend_projects.len(), 2); // rust-project, python-project

    let frontend_projects: Vec<_> = config
        .projects
        .values()
        .filter(|p| p.tags.contains(&"frontend".to_string()))
        .collect();
    assert_eq!(frontend_projects.len(), 2); // js-project, react-project

    // Test filtering by multiple tags (OR logic)
    let js_or_python_projects: Vec<_> = config
        .projects
        .values()
        .filter(|p| {
            p.tags.contains(&"javascript".to_string()) || p.tags.contains(&"python".to_string())
        })
        .collect();
    assert_eq!(js_or_python_projects.len(), 4); // js, fullstack, python, react

    // Test filtering by multiple tags (AND logic)
    let js_and_frontend_projects: Vec<_> = config
        .projects
        .values()
        .filter(|p| {
            p.tags.contains(&"javascript".to_string()) && p.tags.contains(&"frontend".to_string())
        })
        .collect();
    assert_eq!(js_and_frontend_projects.len(), 2); // js-project, react-project

    Ok(())
}

#[test]
fn test_machine_metadata_isolation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut config = Config {
        version: "1.0".to_string(),
        config_path: temp_dir.path().join("config"),
        settings: Default::default(),
        projects: HashMap::new(),
        machine_metadata: HashMap::new(),
    };

    // Create a test project
    let project = Project {
        id: Uuid::new_v4(),
        name: "multi-machine-project".to_string(),
        path: temp_dir.path().join("project"),
        tags: vec!["test".to_string()],
        description: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        git_updated_at: None,
        is_git_repository: false,
    };

    let project_id = project.id;
    config.add_project(project);

    // Simulate access from current machine
    config.record_project_access(project_id);
    config.record_project_access(project_id);

    let (last_accessed, access_count) = config.get_project_access_info(project_id);
    assert!(last_accessed.is_some());
    assert_eq!(access_count, 2);

    // Simulate total access count (should be same as single machine for now)
    let total_count = config.get_total_access_count(project_id);
    assert_eq!(total_count, 2);

    Ok(())
}

#[test]
fn test_config_settings_default_values() {
    let settings = ConfigSettings::default();
    
    // Test default values match expected behavior
    assert_eq!(settings.show_git_status, false);
    assert_eq!(settings.recent_projects_limit, 0);
}

#[test]
fn test_project_creation_with_git_detection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // Create a directory with .git folder
    let git_project_path = temp_dir.path().join("git-project");
    std::fs::create_dir_all(&git_project_path)?;
    std::fs::create_dir_all(git_project_path.join(".git"))?;
    
    // Create a directory without .git folder
    let non_git_project_path = temp_dir.path().join("non-git-project");
    std::fs::create_dir_all(&non_git_project_path)?;
    
    // Test git repository detection
    assert!(pm::utils::is_git_repository(&git_project_path));
    assert!(!pm::utils::is_git_repository(&non_git_project_path));
    
    Ok(())
}

#[test]
fn test_language_detection_comprehensive() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // Test cases for different languages
    let test_cases = vec![
        (vec![("main.rs", ""), ("lib.rs", "")], Some("Rust")),
        (vec![("index.js", ""), ("app.js", "")], Some("JavaScript")),
        (vec![("main.py", ""), ("utils.py", "")], Some("Python")),
        (vec![("main.go", ""), ("server.go", "")], Some("Go")),
        (vec![("Main.java", ""), ("Utils.java", "")], Some("Java")),
        (vec![("README.md", ""), ("config.json", "")], None),
    ];
    
    for (files, expected_language) in test_cases {
        let project_dir = temp_dir.path().join(format!("project-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&project_dir)?;
        
        // Create test files
        for (filename, content) in files {
            std::fs::write(project_dir.join(filename), content)?;
        }
        
        let detected_language = pm::utils::detect_project_language(&project_dir);
        assert_eq!(detected_language.as_deref(), expected_language);
    }
    
    Ok(())
}

#[test]
fn test_concurrent_project_access_simulation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut config = Config {
        version: "1.0".to_string(),
        config_path: temp_dir.path().join("config"),
        settings: Default::default(),
        projects: HashMap::new(),
        machine_metadata: HashMap::new(),
    };

    // Create multiple projects
    let mut project_ids = Vec::new();
    for i in 0..10 {
        let project = Project {
            id: Uuid::new_v4(),
            name: format!("concurrent-project-{}", i),
            path: temp_dir.path().join(format!("project-{}", i)),
            tags: vec!["concurrent".to_string()],
            description: Some(format!("Concurrent test project {}", i)),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            git_updated_at: None,
            is_git_repository: false,
        };
        project_ids.push(project.id);
        config.add_project(project);
    }

    // Simulate concurrent access patterns
    for (i, &project_id) in project_ids.iter().enumerate() {
        // Each project gets accessed i+1 times
        for _ in 0..=i {
            config.record_project_access(project_id);
        }
    }

    // Verify access patterns
    for (i, &project_id) in project_ids.iter().enumerate() {
        let (last_accessed, access_count) = config.get_project_access_info(project_id);
        assert!(last_accessed.is_some());
        assert_eq!(access_count, (i + 1) as u32);
    }

    // Test total access count
    let total_accesses: u32 = project_ids.iter()
        .map(|&id| config.get_total_access_count(id))
        .sum();
    
    let expected_total = (0..10).map(|i| i + 1).sum::<usize>() as u32;
    assert_eq!(total_accesses, expected_total);

    Ok(())
}