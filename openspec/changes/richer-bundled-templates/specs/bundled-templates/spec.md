## ADDED Requirements

### Requirement: github/gitignore vendoring
The `pm` source repository SHALL include `github/gitignore` as a git submodule under `vendor/github-gitignore/` pinned to a specific commit. The submodule SHALL be checked out automatically during CI builds (`actions/checkout@v4` with `submodules: true`). Maintainers MAY refresh the pinned commit via `git submodule update --remote vendor/github-gitignore && git commit`.

#### Scenario: cold checkout fetches the submodule
- **WHEN** a contributor clones the repository with `git clone --recursive` or runs `git submodule update --init --recursive`
- **THEN** `vendor/github-gitignore/Rust.gitignore` exists and is non-empty

#### Scenario: build.rs detects missing submodule and errors clearly
- **GIVEN** `vendor/github-gitignore/` is empty (e.g. submodule not initialized)
- **WHEN** `cargo build` runs
- **THEN** the build fails with a message naming the missing submodule and the recovery command (`git submodule update --init --recursive`)

### Requirement: Build-time embedding of selected gitignore categories
At compile time, `build.rs` SHALL emit a Rust module to `OUT_DIR` exposing each embedded category as a `&'static str` and an aggregate `ALL_CATEGORIES: &[(&str, &str)]` slice. The module SHALL be re-emitted whenever any file under `vendor/github-gitignore/` changes (`cargo:rerun-if-changed`).

The initial set of embedded categories SHALL be:
| key (lowercase) | source file in `github/gitignore` |
|---|---|
| `macos` | `Global/macOS.gitignore` |
| `linux` | `Global/Linux.gitignore` |
| `windows` | `Global/Windows.gitignore` |
| `vscode` | `Global/VisualStudioCode.gitignore` |
| `jetbrains` | `Global/JetBrains.gitignore` |
| `rust` | `Rust.gitignore` |
| `node` | `Node.gitignore` |
| `python` | `Python.gitignore` |
| `dart` | `Dart.gitignore` |
| `go` | `Go.gitignore` |

#### Scenario: every category resolves to non-empty content at runtime
- **WHEN** the binary is invoked with `pm project gitignore --categories rust`
- **THEN** the output contains substantive Rust ignore rules drawn from `Rust.gitignore`

#### Scenario: category key is case-insensitive
- **WHEN** the user runs `pm project gitignore --categories RUST,MacOS`
- **THEN** the same content is produced as `--categories rust,macos`

#### Scenario: unknown category yields a clear error
- **WHEN** the user runs `pm project gitignore --categories notalanguage`
- **THEN** the command exits non-zero with a message listing valid categories

### Requirement: pm-managed block in `.gitignore`
A `.gitignore` file produced or maintained by `pm project init` / `sync` / `gitignore` SHALL embed the synthesized template content inside a clearly delimited region:

```
# >>> pm managed (do not edit; run `pm project gitignore` to refresh) >>>
... synthesized lines ...
# <<< pm managed <<<
```

`pm` SHALL only read or modify the content between these markers. Any content outside the markers (above, between, or below) is the user's region and SHALL be preserved byte-for-byte by `pm project sync` and `pm project gitignore`.

#### Scenario: user lines outside the marker are preserved
- **GIVEN** a `.gitignore` containing user lines, then a marker block, then more user lines
- **WHEN** `pm project gitignore` runs
- **THEN** the user lines above and below the marker block are unchanged
- **AND** the content inside the marker block is replaced with the freshly synthesized output

#### Scenario: missing marker block is appended
- **GIVEN** a `.gitignore` without any pm marker
- **WHEN** `pm project gitignore` runs for the first time
- **THEN** the existing content is preserved
- **AND** a new pm-managed marker block with the synthesized content is appended at the end of the file (preceded by a blank line)

#### Scenario: empty existing file
- **GIVEN** the project has no `.gitignore`
- **WHEN** `pm project init` runs
- **THEN** a new `.gitignore` is created containing only the pm-managed marker block

### Requirement: Composition order inside the managed block
Within the pm-managed block, the synthesized content SHALL follow this order, each section preceded by a `# === <Header> ===` comment:

1. **OS metadata**: `macos`, `linux`, `windows`
2. **IDE**: `vscode`, `jetbrains`
3. **Language**: the project's language category (e.g. `rust`, `node`, `python`, `dart`)
4. **Framework extras**: contents of `configs/<lang>/<framework>/.gitignore.extra` if present

Each embedded source SHALL be prefixed with a one-line attribution comment of the form `# Source: https://github.com/github/gitignore (CC0)` for github/gitignore-derived content.

#### Scenario: default order for ts/nextjs project
- **WHEN** `pm project init -l ts -f nextjs -y` runs
- **THEN** the managed block contains, in order, sections labeled OS metadata, IDE, Language: node, and Framework: nextjs

#### Scenario: section header is human-readable
- **WHEN** the user opens a generated `.gitignore`
- **THEN** the managed block is visually segmented by `# === ... ===` headers identifying each category source

### Requirement: Line deduplication across categories
When two or more selected categories contain identical ignore patterns, the synthesized managed block SHALL include each pattern only once, preserving the section that first introduced it. Comments and blank lines are NOT considered duplicates and are preserved.

#### Scenario: shared pattern is emitted once
- **GIVEN** both `windows` and `linux` categories list a `*.swp` pattern
- **WHEN** `pm project gitignore --categories windows,linux` runs
- **THEN** the managed block contains exactly one `*.swp` line

#### Scenario: comments are not deduplicated
- **GIVEN** two categories each contain a `# Editor backup files` comment
- **WHEN** the managed block is synthesized
- **THEN** both comments appear in their respective sections

### Requirement: `pm project gitignore` command
The system SHALL provide a `pm project gitignore` subcommand that synthesizes `.gitignore` for the current project. With no flags it writes the file in place. With `--diff` it prints a unified diff between the current file and the freshly synthesized result without writing. With `--categories <comma-separated>` the user overrides the default selection.

#### Scenario: default invocation writes the file
- **GIVEN** the current project's `.proj.yaml` declares `language: rust`, `framework: axum`
- **WHEN** the user runs `pm project gitignore`
- **THEN** `.gitignore` is updated in place with the marker block containing OS, IDE, Rust, and axum-extra sections

#### Scenario: --diff prints without writing
- **WHEN** the user runs `pm project gitignore --diff`
- **THEN** stdout contains a unified diff
- **AND** the on-disk `.gitignore` is unchanged

#### Scenario: --categories overrides defaults
- **WHEN** the user runs `pm project gitignore --categories rust,macos`
- **THEN** the managed block contains only Rust and macOS sections (no IDE, no Linux, no framework extras)

### Requirement: Framework `.gitignore.extra` files
The bundled config layout MAY include framework-specific `.gitignore.extra` files at `configs/<lang>/<framework>/.gitignore.extra`. When present, the orchestrator SHALL append the file's contents (verbatim, with a `# === Framework: <fw> ===` header) at the end of the managed block. These files are managed by pm maintainers directly; they are NOT drawn from `github/gitignore`.

#### Scenario: nextjs extra appended for ts/nextjs
- **GIVEN** `configs/ts/nextjs/.gitignore.extra` contains `.next/\nout/\n.vercel\n`
- **WHEN** `pm project gitignore` runs in a `ts/nextjs` project
- **THEN** the managed block ends with a `# === Framework: nextjs ===` header followed by the three lines above

#### Scenario: missing extra is a no-op
- **GIVEN** `configs/python/fastapi/.gitignore.extra` does not exist
- **WHEN** `pm project gitignore` runs in a `python/fastapi` project
- **THEN** the managed block has no Framework section but still includes OS, IDE, and Python categories

### Requirement: First-sync migration of v0.4.x users
When `pm project sync` or `pm project gitignore` runs against a `.gitignore` that was previously managed by v0.4.x (i.e. contains the static lines pm used to write before this change but no `pm managed` marker block), the system SHALL detect a fixed set of historical pm-introduced patterns and move them into the new managed block. The system SHALL emit a stderr notice describing how many lines were migrated.

The historical patterns SHALL include at minimum:
- `/target` (Rust)
- `**/*.rs.bk`, `*.pdb` (Rust)
- `node_modules/`, `dist/`, `*.tsbuildinfo` (TypeScript)
- `__pycache__/`, `*.py[cod]`, `.venv/`, `.env`, `.env.local` (Python)
- IDE/editor patterns the v0.4.x bundles wrote (none beyond the above in v0.4.x)

#### Scenario: legacy lines are migrated
- **GIVEN** a `.gitignore` containing `/target` and `*.tsbuildinfo` outside any marker block
- **WHEN** `pm project gitignore` runs
- **THEN** the resulting `.gitignore` no longer has `/target` or `*.tsbuildinfo` outside the marker block
- **AND** stderr contains a one-line notice mentioning the migration

#### Scenario: unknown lines are preserved as user lines
- **GIVEN** a `.gitignore` containing a custom pattern `my-private-dir/`
- **WHEN** `pm project gitignore` runs
- **THEN** `my-private-dir/` remains outside the marker block, unchanged

### Requirement: Acknowledgement of github/gitignore (CC0)
The repository SHALL include CC0 attribution for `github/gitignore` content in:
1. `LICENSES/github-gitignore-CC0.txt` containing the verbatim CC0 1.0 Universal text and a pointer to `https://github.com/github/gitignore`.
2. README "Acknowledgements" section naming `github/gitignore` as the source of bundled templates.
3. Each section in the synthesized managed block prefixed with `# Source: https://github.com/github/gitignore (CC0)`.

#### Scenario: license file exists
- **WHEN** an inspector lists the repository contents
- **THEN** `LICENSES/github-gitignore-CC0.txt` exists and contains "CC0 1.0 Universal" text

#### Scenario: README mentions the source
- **WHEN** a user reads `README.md`
- **THEN** an "Acknowledgements" section identifies `github/gitignore` as the upstream of bundled templates and links to `https://github.com/github/gitignore`

### Requirement: Build-time error on submodule absence
The `build.rs` script SHALL fail the build with an actionable error when `vendor/github-gitignore/` does not contain at least one expected source file (e.g. `Rust.gitignore`). The error SHALL name the recovery command.

#### Scenario: empty vendor directory
- **GIVEN** `vendor/github-gitignore/` exists but is empty
- **WHEN** `cargo build` runs
- **THEN** the build aborts with stderr containing "Run `git submodule update --init --recursive`"

### Requirement: `pm project init` and `sync` use the new synthesis
`pm project init` SHALL produce a `.gitignore` using the synthesis above when the project has a recognized language. `pm project sync` SHALL re-synthesize the managed block on every invocation, reflecting any updates to the embedded templates.

#### Scenario: init produces marker block
- **GIVEN** a fresh project directory with no `.gitignore`
- **WHEN** `pm project init -l rust -f axum -y` runs
- **THEN** the resulting `.gitignore` contains the pm-managed marker block with synthesized content

#### Scenario: sync refreshes the managed block
- **GIVEN** an existing `.gitignore` with a pm-managed block built against an older embedded template
- **WHEN** `pm project sync` runs after a binary upgrade
- **THEN** the managed block content is replaced with the new synthesis
- **AND** all lines outside the marker block are preserved unchanged

### Requirement: Acknowledged binary size budget
The total embedded gitignore content (sum of `len()` for the 10 default categories) SHALL be within 50 KiB. CI MAY enforce this via a build-time assertion (`build.rs` panics if the total exceeds the limit).

#### Scenario: build asserts size budget
- **WHEN** `cargo build` runs and `vendor/github-gitignore/` content totals more than 50 KiB across the embedded category set
- **THEN** the build fails with a message indicating the overrun
