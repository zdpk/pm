# Docker Development Environment

PM provides a Docker-based development environment that eliminates the need for separate `pm` and `_pm` binaries with different configuration paths.

## Quick Start

```bash
# Build and start development environment
make docker-dev

# Connect to the container
make docker-shell

# Inside the container, PM is ready to use
pm init
pm add /workspace
pm list
```

## Benefits

### 🔒 **Environment Isolation**
- Clean separation between development and production
- No more confusion between `pm` and `_pm` binaries
- Isolated configuration files

### 🚀 **Consistent Development**
- Same environment for all developers
- Matches CI/CD environment
- Pre-configured with all dependencies

### 🛠️ **Simplified Workflow**
- Single `pm` binary in container
- No need to manage multiple config files
- Easy testing without affecting local setup

## Available Commands

### Development Environment
```bash
make docker-build    # Build Docker image
make docker-dev      # Start development container
make docker-shell    # Connect to running container
make docker-stop     # Stop containers
make docker-clean    # Clean up containers and volumes
```

### Testing
```bash
make docker-test     # Run tests in container
make docker-logs     # View container logs
```

## Development Workflow

### 1. Initial Setup
```bash
# Clone repository
git clone https://github.com/zdpk/pm.git
cd pm

# Start development environment
make docker-dev
make docker-shell
```

### 2. Inside Container
```bash
# Initialize PM (creates /home/developer/.config/pm/config.yml)
pm init

# Add test project
pm add /workspace

# Test functionality
pm list
pm config show
```

### 3. Code Changes
- Edit files on host machine
- Changes are automatically synced to container via volume mount
- Rebuild and test inside container:

```bash
# Inside container
cargo build --release
./target/release/pm --version
```

### 4. Testing
```bash
# Run tests in container
make docker-test

# Or run specific tests inside container
cargo test config::tests::test_config_creation
```

## Container Details

### Image Base
- **Base**: `rust:1.80-slim`
- **User**: `developer` (non-root)
- **Working Directory**: `/workspace`

### Installed Tools
- Rust toolchain (1.80)
- Git
- GitHub CLI (`gh`)
- Build essentials (cmake, pkg-config, etc.)

### Volume Mounts
- **Source Code**: `.` → `/workspace` (live sync)
- **Cargo Registry**: Cached for faster builds
- **Target Directory**: Cached builds
- **Git Config**: `~/.gitconfig` → `/home/developer/.gitconfig` (read-only)
- **SSH Keys**: `~/.ssh` → `/home/developer/.ssh` (read-only)
- **PM Config**: Persistent storage for PM configuration

### Environment Variables
- `CARGO_TARGET_DIR=/workspace/target`
- `RUST_LOG=debug`

## Comparison: Docker vs Dual Binary

| Aspect | Dual Binary (`pm`/`_pm`) | Docker Development |
|--------|--------------------------|-------------------|
| **Setup Complexity** | Medium (manage 2 binaries) | Low (single command) |
| **Config Management** | Complex (2 config files) | Simple (isolated) |
| **Environment Consistency** | Host-dependent | Containerized |
| **Development Speed** | Fast (native) | Fast (cached builds) |
| **Isolation** | Poor (shared host) | Excellent |
| **CI/CD Alignment** | Manual sync | Automatic |

## Troubleshooting

### Container Won't Start
```bash
# Check Docker status
docker --version
docker-compose --version

# Rebuild image
make docker-clean
make docker-build
```

### Permission Issues
```bash
# Container runs as 'developer' user (UID 1000)
# If you have permission issues, check volume mounts
ls -la ~/.ssh
ls -la ~/.gitconfig
```

### Build Failures
```bash
# Clean and rebuild
make docker-clean
make docker-build

# Check logs
make docker-logs
```

### Network Issues
```bash
# GitHub CLI authentication inside container
docker-compose exec pm-dev gh auth login
```

## Migration from Dual Binary Setup

If you're currently using `pm`/`_pm` setup:

### 1. Backup Current Config
```bash
# Backup existing configs
cp ~/.config/pm/config.yml ~/.config/pm/config-prod-backup.yml
cp ~/.config/pm/config-dev.yml ~/.config/pm/config-dev-backup.yml
```

### 2. Start Fresh with Docker
```bash
make docker-dev
make docker-shell

# Inside container
pm init
# Manually add your projects or import from backup
```

### 3. Verify Migration
```bash
# Inside container
pm list
pm config show
```

## Best Practices

### Development
1. **Always use Docker for development** - eliminates environment issues
2. **Use volume mounts** - for real-time code syncing
3. **Commit from host** - Git operations work better on host
4. **Test in container** - ensures consistency

### Testing
1. **Run tests in container** - matches CI environment
2. **Use cached volumes** - for faster rebuilds
3. **Clean up regularly** - prevents disk space issues

### Configuration
1. **Keep configs in container** - prevents local interference
2. **Use environment variables** - for container-specific settings
3. **Document custom setups** - for team consistency

## Advanced Usage

### Custom Environment Variables
```yaml
# Add to docker-compose.yml
services:
  pm-dev:
    environment:
      - RUST_LOG=trace
      - PM_DEBUG=1
      - CUSTOM_VAR=value
```

### Additional Volume Mounts
```yaml
# Add to docker-compose.yml
services:
  pm-dev:
    volumes:
      - ~/my-projects:/home/developer/projects:ro
```

### Multi-Stage Development
```bash
# Development stage
make docker-dev

# Testing stage  
make docker-test

# Production build (on host)
make build-prod
```