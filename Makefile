.PHONY: dev release install install-skills-plugin clean test check fmt

# Development build (debug)
dev:
	cargo build --bin pm
	cargo build --bin pmd

# Release build (optimized)
release:
	cargo build --release --bin pm
	cargo build --release --bin pmd

# Install to ~/.cargo/bin
install-dev:
	cargo install --path . --bin pmd --force

# Install to ~/.cargo/bin
install:
	cargo install --path . --bin pm --force

# Install the bundled skills plugin into the active PM config directory
install-skills-plugin:
	./scripts/install-skills-plugin.sh

# Run tests
test:
	cargo test

# Check without building
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Clean build artifacts
clean:
	cargo clean

# Run clippy lints
lint:
	cargo clippy -- -D warnings
