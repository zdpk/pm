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

Where `{db}` is `{workspace}_{project}` with non-alphanumeric characters replaced by `_` and lowercased (e.g. workspace=`work`, project=`my-app` → `work_my_app`). The trailing `_local` suffix used in v0.3.0 has been removed in v0.4.0.

Per-project services additionally inject their environment variable (`APP_PORT`, `FRONTEND_PORT`, `LOCAL_INFRA_PORT`) and, for backend, `APP_HOST=127.0.0.1`.

```bash
pm run api -- cargo run
pm run -- npm run dev
```

### Migration from v0.3.0 → v0.4.0

**Database name change (BREAKING)** — pm now uses `{workspace}_{project}` (no `_local` suffix) so the local database name matches production naming. To migrate existing data:

```bash
pg_dump work_api_local | psql work_api
```

The orchestrator emits a stderr notice the first time it sees a legacy `<ws>_<proj>_local` database and creates the new one alongside automatically.

### Migration from v1 (v0.3.0)

If you previously ran a version of PM that allocated per-project Database/Redis ports, the first `pm ports` command on the new schema migrates `ports.json` from v1 to v2 automatically:

- The original file is backed up to `~/.config/pm/ports.json.bak.v1`.
- All per-project `database` / `redis` services are removed.
- Default `shared` values (`postgres_port: 5432`, `redis_port: 6379`) are injected.
- A one-line notice is printed to stderr.

After migration, point `DATABASE_URL` and `REDIS_URL` consumers at the shared instance and adopt the `REDIS_KEY_PREFIX` convention in app code.

Aliases, `/etc/hosts`, and reverse proxies are not managed.

## Local Dev Orchestrator (v0.4.0)

`pm run` becomes an end-to-end dev environment when a project's `.project.yaml` defines a `services:` section. With one command, pm ensures shared Postgres + Redis are running, the per-project database exists, the routing daemon is alive, and your `front`/`back`/etc. services are spawned with `<service>.<project>.<workspace>.localhost` URLs that resolve through the daemon's reverse proxy.

> **Unix only in v0.4.0** (macOS / Linux). Windows users can still use the stateless v0.3.0 commands (`pm run -- <cmd>`, `pm ports`, `pm ws`, etc.).

### `.project.yaml` services schema

```yaml
language: ts
framework: nextjs
config_version: bundled

services:
  front:
    framework: nextjs        # uses pnpm + Turbopack convention
  back:
    framework: axum
    dir: backend             # spawn cwd, default "."
    dev_cmd: "cargo run"     # framework default if omitted
    port_kind: backend       # framework default if omitted
```

`pm proj init -l ts -f nextjs` writes a default `services: { front: { framework: nextjs } }` block. Use `--no-services` to skip.

### Commands

```bash
# Start all services (and ensure infra)
pm run

# Start one service
pm run front
pm run back

# Specific project
pm run back myproj

# Tail / stop
pm logs back
pm stop                       # stop all services in current project
pm stop front                 # one service

# Daemon
pm proxy status
pm proxy stop
pm proxy start --foreground   # debug

# Shared containers
pm db status
pm db start                   # ensure pm-local-db / pm-local-redis
pm db stop                    # graceful stop, volumes preserved
```

### Hostname routing

Services are reachable via `*.localhost` URLs through the proxy on `127.0.0.1:7100`:

```
front.api.work.localhost:7100  →  front service of work/api
back.api.work.localhost:7100   →  back service of work/api
front.blog.localhost:7100      →  front service of default/blog
                                  (default workspace gets a short alias)
```

`*.localhost` is auto-resolved to `127.0.0.1` by macOS / Linux / Windows DNS clients (RFC 6761), so no `/etc/hosts` edits are needed.

### Docker auto-start

`pm run` auto-creates and starts:
- `pm-local-db` container (default `postgres:16`, volume `pm-local-volume`, port 5432)
- `pm-local-redis` container (default `redis:7`, volume `pm-local-redis-volume`, port 6379)

If port 5432 or 6379 is already bound externally, pm respects that and does not start its own container. Containers persist across `pm run`/`pm stop`; only `pm db stop` shuts them down.

Disable Docker auto-start in `~/.config/pm/config.json`:

```json
{
  "dev": {
    "auto_start_docker": false,
    "proxy_port": 7100,
    "control_port": 7101,
    "postgres_image": "postgres:16",
    "redis_image": "redis:7"
  }
}
```

### Next.js convention: pnpm + Turbopack

For Next.js services, pm enforces:
- **pnpm** as the package manager (`engine-strict=true`, `auto-install-peers=true` via bundled `.npmrc`)
- **Turbopack** for dev builds (`pnpm next dev --turbopack` is the default `dev_cmd`)

`pm proj init -l ts -f nextjs` warns if `package-lock.json` / `yarn.lock` / `bun.lockb` is present.

### `pm run` grammar disambiguation

| Invocation                  | Behavior                                               |
| --------------------------- | ------------------------------------------------------ |
| `pm run`                    | All services in current project (orchestrator)         |
| `pm run front`              | Service `front` in current project                     |
| `pm run myproj`             | All services in project `myproj`                       |
| `pm run back api`           | Service `back` in project `api`                        |
| `pm run myproj -- cmd...`   | Legacy: arbitrary command (v0.3.0 grammar, preserved)  |
| `pm run -- cmd...`          | Legacy: arbitrary command in current project           |

The presence of `--` always selects legacy mode. `.project.yaml` without a `services:` section also keeps legacy behavior, so v0.3.0 users see no regression.

### Logs and rotation

Each spawned service writes stdout/stderr to `~/.config/pm/logs/<workspace>_<project>_<service>.log`. The file rotates to `.log.1` … `.log.3` at spawn time when it exceeds 10 MiB.
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

## Acknowledgements

Bundled `.gitignore` templates are derived from [`github/gitignore`](https://github.com/github/gitignore) and are distributed under the [Creative Commons Zero v1.0 Universal](LICENSES/github-gitignore-CC0.txt) (CC0 1.0) public domain dedication. The repository pins a specific commit via the `vendor/github-gitignore/` git submodule; maintainers refresh it on a release-by-release cadence.

## License

`pm` itself is MIT-licensed. See `LICENSES/` for upstream attributions.
