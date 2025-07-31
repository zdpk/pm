# PM (Project Manager) Makefile
# Provides convenient commands for building and running production and development versions

.PHONY: build-prod run-prod clean install-prod help test docker-dev docker-test docker-build docker-clean docker-shell docker-logs docker-stop

# Default target
help:
	@echo "PM (Project Manager) Build Commands:"
	@echo ""
	@echo "Building:"
	@echo "  make build-prod    - Build production binary (pm)"
	@echo "  make build-all     - Build all binaries"
	@echo ""
	@echo "Running:"
	@echo "  make run-prod      - Run production binary"
	@echo ""
	@echo "Installing:"
	@echo "  make install-prod  - Install production binary"
	@echo ""
	@echo "Docker Development:"
	@echo "  make docker-dev    - Start Docker development environment"
	@echo "  make docker-shell  - Connect to development container"
	@echo "  make docker-test   - Run tests in Docker"
	@echo "  make docker-build  - Build Docker image"
	@echo "  make docker-clean  - Clean Docker containers and volumes"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make test          - Run tests"
	@echo ""
	@echo "Examples:"
	@echo "  make run-prod -- init         # Run 'pm init'"
	@echo "  make run-prod -- add /path    # Run 'pm add /path'"
	@echo "  make docker-shell             # Connect to dev container"
	@echo "  ./script/test-docker.sh       # Run tests in Docker"

# Build commands
build-prod:
	@echo "🔨 Building production binary..."
	cargo build --bin pm --release
	@echo "✅ Production binary built: target/release/pm"

build-all: build-prod

# Run commands
run-prod:
	@echo "🚀 Running production binary..."
	cargo run --bin pm -- $(filter-out $@,$(MAKECMDGOALS))

# Install commands
install-prod:
	@echo "📦 Installing production binary..."
	cargo install --path . --bin pm --force
	@echo "✅ Production binary installed as 'pm'"

# Test and maintenance
test:
	@echo "🧪 Running tests..."
	cargo test

clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean complete"

# Docker commands
docker-build:
	@echo "🐳 Building Docker image..."
	docker-compose build pm-dev
	@echo "✅ Docker image built"

docker-dev:
	@echo "🐳 Starting Docker development environment..."
	docker-compose up -d pm-dev
	@echo "✅ Development environment started"
	@echo "💡 Connect with: docker-compose exec pm-dev bash"

docker-shell:
	@./script/dev-shell.sh

docker-test:
	@./script/test-docker.sh

docker-logs:
	@echo "🐳 Showing development container logs..."
	docker-compose logs -f pm-dev

docker-stop:
	@echo "🐳 Stopping Docker containers..."
	docker-compose down

docker-clean:
	@echo "🐳 Cleaning Docker containers and volumes..."
	docker-compose down -v --rmi local
	docker system prune -f
	@echo "✅ Docker cleanup complete"

# Allow extra arguments to be passed to run commands
%:
	@:
