#!/bin/bash
# Cargo Integration Helper Script
# Provides compatibility layer between Cargo commands and Docker environment

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Check if we're in a cargo context
is_cargo_context() {
    [[ "${CARGO_PKG_NAME:-}" == "pm" ]] || [[ -f "$PROJECT_ROOT/Cargo.toml" ]]
}

# Check if Docker environment is available
is_docker_available() {
    command -v docker &> /dev/null && \
    command -v docker-compose &> /dev/null && \
    docker info &> /dev/null 2>&1
}

# Check if development container is running
is_dev_container_running() {
    docker-compose -f "$PROJECT_ROOT/docker-compose.yml" ps pm-dev | grep -q "Up"
}

# Start development container if not running
ensure_dev_container() {
    if ! is_dev_container_running; then
        echo "🐳 Starting development container..."
        cd "$PROJECT_ROOT"
        make docker-dev > /dev/null 2>&1
        
        # Wait for container to be ready
        local retries=10
        while [ $retries -gt 0 ] && ! is_dev_container_running; do
            sleep 1
            ((retries--))
        done
        
        if [ $retries -eq 0 ]; then
            echo "❌ Failed to start development container"
            return 1
        fi
    fi
}

# Execute command in development container
exec_in_container() {
    ensure_dev_container
    cd "$PROJECT_ROOT"
    docker-compose exec pm-dev "$@"
}

# Execute PM command in container
exec_pm_in_container() {
    exec_in_container pm "$@"
}

# Main function for cargo dev integration
cargo_dev_main() {
    if ! is_docker_available; then
        echo "❌ Docker is not available. Please install Docker and Docker Compose."
        echo "   Or use traditional cargo commands: cargo run --bin pm"
        exit 1
    fi
    
    local command="${1:-start}"
    shift 2>/dev/null || true
    
    case "$command" in
        "start"|"")
            cd "$PROJECT_ROOT"
            make docker-dev
            echo "💡 Use 'cargo dev shell' to connect to the container"
            ;;
        "shell"|"bash")
            exec_in_container bash
            ;;
        "pm")
            exec_pm_in_container "$@"
            ;;
        "build")
            exec_in_container cargo build --release
            ;;
        "test")
            exec_in_container cargo test
            ;;
        "check")
            exec_in_container cargo check
            ;;
        "fmt")
            exec_in_container cargo fmt
            ;;
        "clippy")
            exec_in_container cargo clippy
            ;;
        "init")
            exec_pm_in_container init
            ;;
        "status")
            cd "$PROJECT_ROOT"
            if is_dev_container_running; then
                echo "✅ Development container is running"
                docker-compose ps pm-dev
            else
                echo "❌ Development container is not running"
                echo "💡 Run 'cargo dev' to start it"
            fi
            ;;
        "logs")
            cd "$PROJECT_ROOT"
            docker-compose logs -f pm-dev
            ;;
        "stop")
            cd "$PROJECT_ROOT"
            docker-compose down
            echo "✅ Development container stopped"
            ;;
        "clean")
            cd "$PROJECT_ROOT"
            make docker-clean
            ;;
        "help"|"-h"|"--help")
            cat << EOF
cargo dev - Docker-integrated PM development

USAGE:
    cargo dev [COMMAND] [ARGS...]

COMMANDS:
    start, (none)     Start development environment
    shell, bash       Connect to development container
    pm [ARGS...]      Run PM command in container
    build             Build project in container
    test              Run tests in container
    check             Run cargo check in container
    fmt               Format code in container
    clippy            Run clippy in container
    init              Initialize PM in container
    status            Show container status
    logs              Show container logs
    stop              Stop development container
    clean             Clean Docker environment
    help              Show this help

EXAMPLES:
    cargo dev                    # Start development environment
    cargo dev shell              # Connect to container
    cargo dev pm init            # Initialize PM in container
    cargo dev pm add /workspace  # Add project in container
    cargo dev build              # Build in container
    cargo dev test               # Run tests in container

INTEGRATION:
    This integrates Docker development with Cargo workflow.
    All PM commands run in an isolated container environment.

FALLBACK:
    If Docker is unavailable, use traditional commands:
    cargo run --bin pm -- init
    cargo run --bin _pm -- init
EOF
            ;;
        *)
            echo "❌ Unknown command: $command"
            echo "💡 Use 'cargo dev help' for available commands"
            exit 1
            ;;
    esac
}

# Export functions for sourcing
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    # Script is being executed directly
    cargo_dev_main "$@"
fi