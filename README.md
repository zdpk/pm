# PM - Portable Project Manager CLI

`pm` is a Git project directory manager built around portable workspace manifests.
Instead of storing absolute project paths as the source of truth, PM stores:

- workspace roots
- project relative directories
- Git remotes

That makes it possible to move your PM config between VMs and restore the same workspace layout with `pm sync`.

## Features

- **Portable manifest** - Track projects in `manifest.json`
- **Workspace roots** - Every workspace can own a predictable root directory
- **Lazy restore** - Missing projects can be restored on `pm sw <project>`
- **Bulk restore** - `pm sync` restores missing repositories in parallel
- **Git-aware checks** - Detect missing repos, remote mismatches, and path conflicts
- **Repo spec version tracking** - Record which template/spec version applies to each project
- **Local port management** - Avoid frontend/backend/db/redis port conflicts across projects

## Installation

### From Source

```bash
cargo install --path . --bin pm
```

### Development Version

```bash
cargo install --path . --bin pmd
```

### Bundled Skills Plugin

The repo includes a bundled `skills` command plugin at
`plugins/commands/skills/`.

Install it into your active PM config with:

```bash
make install-skills-plugin
```

Or copy it manually:

```bash
mkdir -p ~/.config/pm/plugins/commands/skills
cp plugins/commands/skills/plugin.toml ~/.config/pm/plugins/commands/skills/
cp plugins/commands/skills/main.py ~/.config/pm/plugins/commands/skills/
```

## Quick Start

```bash
# Initialize PM
pm init

# Create a workspace with an explicit root
pm ws new work --root ~/work

# Add an existing repo under that workspace root
pm add ~/work/company-api

# List projects
pm ls

# Restore missing repos in bulk
pm sync

# Switch to a project
pm sw company-api
```

## Core Model

PM stores two files:

| File | Purpose |
|------|---------|
| `config.json` | Machine-local settings and current workspace/project |
| `manifest.json` | Portable workspace/project definition |
| `history.json` | Removal history snapshots for later restore workflows |

`manifest.json` is the portable source of truth. Each project stores:

- `workspace`
- `dir` relative to the workspace root
- `repo_slug`
- `remote` when available

The effective local path is always computed as:

```text
workspace.root + project.dir
```

## Shell Integration

Add this to `.bashrc` or `.zshrc`:

```bash
pm() {
    if [[ "$1" == "sw" || "$1" == "switch" ]] && [[ -n "$2" ]]; then
        local dir
        dir="$(command pm sw "$2")" && cd "$dir"
    else
        command pm "$@"
    fi
}
```

`pm sw` is interactive and may offer to restore a missing project.
`pm path` is non-interactive and fails if the directory is missing.

## Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `pm init` | | Initialize PM configuration |
| `pm add <path>` | | Register a project under the current workspace root |
| `pm list` | `ls` | List projects |
| `pm switch <project>` | `sw` | Switch to project directory, optionally restoring it |
| `pm path <project>` | | Print project path without restoring |
| `pm remove <project>` | `rm` | Unregister a project |
| `pm history` | | Show recent removal history |
| `pm use <workspace>` | | Switch workspace |
| `pm workspace` | `ws` | Workspace management |
| `pm sync [workspace]` | | Restore missing repositories in parallel |
| `pm manifest migrate` | | Migrate legacy `projects.json`/`workspaces.json` |
| `pm repo` | | Track project repo spec/template versions |
| `pm ports` | | Manage local project port allocations |
| `pm run` | | Run commands with port environment overrides |
| `pm check` | | Validate project health |
| `pm plugin` | | List, enable, or disable command plugins |
| `pm completion <shell>` | | Generate shell completions |

Installed command plugins can also expose top-level commands such as:

```bash
pm skills list
pm skills info sc
pm skills deploy --dry-run
```

## Repo Spec Version Tracking

Record which repo initialization spec version was applied to a project.

Specs are JSON files under `~/.config/pm/repo-specs/`. `pm init` creates the default `rust-axum-sqlx-backend` spec.

```bash
# List registered specs
pm repo spec list

# Show spec details
pm repo spec show rust-axum-sqlx-backend

# Track the current spec version for a project
pm repo track api --spec rust-axum-sqlx-backend

# Track an explicit version
pm repo track api --spec rust-axum-sqlx-backend --version 0.1.0

# Show project tracking status
pm repo status api
pm repo status              # Current-directory project
```

This feature tracks spec/template versions only. It does not render template files, run interactive scaffolding, or manage Git initial commits.

## Local Port Management

PM stores port allocations in `~/.config/pm/ports.json`. It does not rewrite `.env`; `pm run` injects values as environment variable overrides.

PM splits ports into two categories:

- **Per-project ports** for `frontend`, `backend`, and `infra`. Each project gets its own port from the configured range.
- **Shared infrastructure** for `database` (Postgres) and `redis`. A single local instance is shared across all projects, and isolation is achieved by database name and Redis key prefix.

Per-project default ranges:

```text
frontend  10000-19999
backend   20000-29999
infra     45000-49999
```

Shared infrastructure defaults:

```text
postgres  5432
redis     6379
```

### Per-project ports

```bash
# Assign a backend port (default kind)
pm ports assign api

# Assign frontend or infra explicitly
pm ports assign web --kind frontend
pm ports assign worker --kind infra

# `--kind database` and `--kind redis` are rejected — use `pm ports shared`

# List, check, repair, lock, release
pm ports list
pm ports check
pm ports check api
pm ports check --all
pm ports repair api
pm ports lock api --service back
pm ports release api
```

### Shared Postgres / Redis

```bash
# View current shared ports
pm ports shared

# Update one or both
pm ports shared --postgres 5433
pm ports shared --redis 6380
pm ports shared --postgres 5433 --redis 6380
```

Run a single Postgres + Redis container locally and point `pm` at their host ports. Each project gets a dedicated database name and Redis key prefix, so the same instance can serve many projects without conflict.

Example `docker-compose.yml`:

```yaml
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: postgres
    ports: ["5432:5432"]
    volumes: [pgdata:/var/lib/postgresql/data]

  redis:
    image: redis:7
    ports: ["6379:6379"]

volumes:
  pgdata:
```

### `pm run` environment variables

`pm run` always injects the following:

| Variable             | Value                                                                   |
| -------------------- | ----------------------------------------------------------------------- |
| `LOCAL_POSTGRES_PORT`| `shared.postgres_port` (e.g. `5432`)                                    |
| `DATABASE_URL`       | `postgres://postgres:postgres@127.0.0.1:{shared.postgres_port}/{db}`    |
| `LOCAL_REDIS_PORT`   | `shared.redis_port` (e.g. `6379`)                                       |
| `REDIS_URL`          | `redis://127.0.0.1:{shared.redis_port}`                                 |
| `REDIS_KEY_PREFIX`   | `{workspace}:{project}` — apply at the application layer for isolation  |
| `PM_WORKSPACE`       | The current workspace                                                   |
| `PM_PROJECT`         | The current project                                                     |

Where `{db}` is `{workspace}_{project}_local` with non-alphanumeric characters replaced by `_` and lowercased (e.g. workspace=`work`, project=`my-app` → `work_my_app_local`).

Per-project services additionally inject their environment variable (`APP_PORT`, `FRONTEND_PORT`, `LOCAL_INFRA_PORT`) and, for backend, `APP_HOST=127.0.0.1`.

```bash
pm run api -- cargo run
pm run -- npm run dev
```

### Migration from v1

If you previously ran a version of PM that allocated per-project Database/Redis ports, the first `pm ports` command on the new schema migrates `ports.json` from v1 to v2 automatically:

- The original file is backed up to `~/.config/pm/ports.json.bak.v1`.
- All per-project `database` / `redis` services are removed.
- Default `shared` values (`postgres_port: 5432`, `redis_port: 6379`) are injected.
- A one-line notice is printed to stderr.

After migration, point `DATABASE_URL` and `REDIS_URL` consumers at the shared instance and adopt the `REDIS_KEY_PREFIX` convention in app code.

Aliases, `/etc/hosts`, and reverse proxies are not managed.
## Workspaces

Create and manage workspace roots:

```bash
# Create a workspace with explicit root
pm ws new work --root ~/work

# Switch current workspace
pm use work

# Change the root later
pm ws root set work ~/src/work

# Move a project between workspaces
pm ws mv company-api personal

# List workspaces
pm ws list
```

By default, if a workspace has no explicit `root`, PM resolves it as:

```text
config.base_root/<workspace>
```

The default `base_root` is `~/`.

## Restore Workflow

### Bulk restore

On a new VM:

1. copy your `manifest.json`
2. keep or adjust `config.json`
3. run `pm sync`

If missing repositories are found, PM asks once whether to restore them. If you confirm, PM clones them in parallel into their expected workspace paths.

### Lazy restore

If you skip `pm sync`, PM can still restore on demand:

```bash
pm sw company-api
```

If the project is registered but missing locally, PM asks whether to restore it to the expected path and then switches into it.

## Removal Safety and History

`pm rm` now requires an explicit confirmation by default:

```bash
pm rm my-app
pm rm -f my-app
pm rm -rf my-app
pm rm -y my-app
```

Each variant prints the target action and only proceeds if you type exactly `y`.
For non-interactive environments, use `-y` or `--yes`.

Every successful removal action is appended to `history.json`, including:

- action type (`unregistered`, `trashed`, `deleted`)
- timestamp
- project snapshot
- resolved path at the time of removal

You can inspect recent entries with:

```bash
pm history
pm history --limit 50
```

`pm sync` also supports `-y` / `--yes` to restore missing repositories without prompting:

```bash
pm sync -y
pm sync work -y --jobs 8
```

## Listing and Health Checks

```bash
pm ls
pm ls --all
pm ls --tags rust,cli
pm ls --filter orphan
pm check
```

`pm check` distinguishes between:

- present
- missing but restorable
- missing and not restorable
- remote mismatch
- path conflict

## Configuration

Configuration is stored in:

- **macOS**: `~/Library/Application Support/pm/`
- **Linux**: `~/.config/pm/`
- **Windows**: `%APPDATA%\\pm\\`

### Environment Variables

| Variable | Description |
|----------|-------------|
| `PM_CONFIG_DIR` | Override config directory |

### Binary-based Config Separation

| Binary | Config Directory | Purpose |
|--------|------------------|---------|
| `pm` | Default location | Production |
| `pmd` | `pm-dev/` subdirectory | Development/Testing |

## Legacy Migration

If you already have `projects.json` and `workspaces.json`, PM will migrate them into `manifest.json` on load.

You can also run migration explicitly:

```bash
pm manifest migrate
```

## Building

```bash
make dev
make release
make install
make test
make fmt
make lint
```

## License

MIT
