#!/bin/bash
# PM Docker Development Environment Setup Script
# Automates the setup of Docker-based development environment

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check Docker
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed. Please install Docker first."
        echo "Visit: https://docs.docker.com/get-docker/"
        exit 1
    fi
    
    # Check Docker Compose
    if ! command -v docker-compose &> /dev/null; then
        log_error "Docker Compose is not installed. Please install Docker Compose first."
        echo "Visit: https://docs.docker.com/compose/install/"
        exit 1
    fi
    
    # Check Make
    if ! command -v make &> /dev/null; then
        log_error "Make is not installed. Please install Make first."
        exit 1
    fi
    
    # Check if Docker daemon is running
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running. Please start Docker first."
        exit 1
    fi
    
    log_success "All prerequisites satisfied"
}

# Setup development environment
setup_dev_environment() {
    log_info "Setting up Docker development environment..."
    
    # Build Docker image
    log_info "Building Docker image..."
    make docker-build
    
    # Start development container
    log_info "Starting development container..."
    make docker-dev
    
    # Wait for container to be ready
    sleep 3
    
    # Initialize PM in container
    log_info "Initializing PM in container..."
    docker-compose exec -T pm-dev pm init || {
        log_warning "PM initialization failed, container might not be ready yet"
        log_info "You can manually run 'cargo dev init' later"
    }
    
    log_success "Development environment setup complete!"
}

# Show usage information
show_usage() {
    echo "PM Docker Development Setup"
    echo ""
    echo "This script sets up a Docker-based development environment for PM."
    echo ""
    echo "USAGE:"
    echo "    ./script/dev-setup.sh [OPTIONS]"
    echo ""
    echo "OPTIONS:"
    echo "    -h, --help     Show this help message"
    echo "    -c, --check    Only check prerequisites"
    echo "    -f, --force    Force rebuild of Docker image"
    echo ""
    echo "EXAMPLES:"
    echo "    ./script/dev-setup.sh           # Full setup"
    echo "    ./script/dev-setup.sh --check   # Check prerequisites only"
    echo "    ./script/dev-setup.sh --force   # Force rebuild"
}

# Show post-setup instructions
show_instructions() {
    echo ""
    log_success "🎉 PM Docker development environment is ready!"
    echo ""
    echo "QUICK START:"
    echo "  cargo dev            # Start development environment"
    echo "  cargo dev shell      # Connect to container"
    echo "  cargo dev test       # Run tests in container"
    echo "  cargo dev build      # Build in container"
    echo ""
    echo "TRADITIONAL WORKFLOW:"
    echo "  make docker-shell    # Connect to container"
    echo "  make docker-test     # Run tests"
    echo "  make docker-stop     # Stop containers"
    echo ""
    echo "INSIDE CONTAINER:"
    echo "  pm init              # Initialize PM"
    echo "  pm add /workspace    # Add current project"
    echo "  pm list              # List projects"
    echo ""
    echo "For more information:"
    echo "  📖 docs/DOCKER_DEVELOPMENT.md"
    echo "  🆘 cargo dev help"
}

# Main script
main() {
    case "${1:-}" in
        -h|--help)
            show_usage
            exit 0
            ;;
        -c|--check)
            check_prerequisites
            log_success "Prerequisites check passed!"
            exit 0
            ;;
        -f|--force)
            log_info "Force rebuild requested"
            check_prerequisites
            make docker-clean
            setup_dev_environment
            show_instructions
            ;;
        "")
            check_prerequisites
            setup_dev_environment
            show_instructions
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
}

# Trap to handle interrupts
trap 'log_error "Setup interrupted by user"; exit 1' INT TERM

# Run main function
main "$@"