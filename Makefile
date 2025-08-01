# PM (Project Manager) Makefile
# Provides convenient commands for building and running production and development versions

.PHONY: build-prod run-prod clean install-prod help test docker-manual-build docker-manual docker-manual-quick docker-manual-clean bench test-coverage test-watch

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
	@echo "Testing & Quality:"
	@echo "  make test          - Run all tests"
	@echo "  make test-coverage - Generate test coverage report"
	@echo "  make test-watch    - Run tests in watch mode"  
	@echo "  make bench         - Run performance benchmarks"
	@echo ""
	@echo "Manual Testing:"
	@echo "  make docker-manual-quick  - Build binary + container and start test session"
	@echo "  make docker-manual-build  - Build production binary and Docker image"
	@echo "  make docker-manual        - Start manual testing container"
	@echo "  make docker-manual-clean  - Clean up manual testing container"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean         - Clean build artifacts and coverage reports"
	@echo ""
	@echo "Examples:"
	@echo "  make run-prod -- init           # Run 'pm init'"
	@echo "  make run-prod -- add /path      # Run 'pm add /path'"
	@echo "  make docker-manual-quick        # Quick manual testing"
	@echo "  make test-coverage              # Generate HTML coverage report"
	@echo "  make bench                      # Run performance benchmarks"

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

test-coverage:
	@echo "📊 Running coverage analysis..."
	@./script/test-coverage.sh

test-watch:
	@echo "👀 Running tests in watch mode..."
	@if command -v cargo-watch >/dev/null 2>&1; then \
		cargo watch -x test; \
	else \
		echo "❌ cargo-watch not installed. Install with: cargo install cargo-watch"; \
	fi

bench:
	@echo "🚀 Running benchmarks..."
	cargo bench

clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	rm -rf coverage/
	@echo "✅ Clean complete"

# Manual testing commands
docker-manual-build:
	@echo "🐳 Building manual test container with Linux binary..."
	docker build -f Dockerfile.manual -t pm-manual .
	@echo "✅ Manual testing container built"

docker-manual:
	@echo "🚀 Starting manual test container..."
	@echo "💡 PM binary is available at: /usr/local/bin/pm"
	@echo "💡 Type 'pm --help' to see available commands"
	@echo "💡 Type 'exit' to leave the container"
	@if [ -t 1 ]; then \
		docker run -it --rm \
			--name pm-manual-test \
			-v pm-manual-config:/home/pmuser/.config/pm \
			pm-manual; \
	else \
		echo "⚠️  No TTY detected. Use 'docker run -it --rm pm-manual' manually for interactive session"; \
		docker run --rm \
			--name pm-manual-test \
			-v pm-manual-config:/home/pmuser/.config/pm \
			pm-manual \
			bash -c "echo 'PM binary test:' && pm --version && echo 'Container ready. Use docker run -it --rm pm-manual for interactive session.'"; \
	fi

docker-manual-quick: docker-manual-build docker-manual

docker-manual-clean:
	@echo "🧹 Cleaning manual testing environment..."
	@docker rmi pm-manual 2>/dev/null || true
	@docker volume rm pm-manual-config 2>/dev/null || true
	@echo "✅ Manual testing cleanup complete"

# Allow extra arguments to be passed to run commands
%:
	@:
