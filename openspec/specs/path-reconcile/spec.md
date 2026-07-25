# path-reconcile Specification

## Purpose

Define how `pm` detects and repairs drift between the manifest (desired
state) and the filesystem (observed state) under the strict path
convention. The deterministic mapping in `path-convention` makes the
universe of "where a project should live" enumerable; this capability
specifies the read-only diagnostics (`pm check`) and the opt-in mutating
reconciler (`pm sync --adopt`, `pm sync --prune`) that act on it. It also
defines how live local Git origins are inspected (`pm list --remote`).

All mutating actions are explicit: adopt and prune require per-item
confirmation or `--yes`, and non-interactive runs without `--yes` refuse
to proceed. Prune writes a `HistoryAction::Unregistered` snapshot so
removed entries remain auditable.
## Requirements
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

### Requirement: Remote listing reads live local origins without mutation

`pm list --remote` SHALL replace the path column with each registered
project's remote state. The command SHALL read the local repository's `origin`
on every invocation without fetching the network. A live origin SHALL take
precedence over the manifest's cached `remote`; when a live origin is
unavailable, a cached value SHALL be retained in the output and marked as
saved. The command SHALL NOT modify the manifest, filesystem, or Git config.
`pm list --all --remote` SHALL apply the same behavior across all
non-system workspaces.

#### Scenario: Live origin takes precedence over cached metadata
- **GIVEN** a registered project whose local origin is `git@example.com:acme/web.git` and whose manifest remote is `https://example.com/acme/old.git`
- **WHEN** the user runs `pm list --remote`
- **THEN** the output shows the live origin and marks it as changed
- **AND** neither remote is modified

#### Scenario: Cached remote is used when the checkout is unavailable
- **GIVEN** a registered project with cached remote `https://example.com/acme/web.git` whose local path is missing
- **WHEN** the user runs `pm list --remote`
- **THEN** the cached remote is shown with a saved annotation
- **AND** the manifest remains unchanged

#### Scenario: Linked project origin comes from its recorded target
- **GIVEN** a linked project whose recorded target and convention symlink resolve to different repositories
- **WHEN** the user runs `pm list --remote`
- **THEN** the origin is read from the recorded link target
- **AND** the drifted convention symlink is not trusted as the remote source

### Requirement: Reconcile recognizes linked projects

The filesystem scan SHALL record a project-level symlink at
`<base_root>/<workspace>/<dir>` as an observed entry (resolving its target)
rather than skipping it. Classification SHALL cross-reference each project's
manifest link target against the observed symlink: a linked project whose
symlink resolves to its recorded target is healthy and SHALL NOT be reported.
A linked project SHALL NEVER be classified as an orphan and SHALL NEVER be
adopt-eligible. Workspace-level (first-level) symlinks SHALL continue to be
skipped.

#### Scenario: A correctly linked project is healthy
- **GIVEN** a manifest project `widgets` in `acme` with link target `/ext/widgets`, and a symlink at `<base_root>/acme/widgets` resolving to `/ext/widgets`
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as healthy (or not reported as a problem)
- **AND** it is not listed as an orphan, untracked, or adopt candidate

#### Scenario: A linked project with a missing directory is not an orphan
- **GIVEN** a manifest project `widgets` in `acme` with a link target, whose convention path has no symlink
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a broken link, not as an orphan

#### Scenario: A manually-created symlink is never adopted
- **GIVEN** a symlink at `<base_root>/acme/extra` pointing at a git repo, with no matching manifest entry
- **WHEN** the user runs `pm sync --adopt`
- **THEN** `extra` is not adopted into the manifest

### Requirement: `pm check` reports link drift diagnostics

`pm check` SHALL report link-specific drift for projects that have a link
target, distinguishing: a missing symlink or missing target (broken link); a
symlink that resolves to a path other than the recorded target (target drift);
and a real directory occupying the convention path of a linked project
(link replaced by directory). It SHALL also report a symlink occupying the
convention path of a project that has no link target (unexpected symlink).
All such reporting SHALL be read-only.

#### Scenario: Broken link is reported
- **GIVEN** a linked project `widgets` whose target `/ext/widgets` no longer exists
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a broken link
- **AND** the manifest and filesystem are unchanged

#### Scenario: Link target drift is reported
- **GIVEN** a linked project `widgets` recorded as `/ext/widgets` but whose symlink resolves to `/other/widgets`
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as link target drift showing both paths

#### Scenario: Link replaced by a real directory is reported
- **GIVEN** a linked project `widgets` whose convention path is now a real directory instead of a symlink
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a link-replaced-by-directory conflict

#### Scenario: Unexpected symlink for a managed project is reported
- **GIVEN** a managed project `web` (no link target) whose convention path is a symlink
- **WHEN** the user runs `pm check`
- **THEN** `web` is reported as an unexpected symlink conflict

### Requirement: `pm sync` never clones or restores a linked project

`pm sync` SHALL never clone or restore a linked project's content from a remote — a link's bytes live only at its local target, so there is nothing to clone. `pm sync` SHALL, however, repair the symlink *pointer* when the recorded target exists: it recreates a missing symlink (`broken-link`) and replaces a drifted one (`link-drift`) so the convention path resolves to the recorded target. Repair SHALL touch only the symlink — never the external target and never a real directory. When the recorded target does not exist (including on another machine), or a real directory occupies the convention path (`link-replaced-by-dir`), `pm sync` SHALL report the condition and SHALL NOT mutate anything. Repairs SHALL follow sync's existing controls: shown under `--dry-run`, applied under the default confirmation (`--yes` bypasses), and refused in a non-interactive run without `--yes`.

#### Scenario: A linked project is never cloned from its remote
- **GIVEN** a linked project `widgets` with a recorded remote whose symlink and target are both missing
- **WHEN** the user runs `pm sync`
- **THEN** `widgets` is not cloned
- **AND** it is reported as a broken link rather than restored from the remote

#### Scenario: A broken link is repaired when its target exists
- **GIVEN** a linked project `widgets` recorded as `/ext/widgets` where `/ext/widgets` exists but the symlink at the convention path is missing
- **WHEN** the user runs `pm sync` and confirms (or passes `--yes`)
- **THEN** a symlink is created at the convention path pointing at `/ext/widgets`
- **AND** `widgets` is not cloned and `/ext/widgets` is unchanged

#### Scenario: A drifted link is corrected
- **GIVEN** a linked project `widgets` recorded as `/ext/widgets` (which exists) whose symlink currently resolves to `/other/widgets`
- **WHEN** the user runs `pm sync` and confirms
- **THEN** the symlink is replaced to point at `/ext/widgets`
- **AND** neither `/ext/widgets` nor `/other/widgets` is modified

#### Scenario: A link replaced by a real directory is not auto-repaired
- **GIVEN** a linked project `widgets` whose convention path is now a real directory
- **WHEN** the user runs `pm sync`
- **THEN** the condition is reported and the directory is left untouched
- **AND** no symlink is created

#### Scenario: A link unavailable on this machine is left alone
- **GIVEN** a manifest with linked project `widgets` whose recorded target does not exist on this machine and no symlink is present
- **WHEN** the user runs `pm sync` with no flags
- **THEN** `widgets` is neither cloned, repaired, nor removed
- **AND** `pm check` reports it as a link unavailable on this machine

#### Scenario: Repair is previewed by dry-run and refused non-interactively
- **GIVEN** a repairable broken link (recorded target exists, symlink missing)
- **WHEN** the user runs `pm sync --dry-run`
- **THEN** the planned repair is printed and no symlink is created
- **AND** running `pm sync` without `--yes` in a non-interactive context refuses with a non-interactive error

### Requirement: `pm check` marks repairable links

`pm check` SHALL annotate a reported broken or drifted link as *repairable* when its recorded target exists on disk, directing the user to `pm sync`; it SHALL NOT mark a link repairable when the recorded target is missing. The annotation SHALL be read-only and SHALL NOT mutate the manifest or filesystem.

#### Scenario: A broken link with an existing target is marked repairable
- **GIVEN** a linked project `widgets` whose recorded target exists but whose symlink is missing
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a broken link and marked repairable (pointing the user at `pm sync`)

#### Scenario: A broken link with a missing target is not marked repairable
- **GIVEN** a linked project `widgets` whose recorded target does not exist
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a broken link and is not marked repairable

