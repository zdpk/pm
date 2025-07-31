# PM (Project Manager)

A fast, terminal-based project management CLI tool written in Rust. PM helps developers efficiently manage and switch between multiple projects with zero friction.

## Features

### Core Project Management
- **Interactive Setup**: Guided initialization with auto-detection and GitHub integration
- **Fast Project Switching**: Switch between projects in under 1 second
- **Smart Project Discovery**: Automatic detection of Git repositories and programming languages
- **Intuitive Tag Selection**: Two-step interface with smart filtering and flexible tag creation
- **GitHub Integration**: Clone and manage repositories directly from GitHub
- **Advanced Configuration**: Backup, templates, export/import, and validation
- **Machine-Specific Metadata**: Track project access and usage across different machines
- **Fast Directory Switching**: Instant navigation to project directories
- **Shell Integration**: Automatic shell setup for Fish, Bash, and Zsh with directory changing
- **Starship Prompt Integration**: Show project info in your terminal prompt with one command
- **Rich CLI Interface**: Colorful output with progress indicators and interactive prompts

### 🔌 Extension System
- **Modular Architecture**: Lightweight core with extensible functionality
- **No-Prefix Commands**: Extensions integrate seamlessly (`pm docker ps` not `pm-ext-docker`)
- **Multiple Execution Methods**: Run extensions via `pm run`, `pm r`, or fast `pmr` shell alias
- **Extension Discovery**: Built-in help and listing (`pm run ls`, `pm run help`)
- **Easy Installation**: One-command extension installation and management
- **Developer-Friendly**: Simple extension development in any language
- **Cross-Platform**: Extensions work on Linux, macOS, and Windows

### Official Extensions (Available)
- **pm-ext-hooks**: Enhanced Git hooks with `.githook/` directory support
- **pm-ext-direnv**: Auto-activation direnv environment management  
- **pm-ext-1password**: Secure `.pw.yml` → `.env` file generation

### Remote Extension Registry (Planned)
- **Extension Search**: `pm ext search <query>` to find extensions
- **Remote Installation**: `pm ext install <name>` from central registry
- **Registry Management**: Multiple registry support with `pm ext registry` commands
- **Automatic Updates**: Keep extensions up-to-date with `pm ext update`

> **Migration Notice**: Built-in hooks and direnv features have been moved to extensions. See [Migration Guide](docs/MIGRATION_GUIDE.md) for upgrade instructions.

## Installation

### Option 1: Quick Install Script (Recommended)

**macOS (Apple Silicon)**:
```bash
curl -fsSL https://github.com/zdpk/project-manager/releases/latest/download/install.sh | sh
```

### Option 2: Manual Download

Download the latest binary from the [Releases page](https://github.com/zdpk/project-manager/releases):

**macOS (Apple Silicon)**:
```bash
curl -L https://github.com/zdpk/project-manager/releases/latest/download/pm-aarch64-apple-darwin -o pm
chmod +x pm
sudo mv pm /usr/local/bin/
```

> **Note**: Currently only macOS Apple Silicon is supported.

### Option 4: Install via npm (Recommended for Node.js users)

If you have Node.js and npm installed, you can install `pm` globally or use `npx` for temporary execution.

**Global Installation (recommended for frequent use):**
```bash
npm install -g @zdpk/pm
```
After global installation, you can run `pm` directly from your terminal:
```bash
pm --help
```

**Temporary Execution (for one-off use or testing):**
```bash
npx @zdpk/pm --help
```

### Option 3: Build from Source

#### Prerequisites

- Rust 1.70+ (for building from source)
- Git

```bash
# Clone the repository
git clone https://github.com/zdpk/project-manager.git
cd project-manager

# Build the project
cargo build --release

# Install to your system
cargo install --path .
```

## Quick Start

```bash
# Interactive initialization - sets up PM configuration
pm init

# Add a project (with intuitive two-step tag selection)
pm add ~/my-projects/awesome-app

# Add current directory with enhanced experience  
pm add .

# Add all subdirectories in current folder
pm add *

# Scan for existing repositories
pm scan

# Clone GitHub repositories (interactive browse)
pm clone

# Clone specific repository
pm clone owner/repo

# List all projects
pm list

# List projects with filters (aliases work too)
pm ls --tags rust --recent 7d --limit 10

# Switch to a project directory
pm switch my-project

# Using alias
pm sw my-project

# Check project status (useful for prompt integration)
pm status

# Extension management
pm ext list                    # List installed extensions
pm ext install hooks           # Install hooks extension
pm ext install direnv          # Install direnv extension  
pm ext install 1password       # Install 1Password extension

# Run extensions (multiple ways)
pm run a example              # Explicit extension execution
pm r a example                # Using alias 'r'
pmr a example                 # Using shell alias (fastest)

# Extension discovery and help
pm run ls                     # List all installed extensions
pm run help                   # Show extension usage help
pmr ls                        # List extensions (shell alias)

# Use extensions (no prefix required)
pm hooks status               # Check Git hooks status
pm direnv activate            # Auto-activate direnv
pm 1password generate dev     # Generate .env.dev from .pw.yml
```

### Initial Setup Example

When you run `pm init`, you'll be guided through an interactive setup:

```bash
$ pm init
🚀 Initializing PM...

🔍 Detecting GitHub username from GitHub CLI...
✅ Detected GitHub username: your-username
> Use detected GitHub username 'your-username'? Yes
> Configuration directory: ~/.config/pm
  Where PM configuration files will be stored (press Enter for default)
> Show git status in project listings? Yes

📂 Creating configuration directory: /Users/you/.config/pm

✅ PM initialized successfully
📂 Config directory: /Users/you/.config/pm
⚙️ Config file: /Users/you/.config/pm/config.yml


🎯 Next steps:
  pm add .                       # Add current directory with interactive tags
  pm add *                       # Add all subdirectories
  pm scan                        # Scan for existing repositories
  pm clone <owner>/<repo>        # Clone specific repository
  pm clone                       # Browse and select repositories

📖 Use 'pm --help' to see all available commands
```

> **Note**: PM automatically detects your GitHub username from GitHub CLI if you're authenticated. If detection fails, you can still enter it manually.

## Command Reference

All commands support aliases shown in parentheses. Use `pm --help` for complete usage information.

### Project Management

```bash
# Add projects with intuitive two-step tag selection (alias: a)
pm add <path>                                   # Add specific directory
pm add . --name "My Project"                   # Add current directory with custom name
pm add ~/code/api --description "REST API"     # Add with description
pm add *                                        # Add all subdirectories (batch mode)
pm add my-project                               # Create and add new project

# List projects (alias: ls)
pm list                                         # List all projects
pm ls --tags rust,backend                      # Filter by tags (AND logic) 
pm ls --tags-any frontend,web                  # Filter by tags (OR logic)
pm ls --recent 7d                               # Show recent activity (7 days)
pm ls --detailed                                # Show detailed information

# Switch projects (alias: sw)
pm switch <name>                                # Switch to project directory
pm sw <name>                                    # Switch to project (alias)

# Remove projects (alias: rm)
pm rm                                           # Interactive project selection
pm rm <name>                                    # Remove project by name
pm rm <name> -y                                 # Remove without confirmation
```

### GitHub Integration

```bash
# Clone repositories (alias: cl)
pm clone                                        # Interactive browse your repositories
pm clone microsoft/vscode                      # Clone specific repository
pm clone owner/repo --directory ~/custom       # Clone to custom directory

# Scan for repositories (alias: sc)
pm scan                                         # Scan current directory
pm scan ~/Development                           # Scan specific directory
pm scan --show-all                             # Show all found repositories
```

### Two-Step Tag Selection Interface

When adding projects, PM provides an intuitive two-step workflow:

**Step 1 - Choose Your Action:**
```
? What would you like to do?
  > Create Project [project-name] (without tags)
    Add tags to this project
    Create new tag and add to project
```

**Step 2 - Smart Tag Selection:**
```
🏷️ Select tags for this project (type to filter):
  [ ] frontend (8 projects)
  [ ] react (5 projects)
  [ ] typescript (6 projects)
```

**Key Improvements:**
- **🎯 Clear separation**: Actions vs. tag selection
- **⚡ No confusion**: Simple select for actions, checkboxes for tags
- **🔍 Real-time filtering**: Type to find tags instantly
- **✨ Flexible workflow**: Create new, select existing, or skip tags
- **📊 Usage insights**: See project counts for each tag

**Example Workflow:**

```bash
$ pm add ./react-dashboard

? What would you like to do?
  > Add tags to this project

🏷️ Select tags for this project (type to filter): react
  [x] react (5 projects)

✅ Successfully added project 'react-dashboard' with tags: react
```

For more detailed examples and workflows, see [TAG_SELECTION_GUIDE.md](docs/TAG_SELECTION_GUIDE.md).


### Tag Management

```bash
# Manage tags (alias: t)
pm tag add <project> <tags...>                 # Add tags to project
pm tag remove <project> <tags...>              # Remove tags from project
pm tag list                                     # List all available tags
pm tag show [project]                          # Show tags for project
```

### Configuration

```bash
# View and edit configuration (alias: cf)
pm config                              # Show current configuration (default)
pm config show                         # Show current configuration  
pm config edit                         # Edit configuration file
pm config validate                     # Validate configuration file

# Get and set values
pm config get settings.show_git_status # Get specific value
pm config set settings.show_git_status true # Set specific value
pm config list                         # List all available keys

# Backup and restore
pm config backup create [name]         # Create configuration backup
pm config backup restore <name>        # Restore from backup
pm config backup list                  # List available backups
pm config backup delete <name>         # Delete backup

# Templates
pm config template list                # List available templates
pm config template apply <name>        # Apply a template
pm config template save <name>         # Save current config as template

# Import/Export
pm config export --format yaml         # Export configuration
pm config import <file>                # Import configuration
pm config diff [backup]                # Show configuration differences
pm config history --limit 10           # Show configuration history
```

## Configuration

PM stores its configuration in a configurable location (default: `~/.config/pm/config.yml`). The configuration includes:

- **Configuration path**: Where PM stores its configuration files (configurable during init)
- **Application settings**: Show git status, recent projects limit, etc.
- **Project data**: All managed projects and their metadata
- **Machine metadata**: Access tracking across different machines

### Example Configuration

```yaml
version: "1.0"
config_path: "/Users/you/.config/pm"
settings:
  show_git_status: true
  recent_projects_limit: 10
projects: {}
machine_metadata: {}
```

### Configuration Setup

During `pm init`, you can customize:

- **Configuration Directory**: Where PM stores its files (default: `~/.config/pm`)
- **Git Status Display**: Whether to show git status information in project listings

## Advanced Usage

### Filtering and Search

```bash
# Complex filtering
pm ls --tags rust,cli --recent 30d --limit 5

# Find projects by pattern
pm s my-proj    # Suggests similar project names if not found
```

### Shell Integration

PM provides automatic shell integration that allows `pm sw` to actually change your shell's current directory (not just the PM process directory). **Shell integration is automatically set up during `pm init`**.

#### Automatic Setup During Init
```bash
pm init
🚀 Initializing PM...
📂 Configuration directory: ~/.config/pm
🐚 Show git status in project listings? › Yes  
🔧 Setup Zsh shell integration for directory switching? › Yes
   Detected shell: Zsh
🐚 Zsh shell integration installed successfully
   Function file: ~/.config/pm/pm.zsh
   Added to: ~/.zshrc
✅ PM initialized successfully!
```

#### Supported Shells

**Fish Shell**
- ✅ **Native autoloading** - uses Fish's function system (`~/.config/fish/functions/pm.fish`)
- ✅ **Automatic detection** - no manual setup required
- ✅ **Conflict handling** - backup/remove options for existing functions

**Zsh Shell** 
- ✅ **Separate config file** - `~/.config/pm/pm.zsh` + automatic `.zshrc` sourcing
- ✅ **Automatic detection** - no manual setup required  
- ✅ **Conflict handling** - backup options for existing files

**Bash Shell**
- ✅ **Separate config file** - `~/.config/pm/pm.bash` + automatic `.bashrc` sourcing
- ✅ **Automatic detection** - no manual setup required
- ✅ **Conflict handling** - backup options for existing files

#### How It Works
Once integrated, `pm sw` will:
1. Execute the PM switch command
2. Parse the output for "Switched to: /path/to/project"  
3. Change your shell's current directory to that path
4. Display confirmation: "📁 Changed directory to: /path/to/project"
```

### Workflow Integration

```bash
# Integration with other tools (with shell integration)
pm sw my-project && npm start
pm sw api-service && docker-compose up -d

# Useful aliases
alias pml="pm ls"
alias pms="pm switch"
alias pma="pm add ."
```

### Multiple Machine Sync

PM automatically tracks which machine you last accessed each project on, making it easy to work across multiple development environments.

### Starship Prompt Integration

PM integrates seamlessly with [Starship](https://starship.rs/) to show project information in your terminal prompt using the `pm status` command.

#### Quick Setup

1. **Install Starship** (if not already installed):
   ```bash
   curl -sS https://starship.rs/install.sh | sh
   ```

2. **Add PM configuration** to your `~/.config/starship.toml`:
   ```toml
   [custom.pm]
   command = '''pm status --format json --quiet | jq -r "
     if .git_branch != \"\" then
       if .git_changes then .name + \" [\" + .git_branch + \"*]\"
       else .name + \" [\" + .git_branch + \"]\"
       end
     else .name
     end
   " 2>/dev/null || echo ""'''
   when = "pm status --quiet"
   format = "📁 [$output](bold blue) "
   description = "Show PM project with git status"
   ```

3. **Restart your shell** or reload configuration:
   ```bash
   exec $SHELL
   ```

#### What You'll See

Once configured, your prompt will show project information:
```bash
~/projects/my-app 📁 my-app [main*] ❯
```

#### Alternative Configurations

**Minimal (project name only):**
```toml
[custom.pm]
command = 'pm status --format json --quiet | jq -r ".name" 2>/dev/null || echo ""'
when = "pm status --quiet"
format = "📁 [$output](bold blue) "
```

**Simple (without jq dependency):**
```toml
[custom.pm]
command = 'pm status --quiet'
when = "pm status --quiet"
format = "📁 [$output](bold blue) "
```

For complete Starship integration documentation, see [docs/STARSHIP_INTEGRATION.md](docs/STARSHIP_INTEGRATION.md).

## Project Structure

```
project-manager/
├── src/
│   ├── commands/           # Command implementations
│   │   ├── config.rs      # Configuration management
│   │   ├── init.rs        # Interactive initialization
│   │   ├── project.rs     # Project operations
│   │   └── tag.rs         # Tag management
│   ├── config.rs          # Configuration types and loading
│   ├── display.rs         # Output formatting and colors
│   ├── validation.rs      # Input validation
│   ├── utils.rs           # Utility functions
│   └── main.rs            # CLI entry point
├── docs/                  # Comprehensive documentation
│   ├── ARCHITECTURE.md    # System architecture
│   ├── CLI_USAGE.md       # Detailed CLI guide
│   ├── COMMANDS.md        # Command reference
│   ├── CONFIG_PATHS.md    # Configuration file structure
│   └── MIGRATION_GUIDE.md # Version migration guide
├── schemas/               # JSON schemas for validation
├── script/                # Installation scripts
├── .github/workflows/     # CI/CD workflows
└── target/                # Build artifacts
```

## Development

### 🐳 Docker Development (Recommended)

For a consistent, isolated development environment:

```bash
# Quick start - one command setup
./script/dev-setup.sh

# Or use cargo integration:
cargo dev              # Start Docker development environment
cargo dev shell        # Connect to container
cargo dev pm init      # Initialize PM in container
cargo dev test         # Run tests in container
cargo dev build        # Build in container
```

**Benefits:**
- ✅ Isolated environment (no conflicts with system PM)
- ✅ Consistent across different machines
- ✅ Pre-configured with all dependencies
- ✅ Single `pm` binary (no `pm`/`_pm` confusion)

See [Docker Development Guide](docs/DOCKER_DEVELOPMENT.md) for complete details.

### Traditional Development

#### Running from Source

```bash
# Run PM binary
cargo run --bin pm -- <command>

# Examples
cargo run --bin pm -- init
cargo run --bin pm -- ls --detailed
cargo run --bin pm -- add . --tags rust,cli
```

#### Testing

```bash
# Docker environment (recommended)
cargo dev test                    # Run tests in container

# Traditional environment
cargo test                        # Run all tests
cargo test -- --nocapture         # Run with output
cargo test test_name               # Run specific test
```

#### Building

```bash
# Docker environment (recommended)
cargo dev build                   # Build in container
cargo dev fmt                     # Format code in container
cargo dev clippy                  # Run lints in container

# Traditional environment
cargo build                       # Debug build
cargo build --release             # Release build
cargo fmt --all -- --check        # Check formatting
cargo clippy --all-targets --all-features -- -D warnings   # Run clippy
```

#### Development Workflow Comparison

| Task | Docker Method | Traditional Method |
|------|---------------|-------------------|
| **Setup** | `./script/dev-setup.sh` | Manual dependency install |
| **Initialize** | `cargo dev pm init` | `cargo run --bin pm -- init` |
| **Test** | `cargo dev test` | `cargo test` |
| **Build** | `cargo dev build` | `cargo build --release` |
| **Config File** | Single `config.yml` | Single `config.yml` |

## Troubleshooting

### Common Issues

**PM not initialized error**:
```bash
pm init  # Run the initialization wizard
```

**Project not found**:
```bash
pm ls    # List all projects to see available names
```

**GitHub connectivity issues**:
```bash
gh auth status                    # Check GitHub CLI authentication
pm clone                          # Try interactive repository browsing
```

**Configuration issues**:
```bash
pm config validate  # Check configuration validity
pm config edit       # Manually edit configuration
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Check formatting (`cargo fmt --all -- --check`)
6. Run clippy (`cargo clippy --all-targets --all-features -- -D warnings`)
7. Commit your changes (`git commit -m 'Add amazing feature'`)
8. Push to the branch (`git push origin feature/amazing-feature`)
9. Open a Pull Request

## License

MIT License - see the [LICENSE](LICENSE) file for details.

## Version

Current version: 0.1.1

Built with ❤️ in Rust