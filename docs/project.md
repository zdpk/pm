# pm project — Config File Management

Centrally manage and sync config files (linters, formatters, Dockerfile, CI/CD, etc.) across projects.

## Commands

| Command | Alias | Description |
|---------|-------|-------------|
| `pm project init` | `pm p init` | Initialize project with config files |
| `pm project sync` | `pm p sync` | Sync config files to latest version |
| `pm project check` | `pm p check` | Check if config files are outdated |
| `pm project diff` | `pm p diff` | Show diff between local and upstream |
| `pm project update` | `pm p update` | Pull latest config repo |
| `pm project add` | `pm p add` | Register project without copying files |
| `pm project list` | `pm p list`, `pm p ls` | List managed projects |

---

## Supported Languages & Frameworks

### Rust

| Framework | ID | Config Files |
|-----------|----|-------------|
| Axum | `axum` | clippy.toml, rustfmt.toml, Cargo.toml (template), Dockerfile |
| Clap (CLI) | `clap` | clippy.toml, rustfmt.toml, Cargo.toml (template), cargo-dist settings |

**Auto-detect markers:** `Cargo.toml`
**Framework detection:** scans `Cargo.toml` dependencies for `axum` or `clap`

### TypeScript

| Framework | ID | Config Files |
|-----------|----|-------------|
| Next.js | `nextjs` | eslint.config.js, tsconfig.json, .prettierrc, next.config.js, Dockerfile |
| NestJS | `nestjs` | eslint.config.js, tsconfig.json, .prettierrc, nest-cli.json, Dockerfile |

**Auto-detect markers:** `package.json`
**Framework detection:** `next.config.*` → Next.js, `nest-cli.json` → NestJS

### Python

| Framework | ID | Config Files |
|-----------|----|-------------|
| FastAPI | `fastapi` | pyproject.toml, ruff.toml, Dockerfile |

**Auto-detect markers:** `pyproject.toml`, `requirements.txt`
**Framework detection:** scans `pyproject.toml` dependencies for `fastapi`

### Dart

| Framework | ID | Config Files |
|-----------|----|-------------|
| Flutter | `flutter` | analysis_options.yaml, pubspec.yaml (template) |

**Auto-detect markers:** `pubspec.yaml`
**Framework detection:** scans `pubspec.yaml` for `flutter`

### C

| Framework | ID | Config Files |
|-----------|----|-------------|
| (general) | — | .clang-format, .clang-tidy, Makefile/CMakeLists.txt (template) |

**Auto-detect markers:** `Makefile`, `CMakeLists.txt`, `*.c`
**Framework detection:** none

---

## Shared Config Files

Language-agnostic files included via `--ci`, `--docker`, `--hooks`, or `--all`:

| Category | Flag | Files |
|----------|------|-------|
| Common | (always) | .editorconfig, .gitignore |
| CI/CD | `--ci` | .github/workflows/ |
| Docker | `--docker` | .dockerignore, Dockerfile |
| Hooks | `--hooks` | .pre-commit-config.yaml |

---

## File Strategies

Each config file has a sync strategy declared in the config repo's `manifest.yaml`:

| Strategy | On `init` | On `sync` | Use case |
|----------|-----------|-----------|----------|
| **managed** | Copy from config repo | Overwrite completely | Files that must stay identical across projects (rustfmt.toml, .editorconfig) |
| **merged** | Copy from config repo | Append missing lines only | Files with project-specific additions (.gitignore, Dockerfile) |
| **template** | Copy from config repo | Skip (never touched) | Files that diverge per project (Cargo.toml, package.json) |

---

## Per-Project Config: `.project.yaml`

Created by `pm project init` or `pm project add` in the project root:

```yaml
language: rust
framework: axum
config_version: a3f2b1c
includes:
- ci
- docker
```

| Field | Description |
|-------|-------------|
| `language` | Language ID (rust, ts, python, dart, c) |
| `framework` | Framework ID (axum, clap, nextjs, etc.) or omitted |
| `config_version` | Config repo commit hash at last sync |
| `includes` | Selected extras: ci, docker, hooks |

---

## Config Repo Structure

The config repo is a separate Git repository that stores canonical config files:

```
proj-config/
├── manifest.yaml               # Supported languages & frameworks
├── rust/
│   ├── common/
│   │   ├── manifest.yaml       # File strategies
│   │   ├── clippy.toml
│   │   └── rustfmt.toml
│   └── axum/
│       ├── manifest.yaml
│       └── Dockerfile
├── typescript/
│   ├── common/
│   │   ├── manifest.yaml
│   │   ├── eslint.config.js
│   │   └── tsconfig.json
│   └── nextjs/
│       └── manifest.yaml
├── python/
│   └── ...
├── dart/
│   └── ...
├── c/
│   └── ...
└── shared/
    ├── manifest.yaml
    ├── .editorconfig
    ├── .gitignore
    ├── ci/
    ├── docker/
    └── hooks/
```

### Root `manifest.yaml`

```yaml
meta:
  version: 1

languages:
  - id: rust
    name: Rust
    markers: [Cargo.toml]
    frameworks: [axum, clap]
  - id: ts
    name: TypeScript
    markers: [package.json]
    frameworks: [nextjs, nestjs]
  - id: python
    name: Python
    markers: [pyproject.toml, requirements.txt]
    frameworks: [fastapi]
  - id: dart
    name: Dart
    markers: [pubspec.yaml]
    frameworks: [flutter]
  - id: c
    name: C
    markers: [Makefile, CMakeLists.txt, "*.c"]
    frameworks: []
```

### Per-directory `manifest.yaml`

```yaml
files:
  - path: rustfmt.toml
    strategy: managed
  - path: .gitignore
    strategy: merged
  - path: Cargo.toml
    strategy: template
```

---

## Setup

Add config repo URL to PM's `config.json` (`~/.config/pm/config.json`):

```json
{
  "config_repo": {
    "url": "https://github.com/your-user/proj-config.git",
    "cache_dir": "~/.config/pm/config-repo"
  }
}
```

Then fetch the config repo:

```bash
pm project update
```

---

## Usage Examples

### Initialize a new project (interactive)

```bash
cd ~/projects/my-api
pm project init
```

### Initialize (non-interactive)

```bash
pm project init -l rust -f axum --all -y
pm project init -l ts -f nextjs --ci --docker -y
```

### Check all projects

```bash
pm project check --all
```

```
✗ my-api (rust/axum) — outdated (3 files changed)
  - clippy.toml
  - Dockerfile
  - .github/workflows/ci.yml
✓ my-frontend (ts/nextjs) — up to date
```

### Sync current project

```bash
pm project sync
pm project sync --dry-run    # preview only
```

### Sync all projects

```bash
pm project sync --all
```

### View diff

```bash
pm project diff
```

### List managed projects

```bash
pm project list
```

```
NAME             STACK            PATH                             CONFIG
my-api           rust/axum        ~/projects/my-api                a3f2b1c
my-frontend      ts/nextjs        ~/projects/my-frontend           d4e5f6g
```

---

## CLI Reference

### `pm project init`

```
pm project init [OPTIONS]

Options:
  -l, --language <LANG>     Language: rust, ts, python, dart, c
  -f, --framework <FW>      Framework: axum, clap, nextjs, nestjs, fastapi, flutter
      --ci                  Include CI/CD workflows
      --docker              Include Dockerfile
      --hooks               Include pre-commit hooks
      --all                 Include everything (ci + docker + hooks)
  -y, --no-interactive      Skip all prompts
```

### `pm project sync`

```
pm project sync [OPTIONS]

Options:
      --all       Sync all registered projects
      --dry-run   Preview changes without applying
```

### `pm project check`

```
pm project check [OPTIONS]

Options:
      --all       Check all registered projects
```

### `pm project diff`

```
pm project diff
```

### `pm project update`

```
pm project update
```

### `pm project add`

```
pm project add [OPTIONS]

Options:
  -l, --language <LANG>     Language
  -f, --framework <FW>      Framework
```

### `pm project list`

```
pm project list
```
