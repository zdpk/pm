#!/usr/bin/env cargo
//! Cargo integration for Docker-based PM development
//! 
//! This binary provides seamless integration between Cargo and Docker development environment.
//! Usage: cargo dev [COMMAND]

use std::env;
use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = env::args().collect();
    
    // Skip the binary name and "dev" if present
    let cmd_args: Vec<&str> = if args.len() > 1 && args[1] == "dev" {
        args.iter().skip(2).map(|s| s.as_str()).collect()
    } else {
        args.iter().skip(1).map(|s| s.as_str()).collect()
    };

    let command = cmd_args.get(0).unwrap_or(&"help");

    match *command {
        "shell" => run_docker_shell(),
        "test" => run_docker_test(),
        "build" => run_docker_build(),
        "init" => run_docker_init(),
        "clean" => run_docker_clean(),
        "logs" => run_docker_logs(),
        "stop" => run_docker_stop(),
        "help" | "--help" | "-h" => show_help(),
        _ => {
            // Default to starting development environment
            println!("🐳 Starting Docker development environment...");
            run_make_command("docker-dev");
        }
    }
}

fn run_docker_shell() {
    println!("🐳 Connecting to development container...");
    run_make_command("docker-shell");
}

fn run_docker_test() {
    println!("🐳 Running tests in Docker...");
    run_make_command("docker-test");
}

fn run_docker_build() {
    println!("🐳 Building in Docker container...");
    
    // First ensure container is running
    run_make_command("docker-dev");
    
    // Then run build inside container
    let status = Command::new("docker-compose")
        .args(&["exec", "pm-dev", "cargo", "build", "--release"])
        .status()
        .expect("Failed to execute docker-compose");

    if !status.success() {
        eprintln!("❌ Docker build failed");
        exit(1);
    }
    
    println!("✅ Docker build completed");
}

fn run_docker_init() {
    println!("🐳 Initializing PM in Docker container...");
    
    // Ensure container is running
    run_make_command("docker-dev");
    
    // Run pm init inside container
    let status = Command::new("docker-compose")
        .args(&["exec", "pm-dev", "pm", "init"])
        .status()
        .expect("Failed to execute docker-compose");

    if !status.success() {
        eprintln!("❌ PM initialization failed");
        exit(1);
    }
    
    println!("✅ PM initialized in container");
}

fn run_docker_clean() {
    println!("🐳 Cleaning Docker environment...");
    run_make_command("docker-clean");
}

fn run_docker_logs() {
    println!("🐳 Showing Docker logs...");
    run_make_command("docker-logs");
}

fn run_docker_stop() {
    println!("🐳 Stopping Docker containers...");
    run_make_command("docker-stop");
}

fn run_make_command(target: &str) {
    let status = Command::new("make")
        .arg(target)
        .status()
        .expect("Failed to execute make command");

    if !status.success() {
        eprintln!("❌ Make command failed: {}", target);
        exit(1);
    }
}

fn show_help() {
    println!("cargo dev - Docker-integrated PM development");
    println!();
    println!("USAGE:");
    println!("    cargo dev [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    (no command)  Start Docker development environment");
    println!("    shell         Connect to development container");
    println!("    test          Run tests in Docker");
    println!("    build         Build project in Docker");
    println!("    init          Initialize PM in container");
    println!("    clean         Clean Docker environment");
    println!("    logs          Show container logs");
    println!("    stop          Stop Docker containers");
    println!("    help          Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("    cargo dev              # Start development environment");
    println!("    cargo dev shell        # Connect to container");
    println!("    cargo dev test         # Run tests");
    println!("    cargo dev build        # Build in container");
    println!();
    println!("TRADITIONAL CARGO:");
    println!("    cargo prod -- init     # Run production binary");
    println!("    cargo debug -- init    # Run debug binary");
    println!();
    println!("For more information, see docs/DOCKER_DEVELOPMENT.md");
}