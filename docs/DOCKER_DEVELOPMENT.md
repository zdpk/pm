# Manual Testing with Docker

PM provides a lightweight Docker container for manual testing that allows you to test the PM binary in an isolated environment without affecting your local setup.

## Quick Start

```bash
# Build binary and start manual testing container
make docker-manual-quick

# Inside the container, PM is ready to use
pm --version
pm init
pm add /tmp/test-project
pm list
```

## Benefits

### 🔒 **Environment Isolation**
- Clean separation from your local PM installation
- Independent configuration and data
- Safe testing without side effects

### 🚀 **Lightweight & Fast**
- Minimal container with only runtime dependencies
- Quick startup time
- Small disk footprint

### 🛠️ **Simple Workflow**
- Single command to build and test
- Pre-built binary copied from host
- Easy cleanup when done

## Available Commands

### Manual Testing
```bash
make docker-manual-build    # Build binary and Docker image
make docker-manual          # Start manual testing container  
make docker-manual-quick    # Build + start in one command
make docker-manual-clean    # Clean up container and volumes
```

## Development Workflow

### 1. Quick Testing
```bash
# One command to build and test
make docker-manual-quick

# Inside container, test PM functionality
pm --version
pm init
pm add /tmp/test-project
pm list
```

### 2. Iterative Development
```bash
# Make code changes on host
# Build new binary
cargo build --release

# Rebuild container with new binary
make docker-manual-build

# Test the changes
make docker-manual
```

### 3. Cleanup
```bash
# Remove container and volumes when done
make docker-manual-clean
```

## Container Details

### Image Base
- **Base**: `debian:bookworm-slim`
- **User**: `pmuser` (non-root)
- **Working Directory**: `/home/pmuser`

### Installed Tools
- Git (for repository operations)
- curl (for downloads)
- ca-certificates (for HTTPS)

### Binary Location
- **PM Binary**: `/usr/local/bin/pm`
- **E2E Script**: `/usr/local/bin/run-e2e-tests.sh`

### Volume Mounts
- **PM Config**: `pm-manual-config:/home/pmuser/.config/pm` (persistent)

## Comparison: Manual Testing vs Local Development

| Aspect | Local Development | Docker Manual Testing |
|--------|------------------|----------------------|
| **Setup Complexity** | None (native) | Low (single command) |
| **Config Management** | Uses local config | Isolated config |
| **Environment Consistency** | Host-dependent | Containerized |
| **Development Speed** | Fastest (native) | Fast (cached builds) |
| **Isolation** | None (shared host) | Excellent |
| **CI/CD Alignment** | Manual verification | Automatic alignment |

## Troubleshooting

### Container Won't Start
```bash
# Check Docker status
docker --version

# Rebuild image
make docker-manual-clean
make docker-manual-build
```

### Permission Issues
```bash
# Container runs as 'pmuser' user (UID 1000)
# Check binary permissions
ls -la target/release/pm
```

### Build Failures
```bash
# Clean and rebuild
make docker-manual-clean
cargo build --release
make docker-manual-build
```

### Binary Not Found
```bash
# Ensure binary exists before building container
cargo build --release
ls -la target/release/pm
```

## Using Manual Testing Container

The manual testing container provides an isolated environment for testing PM functionality:

### 1. Build and Start Container
```bash
# Build release binary and start container
make docker-manual-quick
```

### 2. Test PM Functionality
```bash
# Inside container
pm --version
pm init
pm add /tmp/test-project
pm list
```

### 3. Clean Up When Done
```bash
# Exit container and clean up
exit
make docker-manual-clean
```

## Best Practices

### Manual Testing
1. **Build release binary first** - ensures testing actual release build
2. **Use isolated container** - prevents local environment interference
3. **Test critical workflows** - focus on user-facing functionality
4. **Clean up after testing** - prevents resource accumulation

### Development Workflow
1. **Develop locally** - faster iteration and debugging
2. **Test in container** - verify functionality in clean environment
3. **Use volume persistence** - for testing configuration changes
4. **Regular cleanup** - prevents disk space issues

### Configuration
1. **Separate test configs** - container uses isolated configuration
2. **Document test scenarios** - for consistent testing approach
3. **Use E2E scripts** - for automated verification

## Advanced Usage

### Custom Test Environment
```bash
# Run container with custom environment
docker run -it --rm \
  -e RUST_LOG=trace \
  -e PM_DEBUG=1 \
  -v pm-manual-config:/home/pmuser/.config/pm \
  pm-manual
```

### Testing with Custom Projects
```bash
# Mount local projects for testing (read-only)
docker run -it --rm \
  -v ~/my-projects:/home/pmuser/projects:ro \
  -v pm-manual-config:/home/pmuser/.config/pm \
  pm-manual
```

### E2E Testing in CI
```bash
# Manual testing container is also used in GitHub Actions
# for automated E2E testing with isolated environment
make docker-manual-build
docker run --rm pm-manual run-e2e-tests.sh
```