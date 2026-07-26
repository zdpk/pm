# PM - Portable Project Manager CLI

`pm` is a Git project directory manager built around a portable workspace manifest
with a **strict path convention** (v4): every project lives at a deterministic
path derived from its workspace and name. The manifest declares *what* PM
manages; the filesystem reflects *where* it lives, computed mechanically.

PM stores:

- a single configurable `base_root`
- workspace and project **names**
- Git remotes and metadata

That makes it possible to move your PM config between machines and restore the
same workspace layout with `pm sync`.

## Features

- **Portable manifest** - Track projects in `manifest.json`
- **Strict path convention** - Every project lives at `base_root/<workspace>/<project>`
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

# Create a workspace (lives at base_root/work)
pm ws new work

# Add a project — by URL (cloned into place) or by path (moved into place)
pm add https://github.com/acme/company-api.git
# or:
pm add ~/code/company-api          # → moved to base_root/work/company-api

# List projects
pm ls

# Restore missing repos in bulk
pm sync

# Switch to a project
pm sw company-api
```

On first initialization, PM stores `~/ws` as `base_root` and creates the
`default` and `.trash` workspace directories beneath it. If `~/ws` is already
in use, PM offers to use `~/.ws` instead. The selected root is persisted in
`config.json` and can be changed later.

## Core Model

PM stores two files:

| File | Purpose |
|------|---------|
| `config.json` | Machine-local settings and current workspace/project |
| `manifest.json` | Portable workspace/project definition |
| `history.json` | Removal history snapshots for later restore workflows |

`manifest.json` is the portable source of truth for **what** PM manages
(membership and metadata). The filesystem is the authority for **where** files
physically live; PM's reconciler keeps the two aligned (`pm sync`).

Each project stores:

- `workspace` — the workspace name
- `dir` — a single safe directory component (no `/`, `\`, `..`, etc.)
- `repo_slug`
- `remote` when available — a cached recovery URL refreshed explicitly with
  `pm sync --remotes`

The effective local path is computed deterministically as:

```text
<base_root>/<workspace>/<project.dir>
```

The default `base_root` is `~/ws`. It is the *only* configurable axis of
the path layout — workspaces no longer carry their own root override, and
`project.dir` cannot contain nested paths.

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
| `pm add <url\|path>` | | Add a project: clone a git URL, move a local directory, or create the directory when the supplied path is missing |
| `pm link <path>` | | Link an existing external git repo into the convention path via a symlink, without moving it (Unix-only) |
| `pm list` | `ls` | List projects (`--remote` shows each local repo's live `origin`) |
| `pm switch <project>` | `sw` | Switch to project directory, optionally restoring it |
| `pm path <project>` | | Print project path without restoring |
| `pm remove <project>` | `rm` | Unregister a project |
| `pm history` | | Show recent removal history |
| `pm use <workspace>` | | Switch workspace |
| `pm workspace` | `ws` | Workspace management |
| `pm sync [workspace]` | | Reconcile manifest vs. filesystem, or refresh cached origins with `--remotes` |
| `pm repo` | | Track project repo spec/template versions |
| `pm ports` | | Manage local project port allocations |
| `pm run` | | Run commands with port environment overrides |
| `pm check` | | Diagnose drift between manifest and filesystem (read-only) |
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

PM chooses each service's upstream port before spawning it, injects that port
into the service, and records a hostname → port projection in `routes.json`.
DNS only resolves the hostname to loopback; the single shared PM reverse proxy
reads the HTTP `Host` header and selects the actual upstream port. This is HTTP
request routing, not kernel NAT or generic TCP port mapping, so Postgres and
Redis continue to use their direct shared ports.

Open the read-only applications dashboard to see every service tracked by PM
across all workspaces and projects:

```text
http://pm.localhost:7100/
```

The configured `dev.proxy_port` replaces `7100`. `http://localhost:7100/` and
`http://127.0.0.1:7100/` serve the same page. The dashboard shows process and
route health but intentionally exposes no environment values, credentials,
logs, or lifecycle controls.

Detached orchestrator services are labeled **Background**. A legacy
`pm run -- <command>` invocation is registered only while its child is alive
and is labeled **Foreground**; PM stores the executable name but never command
arguments. PM uses the registered port kind to render frontend services as
clickable cards and lists backend, infrastructure, and foreground processes in
a compact non-navigable section below. A frontend card prefers its matching
Tailnet URL and falls back to its local route. Known frontend workspace/project
identities also show a versioned, embedded flat risograph-style illustration and
a concise product description inside a restrained paper-and-ink editorial
layout; unknown projects use a neutral fallback visual. Supporting process rows
do not show navigation addresses. When Tailscale Serve is installed, the
dashboard reads its current loopback proxy mappings and shows matching Tailnet
addresses on frontend cards. The page refreshes every five seconds while idle,
defers refresh while an interactive control has focus, and exposes a pause
control for longer inspection.

PM always binds the dashboard to loopback. To put a trusted Tailnet reverse
proxy in front of it, allow the machine's exact MagicDNS hostname and expose the
proxy port with Tailscale Serve:

```json
{
  "dev": {
    "dashboard_hosts": ["my-machine.example.ts.net"]
  }
}
```

```bash
tailscale serve --bg --https=9443 http://127.0.0.1:7100
```

Wildcard Tailnet hosts are not accepted; only exact configured hostnames are
treated as dashboard requests.

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

## Bundled `.gitignore` templates (v0.5.0)

`pm project init` and `pm project sync` now synthesize `.gitignore` from [`github/gitignore`](https://github.com/github/gitignore) (CC0) embedded at build time, layered with framework-specific extras. The result lives inside a fenced block; everything outside is yours to edit:

```
# user-edited content above the marker is left alone
my-secrets/
*.local

# >>> pm managed (do not edit; run `pm project gitignore` to refresh) >>>
# === OS: macOS ===
# Source: https://github.com/github/gitignore (CC0)
.DS_Store
...

# === Language: Node ===
# Source: https://github.com/github/gitignore (CC0)
node_modules/
...

# === Framework: nextjs ===
# Source: pm bundled framework extras
.next/
.vercel
# <<< pm managed <<<

# user content below the marker is also preserved
team-only.txt
```

### `pm project gitignore` command

```bash
# Refresh in place (used by init / sync automatically)
pm project gitignore

# Preview without writing
pm project gitignore --diff

# Override the auto-derived selection
pm project gitignore --categories rust,macos,vscode
```

Default category selection:
- **OS**: macos, linux, windows (always — for team-safety in cross-platform repos)
- **IDE**: vscode, jetbrains
- **Language**: derived from `.project.yaml`'s `language` (rust → Rust, ts/typescript → Node, python → Python, dart → Dart, go → Go)
- **Framework extras**: `configs/<lang>/<framework>/.gitignore.extra` if present (currently nextjs, nestjs, fastapi, flutter)

`--categories` overrides the default selection entirely. Valid keys: `macos`, `linux`, `windows`, `vscode`, `jetbrains`, `rust`, `node`, `python`, `dart`, `go`. Case-insensitive.

### Migration from v0.4.x

Existing `.gitignore` files keep their pre-v0.5.0 lines until the first sync. On that run, pm detects a fixed set of v0.4.x bundled patterns (`/target`, `node_modules/`, `__pycache__/`, etc.) in the user region and moves them into the new managed block. A one-line stderr notice reports the count. Custom user lines that don't match the legacy set are never touched.

The marker text is a stable contract — changing it requires a future BREAKING release.

## Workspaces

Workspace directories are derived from their name; PM creates `base_root/<name>/`
automatically and projects live one level deeper.

```bash
# Create a workspace (directory: base_root/work)
pm ws new work

# Switch current workspace
pm use work

# Move a project between workspaces (relocates files + updates manifest)
pm ws mv company-api personal

# List workspaces
pm ws list
```

A workspace's path is always:

```text
<base_root>/<workspace_name>
```

The default `base_root` is `~/ws`; change it in `config.json`. There is
no per-workspace root override — that was removed in v4 to make manifests
portable and paths deterministic.

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

### Drift reconciliation

The manifest is `pm`'s source of truth for **what** is managed; the filesystem
is the truth for **what physically exists**. `pm` surfaces drift between the
two rather than silently reconciling. Two commands work together:

```bash
# Read-only diagnostic. Lists missing entries, conflicts, and untracked dirs
# under base_root that are not in the manifest. Makes no changes.
pm check

# Mutating reconciler. Default behavior (no flags) is unchanged: restore
# missing entries that have a remote. Opt-in flags add the other two
# directions:
pm sync --adopt           # register untracked git dirs (per-item confirm)
pm sync --prune           # drop manifest entries whose dir is missing
                          # (writes history.json snapshot)
pm sync --adopt --prune --dry-run   # preview the plan without applying
pm sync --adopt --prune --yes       # non-interactive; required outside a TTY
```

Adopt skips entries whose name fails the project-name regex, whose workspace
is not declared in the manifest (run `pm ws new <name>` first), or whose name
collides with an existing project. Prune appends each removed project to
`history.json` as `Unregistered` so `pm history` can surface it.

### Remote metadata

The local repository's `origin` is authoritative while its checkout exists.
The manifest's `remote` is a portable recovery URL used when `pm` needs to
restore a missing project.

```bash
pm ls --remote                 # live origins in the current workspace
pm ls --all --remote           # live origins in every workspace
pm sync --remotes --dry-run    # preview Git → manifest metadata updates
pm sync --remotes --yes        # apply all updates without prompting
pm sync work --remotes --yes   # update one workspace
```

Remote listing reads each registered local repository directly and never
fetches the network or changes Git/manifest state. A live `origin` takes
precedence; when the checkout or origin is unavailable, the output retains the
cached value and marks it `(saved)`.

`pm sync --remotes` is an explicit metadata-only mode, separate from
restore/adopt/prune. It copies changed or missing safe origins into the
manifest and refreshes the derived `repo_slug`, but never modifies the Git
remote itself. Missing paths, non-Git directories, and repositories without an
`origin` keep their existing cached value instead of clearing it. The exact
local `origin` URL is copied so changes such as HTTPS → SSH are reflected in
future restore operations. Eligible origins must be independent of the
directory from which a future restore runs: HTTP(S), SSH, Git-protocol and
scp-style remotes are supported. Relative local paths and unsupported remote
helper forms are skipped, preserving any previously saved value. An absolute
local path (or `file:///...` URL) is accepted because it is cwd-independent,
but it remains machine-local and will only restore while that path is
available on the same machine.

## Linked Projects

Sometimes a git repo must stay where it already lives — a shared checkout, a
mount, or a path other tools depend on. `pm link` registers such a repo by
placing a **symlink** at the convention path instead of moving it:

```bash
# Link an external repo into the current workspace (Unix-only).
pm link /mnt/shared/widgets
#   creates base_root/<workspace>/widgets  →  /mnt/shared/widgets
#   and records the project with that canonical link target.

pm link /mnt/shared/widgets --workspace labs --name gizmo
```

The external directory is never moved or copied — `pm` manages only the
symlink and the manifest entry. Because the project still resolves at
`base_root/<workspace>/<dir>`, the strict path convention stays intact.

Link safety and lifecycle:

- **`pm link` requires a git repository** and aborts if the convention path is
  already occupied or the name collides — leaving disk and manifest untouched.
- **`pm rm <linked>` removes only the symlink** (never the external target),
  regardless of `-f`/`-r`.
- **`pm mv <linked>` relocates the symlink** to the new convention path and
  preserves the link target; the external directory is not moved.
- **Links are machine-local.** A linked project does not exist on another
  machine: there is no remote to clone, so `pm sync` there simply leaves it
  absent (`pm check` reports it as a link unavailable on this machine).

`pm check` distinguishes link drift from ordinary drift: `broken-link`
(symlink or target missing), `link-drift` (resolves to the wrong path),
`link-replaced-by-dir` (a real directory took the convention path), and
`unexpected-symlink` (a managed project's path is unexpectedly a symlink).

`pm sync` **repairs recoverable links** as part of its normal run: when the
recorded target still exists, it recreates a missing symlink (`broken-link`) or
corrects a drifted one (`link-drift`) so the convention path resolves to the
target again. Repair touches only the symlink — `pm sync` never clones a linked
project from a remote, never deletes the external target, and never removes a
real directory. Unrecoverable cases stay manual and are only reported: a missing
target (including on another machine) and `link-replaced-by-dir`. Repairs appear
in `pm sync --dry-run` and are confirmed like restores (`--yes` to skip prompts).
`pm check` marks a `broken-link`/`link-drift` as *repairable* when its target
exists, pointing you at `pm sync`.

## Listing and Health Checks

```bash
pm ls
pm ls --all
pm ls --remote
pm ls --all --remote
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

## v4 Compatibility

Manifest schema v4 introduced the strict path convention and intentionally
breaks compatibility with earlier versions. PM does **not** ship an automatic
migration: loading a `manifest.json` with `version < 4` (or one that uses a
per-workspace `root` field or a nested `project.dir`) fails with a clear
incompatibility error. Recreate your workspaces under the new convention.

This is a one-time break tied to the v0.x development phase; future schema
bumps will ship migrations.

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
