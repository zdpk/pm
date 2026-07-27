.PHONY: dev release install install-dev install-skills-plugin clean test check fmt \
	dashboard-install dashboard-build dashboard-test dashboard-lint dashboard-verify

# Development build (debug)
dev: dashboard-build
	cargo build --bin pm
	cargo build --bin pmd

# Release build (optimized)
release: dashboard-build
	cargo build --release --bin pm
	cargo build --release --bin pmd

# Install to ~/.cargo/bin
install-dev: dashboard-build
	cargo install --path . --bin pmd --force

# Install to ~/.cargo/bin
install: dashboard-build
	cargo install --path . --bin pm --force

# Install the bundled skills plugin into the active PM config directory
install-skills-plugin:
	./scripts/install-skills-plugin.sh

# Install dashboard dependencies
dashboard-install:
	pnpm --dir dashboard-ui install --frozen-lockfile

# Build the embedded shadcn dashboard assets
dashboard-build: dashboard-install
	pnpm --dir dashboard-ui build

# Run dashboard component tests
dashboard-test: dashboard-install
	pnpm --dir dashboard-ui test

# Run dashboard lints
dashboard-lint: dashboard-install
	pnpm --dir dashboard-ui lint

# Verify committed dashboard assets match the source
dashboard-verify: dashboard-test dashboard-lint dashboard-build
	git diff --exit-code -- dashboard-ui/dist

# Run all tests
test: dashboard-test
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
