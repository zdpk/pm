## ADDED Requirements

### Requirement: `pm check` surfaces untracked directories

In addition to its existing manifest-driven diagnostics, `pm check` SHALL scan the convention root and report any directory at `<base_root>/<workspace>/<dir>` that is a git repository but has no matching project in the manifest. The command SHALL classify such entries as `untracked` and SHALL NOT modify the manifest or the filesystem.

#### Scenario: Untracked git directory is reported
- **GIVEN** a git repository exists at `<base_root>/acme/web` and the manifest has no project with workspace `acme` and dir `web`
- **WHEN** the user runs `pm check`
- **THEN** the output lists `acme/web` with an `untracked` status
- **AND** the manifest and the filesystem are unchanged

#### Scenario: Untracked non-git directory is reported separately or omitted
- **GIVEN** a non-git directory exists at `<base_root>/acme/notes` and is not in the manifest
- **WHEN** the user runs `pm check`
- **THEN** the directory is either reported as `untracked-non-git` (informational) or omitted; it is never treated as adopt-eligible

#### Scenario: System workspace contents are not reported as untracked
- **GIVEN** a directory exists at `<base_root>/.trash/old` whose corresponding project lives in the manifest under the `.trash` workspace
- **WHEN** the user runs `pm check`
- **THEN** the directory is not reported as `untracked`

### Requirement: `pm sync --adopt` registers untracked git directories

When invoked with `--adopt`, `pm sync` SHALL offer to register each untracked git directory discovered by the scan. The system SHALL infer the project name, dir, and workspace from the directory location; derive the remote from `origin` if present; and require explicit confirmation per item (or `--yes`) before mutating the manifest.

#### Scenario: Adopt registers an untracked git directory after confirmation
- **GIVEN** an untracked git directory at `<base_root>/acme/web` with origin `https://example.com/acme/web.git`
- **WHEN** the user runs `pm sync --adopt` in a TTY and answers `y` for the entry
- **THEN** the manifest gains a project named `web` in workspace `acme` with `dir = "web"` and `remote = "https://example.com/acme/web.git"`
- **AND** the working copy on disk is unchanged

#### Scenario: Adopt skips entries with name collisions
- **GIVEN** the manifest already contains a project named `web` in any workspace, and an untracked git directory at `<base_root>/labs/web`
- **WHEN** the user runs `pm sync --adopt`
- **THEN** the labs/web entry is reported as a collision and skipped
- **AND** the manifest is unchanged for that entry

#### Scenario: Adopt skips entries whose inferred name is not a valid project name
- **GIVEN** an untracked git directory at `<base_root>/acme/1bad` whose name fails the project-name regex
- **WHEN** the user runs `pm sync --adopt`
- **THEN** the entry is reported with the violated rule and skipped
- **AND** the manifest is unchanged for that entry

#### Scenario: Adopt skips entries whose parent workspace is not in the manifest
- **GIVEN** a git directory at `<base_root>/strangers/web` where `strangers` is not a workspace in the manifest
- **WHEN** the user runs `pm sync --adopt`
- **THEN** the entry is reported with a missing-workspace warning and skipped
- **AND** the manifest is unchanged

### Requirement: `pm sync --prune` removes orphan manifest entries with history

When invoked with `--prune`, `pm sync` SHALL offer to remove each manifest entry whose project directory is missing on disk. Each removal SHALL require explicit confirmation per item (or `--yes`) and SHALL append a snapshot of the removed project to `history.json` as `HistoryAction::Unregistered`.

#### Scenario: Prune removes an orphan and records history
- **GIVEN** a manifest project `web` in workspace `acme` whose directory `<base_root>/acme/web` does not exist
- **WHEN** the user runs `pm sync --prune` in a TTY and answers `y`
- **THEN** the project is removed from the manifest
- **AND** `history.json` records the project with action `Unregistered`

#### Scenario: Prune confirms each entry independently
- **GIVEN** two orphan projects `web` and `api`
- **WHEN** the user runs `pm sync --prune` and answers `y` for `web` and `n` for `api`
- **THEN** `web` is removed and recorded in history; `api` remains in the manifest

#### Scenario: Prune is the only path that removes orphans
- **GIVEN** any orphan project (no remote or with remote)
- **WHEN** the user runs `pm sync` (no flags) or `pm sync --adopt`
- **THEN** the orphan is not removed from the manifest

### Requirement: Reconcile commands respect dry-run and non-interactive modes

The `pm sync` command SHALL accept a `--dry-run` flag that prints the full plan (which entries would be adopted, pruned, or restored) without mutating either the manifest or the filesystem. When `pm sync --adopt` or `pm sync --prune` runs in a non-interactive context (no TTY) without `--yes`, the command SHALL fail with a clear non-interactive error rather than silently applying changes.

#### Scenario: Dry-run prints the plan and changes nothing
- **WHEN** the user runs `pm sync --adopt --prune --dry-run`
- **THEN** the command prints every adopt and prune that would happen
- **AND** neither the manifest nor the filesystem is modified
- **AND** `history.json` is unchanged

#### Scenario: Non-interactive adopt without --yes errors out
- **GIVEN** stdin/stdout are not TTYs
- **WHEN** the user runs `pm sync --adopt` without `--yes`
- **THEN** the command fails with a non-interactive error referencing `--yes`
- **AND** the manifest is unchanged

#### Scenario: --yes applies without prompting
- **GIVEN** stdin/stdout are not TTYs and there are adopt and prune candidates
- **WHEN** the user runs `pm sync --adopt --prune --yes`
- **THEN** all candidates are processed without interactive prompts
- **AND** the resulting changes match what `--dry-run` previously reported

### Requirement: Default `pm sync` behavior is unchanged

When invoked without `--adopt` or `--prune`, `pm sync` SHALL behave exactly as before: it restores missing manifest entries that carry a remote and leaves everything else alone. Adopt and prune SHALL be opt-in.

#### Scenario: Plain `pm sync` only restores; never adopts or prunes
- **GIVEN** untracked git directories and orphan manifest entries both exist
- **WHEN** the user runs `pm sync` with no flags
- **THEN** missing entries with a remote are cloned as before
- **AND** untracked directories are neither adopted nor reported as errors
- **AND** orphan entries (with or without remote) are not removed
