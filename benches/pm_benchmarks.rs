use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pm::{Project, config::{Config, ConfigSettings}};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

fn create_test_config_with_projects(n: usize) -> Config {
    let mut config = Config {
        version: "1.0".to_string(),
        config_path: PathBuf::from("/tmp/bench"),
        settings: ConfigSettings::default(),
        projects: HashMap::new(),
        machine_metadata: HashMap::new(),
    };

    // Add n projects
    for i in 0..n {
        let project = Project {
            id: Uuid::new_v4(),
            name: format!("benchmark-project-{}", i),
            path: PathBuf::from(format!("/tmp/project-{}", i)),
            tags: vec![
                format!("tag-{}", i % 10),
                format!("category-{}", i % 5),
                format!("type-{}", i % 3),
            ],
            description: Some(format!("Benchmark project {}", i)),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            git_updated_at: None,
            is_git_repository: i % 2 == 0,
        };
        config.add_project(project);
    }

    config
}

fn bench_project_operations(c: &mut Criterion) {
    c.bench_function("add_single_project", |b| {
        b.iter(|| {
            let mut config = Config {
                version: "1.0".to_string(),
                config_path: PathBuf::from("/tmp/bench"),
                settings: ConfigSettings::default(),
                projects: HashMap::new(),
                machine_metadata: HashMap::new(),
            };

            let project = Project {
                id: Uuid::new_v4(),
                name: "benchmark-project".to_string(),
                path: PathBuf::from("/tmp/project"),
                tags: vec!["benchmark".to_string()],
                description: Some("Benchmark project".to_string()),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                git_updated_at: None,
                is_git_repository: false,
            };

            config.add_project(black_box(project));
        })
    });

    c.bench_function("access_tracking_single", |b| {
        let mut config = create_test_config_with_projects(1);
        let project_id = config.projects.keys().next().copied().unwrap();

        b.iter(|| {
            config.record_project_access(black_box(project_id));
        })
    });

    c.bench_function("access_tracking_batch_100", |b| {
        let mut config = create_test_config_with_projects(100);
        let project_ids: Vec<Uuid> = config.projects.keys().copied().collect();

        b.iter(|| {
            for &project_id in &project_ids {
                config.record_project_access(black_box(project_id));
            }
        })
    });

    c.bench_function("project_filtering_by_tags_small", |b| {
        let config = create_test_config_with_projects(100);
        
        b.iter(|| {
            let filtered: Vec<_> = config
                .projects
                .values()
                .filter(|p| p.tags.contains(&"tag-1".to_string()))
                .collect();
            black_box(filtered);
        })
    });

    c.bench_function("project_filtering_by_tags_large", |b| {
        let config = create_test_config_with_projects(10000);
        
        b.iter(|| {
            let filtered: Vec<_> = config
                .projects
                .values()
                .filter(|p| p.tags.contains(&"tag-1".to_string()))
                .collect();
            black_box(filtered);
        })
    });

    c.bench_function("project_search_by_name", |b| {
        let config = create_test_config_with_projects(1000);
        
        b.iter(|| {
            let found: Vec<_> = config
                .projects
                .values()
                .filter(|p| p.name.contains(black_box("project-500")))
                .collect();
            black_box(found);
        })
    });
}

fn bench_language_detection(c: &mut Criterion) {
    c.bench_function("detect_language_rust_project", |b| {
        let temp_dir = TempDir::new().unwrap();
        
        // Create test files
        for i in 0..20 {
            std::fs::write(
                temp_dir.path().join(format!("file_{}.rs", i)),
                "fn main() {}"
            ).unwrap();
        }
        
        for i in 0..5 {
            std::fs::write(
                temp_dir.path().join(format!("script_{}.js", i)),
                "console.log('hello');"
            ).unwrap();
        }

        b.iter(|| {
            let language = pm::utils::detect_project_language(black_box(temp_dir.path()));
            black_box(language);
        })
    });

    c.bench_function("detect_language_mixed_project", |b| {
        let temp_dir = TempDir::new().unwrap();
        
        // Create mixed language files
        let extensions = vec!["rs", "js", "py", "go", "java", "cpp", "ts", "rb"];
        for (i, ext) in extensions.iter().cycle().take(100).enumerate() {
            std::fs::write(
                temp_dir.path().join(format!("file_{}.{}", i, ext)),
                "// code"
            ).unwrap();
        }

        b.iter(|| {
            let language = pm::utils::detect_project_language(black_box(temp_dir.path()));
            black_box(language);
        })
    });
}

fn bench_config_operations(c: &mut Criterion) {
    c.bench_function("config_creation", |b| {
        b.iter(|| {
            let config = Config {
                version: "1.0".to_string(),
                config_path: PathBuf::from("/tmp/bench"),
                settings: ConfigSettings::default(),
                projects: HashMap::new(),
                machine_metadata: HashMap::new(),
            };
            black_box(config);
        })
    });

    c.bench_function("machine_id_generation", |b| {
        b.iter(|| {
            let machine_id = pm::config::get_machine_id();
            black_box(machine_id);
        })
    });

    c.bench_function("access_info_retrieval_large_config", |b| {
        let config = create_test_config_with_projects(10000);
        let project_id = config.projects.keys().next().copied().unwrap();

        b.iter(|| {
            let info = config.get_project_access_info(black_box(project_id));
            black_box(info);
        })
    });
}

criterion_group!(
    benches,
    bench_project_operations,
    bench_language_detection,
    bench_config_operations
);
criterion_main!(benches);