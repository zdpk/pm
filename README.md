# PM - Project Manager CLI

A fast, minimal CLI tool for managing Git project directories. Register, organize, and switch between projects with ease.

## Features

- **Project Registration** - Track projects across your system
- **Workspace Organization** - Group projects by context (work, personal, etc.)
- **Quick Navigation** - Switch between projects with shell integration
- **Git Status at a Glance** - See branch and change status in listings
- **Cross-Platform** - Works on macOS, Linux, and Windows

## Installation

### From Source

```bash
cargo install --path . --bin pm
```

### Development Version

```bash
# Separate config directory for testing
cargo install --path . --bin pmd
```

## Quick Start

```bash
# Initialize PM
pm init

# Add current directory as a project
pm add .

# Add a project with a custom name
pm add ~/projects/my-app --name my-app

# List all projects
pm ls

# Switch to a project (requires shell integration)
pm sw my-app
```

## Shell Integration

Add to your `.bashrc` or `.zshrc`:

```bash
pm() {
    if [[ "$1" == "sw" || "$1" == "switch" ]] && [[ -n "$2" ]]; then
        local dir
        dir="$(command pm path "$2")" && cd "$dir"
    else
        command pm "$@"
    fi
}
```

## Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `pm init` | | Initialize PM configuration |
| `pm add <path>` | | Register a project |
| `pm list` | `ls` | List projects |
| `pm switch <project>` | `sw` | Switch to project directory |
| `pm remove <project>` | `rm` | Unregister a project |
| `pm use <workspace>` | | Switch workspace |
| `pm workspace` | `ws` | Workspace management |
| `pm check` | | Validate project paths |
| `pm completion <shell>` | | Generate shell completions |

## Listing Projects

```bash
pm ls                    # Current workspace
pm ls --all              # All workspaces
pm ls --tags rust,cli    # Filter by tags
pm ls --sort name        # Sort by name
pm ls --no-status        # Skip git status (faster)
```

### Output Format

```
● default (3 projects)

  NAME       BRANCH         STATUS         LAST       PATH
* my-app     main           clean          2h ago     ~/projects/my-app
  api        develop        3 changed      1d ago     ~/projects/api
  docs       main           not git        5d ago     ~/projects/docs
```

### Sort Options

| Option | Description |
|--------|-------------|
| `accessed` | Last accessed (default) |
| `name` | Alphabetical |
| `path` | By path |
| `added` | Registration date |
| `frequency` | Access count |
| `status` | Git status (dirty first) |

## Workspaces

Organize projects into logical groups:

```bash
# Create a workspace
pm ws new work

# Switch workspace
pm use work

# Move project to workspace
pm ws mv my-app work

# List workspaces
pm ws list
```

### Workspace-specific Git Config

```bash
# Set git config for a workspace
pm ws config work user.email "john@company.com"
pm ws config work user.name "John Doe"

# Apply to all projects in workspace
pm ws apply-git work
```

## Configuration

Configuration is stored in:
- **macOS**: `~/Library/Application Support/pm/`
- **Linux**: `~/.config/pm/`
- **Windows**: `%APPDATA%\pm\`

### Files

| File | Description |
|------|-------------|
| `config.json` | Global settings |
| `projects.json` | Project registry |
| `workspaces.json` | Workspace data |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `PM_CONFIG_DIR` | Override config directory |

### Binary-based Config Separation

| Binary | Config Directory | Purpose |
|--------|------------------|---------|
| `pm` | Default location | Production |
| `pmd` | `pm-dev/` subdirectory | Development/Testing |

## Shell Completions

```bash
# Bash
pm completion bash > ~/.bash_completion.d/pm

# Zsh
pm completion zsh > ~/.zfunc/_pm

# Fish
pm completion fish > ~/.config/fish/completions/pm.fish
```

## Building

```bash
# Development build
make dev

# Release build
make release

# Install
make install

# Run tests
make test

# Format code
make fmt

# Lint
make lint
```

## License

MIT
