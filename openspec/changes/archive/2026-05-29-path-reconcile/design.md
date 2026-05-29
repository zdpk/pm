## Context

`strict-path-convention` (v4) makes project paths a deterministic function of
`base_root + workspace + project.dir` and establishes the desired/observed
state model (D8 of that change): the manifest declares *what* `pm` manages,
the filesystem owns *whether* a directory physically exists. The two can
drift, and the convention finally makes that drift mechanically detectable —
the universe of "where a project should be" is enumerable as `base_root/*/*`.

Existing tooling covers one direction only:

- `pm check` (`src/commands/check.rs`): manifest → filesystem, read-only.
  Reports `missing-restorable`, `missing-unrestorable`, non-git directory at
  expected path, remote mismatch.
- `pm sync` (`src/commands/sync.rs`): manifest → filesystem, mutating.
  Clones missing projects from `remote` when available.

There is no command that detects **untracked directories** (disk has a git
repo at `base_root/<ws>/<dir>` but the manifest has no entry for it) or that
**prunes** manifest entries whose directories have been deleted out-of-band.

## Goals / Non-Goals

**Goals:**

- Extend `pm check` with untracked-directory detection so users can see
  drift in both directions without changing state.
- Extend `pm sync` with `adopt` (register untracked dirs) and `prune`
  (drop orphan manifest entries with history). The existing re-clone
  behavior is preserved.
- Honor the D8 model: adopt/prune always require explicit confirmation;
  prune writes a history entry. No silent mutations.
- Provide `--dry-run` and `--yes` flags consistent with existing commands.

**Non-Goals:**

- Auto-adoption (manifest is the membership SoT — items appear there only
  by explicit user action).
- Conflict auto-resolution (remote mismatch, non-git collision at the
  expected path). Diagnostic only.
- Re-linking an untracked dir to an orphan manifest entry. Treat the two
  cases independently; if the user wants the link, they prune + adopt.
- Reconciliation of `.trash` contents. Trash items are tracked by design.
- External-path workspaces (`pm link`). Separate concern.

## Decisions

### D1. Drift taxonomy

Three categories surfaced by `pm check`:

| State | Manifest | Disk | Action class |
|-------|----------|------|--------------|
| **orphan** | has entry | missing | `pm sync` can re-clone (remote) or prune (confirmation) |
| **untracked** | no entry | git dir present | `pm sync --adopt` registers (confirmation) |
| **conflict** | has entry | exists but mismatched (non-git, remote diff) | diagnostic only |

Untracked non-git directories are reported but not adopt-eligible (heuristic:
adopt only what looks like a project to `pm`).

### D2. Scanning strategy

A new helper `scan_base_root(config) -> Vec<DiskEntry>` walks two levels of
`base_root`:

- Level 1: workspace directories (each immediate child of `base_root`).
- Level 2: project directories inside each workspace.

Rules:
- Skip symlinks at both levels (avoid recursion + cross-fs surprises).
- Skip non-directories.
- Treat names that violate path-convention shape (`.`, `..`, contain
  separators after `read_dir` — shouldn't happen but be defensive) as
  noise and ignore.
- `.`-prefix workspaces (system, e.g. `.trash`) are scanned. Their
  contents are not surfaced as "untracked" — anything in `.trash` is a
  tracked-but-trashed project handled by the existing trash workflow.

The scan returns enough metadata for classification without a second pass:
`(workspace_name, project_dir, is_git, remote)`.

### D3. `pm check` extension (read-only)

Today's check iterates the manifest. We add a second pass that iterates
`scan_base_root` and reports any disk entry whose `(workspace, dir)` is not
in the manifest. Format follows the existing aligned-column output with a
new `untracked` status.

`pm check` makes **no** filesystem or manifest changes regardless of what
it finds. Mutating actions are gated behind `pm sync`.

### D4. `pm sync` modes

`pm sync` becomes a three-mode reconciler:

- **restore** (existing, default): orphan with remote → `git clone`.
- **adopt** (new, opt-in via `--adopt`): untracked git dir → register in
  manifest. Inferred fields:
  - `name` = directory name (must pass `validate_project_dir` + the
    project-name regex; otherwise reported and skipped).
  - `workspace` = parent directory name (must exist in manifest;
    otherwise reported and skipped — adopt does not create workspaces).
  - `dir` = directory name (same as `name` under the convention).
  - `remote` = `git remote get-url origin` if available.
  - `repo_slug` = derived from `remote`, else `name`.
  - `tags`, `note`, `proj`, `repo_spec` = empty.
  - On name collision with an existing manifest entry: skip with an
    explicit warning. The user can manually `pm add` with `--force` if
    they really want to overwrite, but adopt never does it silently.
- **prune** (new, opt-in via `--prune`): orphan without remote (and any
  orphan the user confirms) → remove manifest entry and append to history
  as `HistoryAction::Unregistered`.

Combined invocations (`pm sync --adopt --prune`) process both. Each item
prompts individually unless `--yes`.

Default `pm sync` with no extra flags retains the v0.5.0 behavior
(restore only) to avoid surprising existing users when this change ships.

### D5. Confirmation contract

- Interactive TTY: each adopt and each prune prompts `[y/N]` before
  applying. The existing `prompt_yes_no` helper in `src/restore.rs` is
  reused.
- `--yes`: skip all prompts, apply all. Equivalent to "yes to every
  prompt." Both adopts and prunes still log what was done.
- Non-TTY without `--yes`: error out with guidance (mirrors the existing
  `NonInteractiveRestore` behavior). Refusing destructive defaults in
  non-interactive contexts is consistent with D8.
- `--dry-run`: print the plan (one line per item), make no changes.
  Independent of `--yes`.

### D6. Prune writes history

Prune deletes a manifest entry. To preserve the D8 "manifest is SoT, so
editing it is a deliberate act" contract, prune appends a
`HistoryAction::Unregistered` snapshot to `history.json` containing the
project's name, workspace, repo_slug, dir, optional remote, and the
convention path. The existing `pm history` command then surfaces it.

### D7. Capability ownership

The new behavior lives in a new capability spec `path-reconcile` rather
than extending `path-convention`. Rationale: `path-convention` defines the
*mapping* between names and paths; `path-reconcile` defines the *process*
of repairing drift between manifest and filesystem under that mapping.
Keeping them separate keeps each spec coherent.

### D8. Module layout

A new module `src/reconcile.rs` holds:

- `DiskEntry` struct and `scan_base_root(config) -> Vec<DiskEntry>`
- `classify(manifest, disk) -> ReconcileReport` returning `orphan`,
  `untracked`, `conflict` lists
- `adopt_one(...)` and `prune_one(...)` helpers used by `pm sync`

`pm check` and `pm sync` call into this module; the modules `check.rs`
and `sync.rs` stay command-shaped and don't grow new logic of their own.

## Risks / Trade-offs

- [**Adopt naming collisions**] An untracked dir at `base_root/<ws>/foo`
  when the manifest already has a different project named `foo` (perhaps
  in another workspace). → Skip with warning. Conservative default; user
  can resolve manually.
- [**Performance on large `base_root`**] Two-level scan is O(workspaces ×
  projects-per-workspace). Should be fine in practice; if it becomes a
  problem, cache per workspace or parallelize. Not in this change.
- [**Adopt of a dir that turns out to be the user's home or some
  important directory**] Mitigated by requiring `is_git_repo` for adopt,
  and by the path-convention shape constraint on the dir name.
- [**Symlink farms**] `scan_base_root` deliberately skips symlinks. If a
  user wants their projects symlinked from elsewhere, that's `pm link`
  territory (separate change), not in scope here.

## Migration Plan

None. This is purely additive — new flags on existing commands and new
diagnostic categories. Default invocations of `pm check` and `pm sync`
retain v0.5.0 behavior; the new capabilities are opt-in.

Documentation:
- README sections for `pm check` and `pm sync` updated to describe the
  new flags and the desired/observed state mental model.
- A short "drift workflow" example showing `pm check` → review →
  `pm sync --adopt --prune --dry-run` → `pm sync --adopt --prune`.

## Open Questions

- Should adopt offer to auto-link an untracked dir to an orphan manifest
  entry with a matching `remote`? Could save the user a manual prune +
  adopt cycle. Initial answer: no, keep the two flows independent for
  clarity; revisit after we see how often this case shows up in practice.
- Behavior when an adopt's inferred `name` clashes with the
  project-name regex (e.g., directory `1web`). Initial answer: skip with
  warning; the user can `pm add` with an explicit `--name`.
- Should `pm sync --prune` ever auto-prune orphans without a remote (the
  unrestorable case)? D8 says no — prune is SoT editing, always
  confirmed. We hold that line.
