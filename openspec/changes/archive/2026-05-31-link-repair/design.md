## Context

`pm-link` added linked projects: a symlink at `<base_root>/<workspace>/<dir>`
pointing at a canonical external target recorded in `Project.link`. `classify`
(in `src/reconcile.rs`) already distinguishes the relevant states —
`BrokenLink` (symlink/target missing), `LinkTargetDrift` (symlink resolves to a
path other than the record), `LinkReplacedByDir` (a real directory took the
path) — and `pm check` reports them. `pm sync` currently *excludes* linked
projects from its restore/clone plan and only reports broken links.

The manifest records the canonical target, so when that target still exists the
fix is deterministic: (re)create the symlink. This change makes `pm sync`
perform that repair as part of its existing restore flow, reusing the
desired/observed model (D8): the manifest declares the link, the filesystem is
reconciled toward it.

## Goals / Non-Goals

**Goals:**
- `pm sync` repairs recoverable links (recreate a missing symlink, correct a
  drifted one) when the recorded target exists.
- Repair touches only the symlink pointer — never the external target, never a
  real directory.
- Fold repair into sync's existing plan / dry-run / confirmation / non-
  interactive model, with no new flag.

**Non-Goals:**
- Removing a real directory to make room for a link (`link-replaced-by-dir`
  stays a manual, reported conflict).
- A standalone `pm link --repair` command.
- Cloning a linked project from a remote (still never happens).
- Verifying that the target is the "right" repo by content.

## Decisions

### R1 — Repair lives in `pm sync`, not a new command

Repair is folded into `pm sync`'s default reconcile flow rather than exposed as
`pm link --repair`.

- **Why:** repair = restore the manifest's declared state, which is exactly
  what `pm sync` already does for missing clones. The target is already
  recorded; no new user input is needed. A `pm link --repair <project>` flag
  would also be awkward — `pm link`'s positional is a *path* to link, whereas
  repair operates on an existing *project name*.
- **Alternative considered:** standalone `pm link --repair`. Rejected per the
  above and the chosen scope.

### R2 — Repair is part of the default flow, gated by sync's existing model

No `--repair` flag. A recoverable link is repaired in the same pass that
restores missing clones, and is subject to the same controls:

- listed in the plan and under `--dry-run`;
- applied under the default confirmation prompt (`--yes` bypasses);
- a non-interactive run without `--yes` refuses (reuses
  `PmError::NonInteractiveRestore`).

- **Why:** recreating a symlink whose target exists restores declared intent —
  the same "restore" bucket as clone-restore, and unlike `--adopt`/`--prune` it
  neither adds nor drops manifest membership. It is local and reversible (`pm
  rm` removes the symlink). Default-on (no flag) matches the user's chosen
  scope while sync's confirmation model keeps it safe.
- **Alternative considered:** opt-in `pm sync --repair`. Rejected — the chosen
  design integrates repair into the default flow.

### R3 — Repair only ever touches the symlink pointer

- `BrokenLink` with an existing target → create the symlink.
- `LinkTargetDrift` → remove the wrong symlink node and create the correct one.
- `BrokenLink` whose target is missing → **report, do not repair** (nothing to
  point at; also the cross-machine case).
- `LinkReplacedByDir` → **report, do not repair** (removing a real directory
  could destroy data; stays manual).

- **Why:** the defining safety property of a link is that pm manages only the
  pointer. Repair must never use `remove_dir_all` or relocate the target.

### R4 — Recoverability is decided by the recorded target's existence

A linked project is *repairable* iff `Project.link`'s path exists on disk.
This is the single predicate `check` annotates and `sync` plans on.

- **Why:** the recorded canonical target is the manifest's declared intent; its
  on-disk presence is the only thing that makes a symlink meaningful.

### R5 — Symlink creation is factored into one shared, Unix-gated helper

The `#[cfg(unix)]`/`#[cfg(not(unix))]` `create_symlink` currently private to
`commands/link.rs` is moved to a shared location so both `pm link` and the
repair path use one implementation (and one platform gate).

- **Why:** avoid divergent symlink logic; keep the Unix gate in exactly one
  place. On non-Unix, repair simply produces no symlink actions (a linked
  project can't have been created there).
- **Alternative considered:** duplicate the helper. Rejected (drift risk).

### R6 — `repair_one` mirrors `adopt_one`/`prune_one`

Reconcile exposes a small helper that, given config + a linked project + its
observed entry, performs the pointer (re)creation. `sync` builds a repair plan
from the linked-project conflicts in the report and applies it alongside
restores. `check` stays read-only and only annotates.

- **Why:** consistency with the existing reconcile helper shape; keeps `sync`
  command-shaped and the FS mutation in one tested place.

## Risks / Trade-offs

- **[Drift repair overwrites a hand-edited symlink]** → The user may have
  repointed the symlink deliberately. Mitigation: repair only replaces a
  symlink that disagrees with the *recorded* target (the declared intent, D8),
  it is shown in the plan, and it is confirmed; `--dry-run` previews it.
- **[Target exists but is the wrong repo]** → Repair trusts the recorded
  canonical path; it does not verify repo identity. Mitigation: documented
  non-goal; drift detection compares against the record, not content.
- **[TOCTOU: target disappears between plan and apply]** → Creating the symlink
  yields a dangling link. Mitigation: harmless — the next `pm check` reports it
  as broken; no data is touched.
- **[Perceived conflict with "sync never restores a link"]** → Resolved by
  distinguishing *clone from remote* (never) from *symlink recreation* (the
  repair). The path-reconcile requirement is modified to state this precisely.

## Migration Plan

None. Purely behavioral and additive: default `pm sync` gains repair, but it
acts only on linked projects whose target exists — a no-op for anyone not using
links. No manifest or schema change.

## Open Questions

- Should a future `pm check --fix` shortcut invoke the same repair path without
  the full sync? Out of scope here; the repair helper would make it trivial
  later.
