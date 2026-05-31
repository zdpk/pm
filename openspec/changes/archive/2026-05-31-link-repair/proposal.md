## Why

`pm-link` shipped linked projects (a symlink at the convention path pointing at
an external git repo) and taught `pm check`/`pm sync` to *detect* link drift —
but repair is manual. When a linked project's symlink goes missing (e.g. the
workspace dir was recreated) or points at the wrong place, `pm check` reports a
`broken-link`/`link-drift` and the user must delete and re-`pm link` by hand.
Yet the manifest already records the canonical target, so the fix is
deterministic: recreate the symlink. This is exactly the "restore declared
state" job `pm sync` already does for missing clones.

## What Changes

- **`pm sync` repairs recoverable links** as part of its default restore flow
  (no new flag): for a linked project whose recorded target still exists, sync
  recreates a missing symlink (`broken-link`) or replaces a wrong one
  (`link-drift`) so the convention path resolves to the recorded target.
- **Repair only ever touches the symlink pointer**, never the external target
  and never a real directory. Unrecoverable cases are reported, not mutated:
  - target no longer exists (also the cross-machine case) → reported, not
    repaired (nothing to point at);
  - a real directory occupies the convention path (`link-replaced-by-dir`) →
    reported, not repaired (removing it could destroy data).
- **Repairs are surfaced in the sync plan** and gated by sync's existing
  interactive model: shown under `--dry-run`, applied under the default
  confirmation (and `--yes` bypass); a non-interactive run without `--yes`
  refuses, consistent with restore/adopt/prune.
- **`pm sync` still never clones a linked project from a remote.** Repair is
  symlink recreation, a distinct, local, reversible action — not a clone. This
  refines (does not contradict) the v0.5.x rule.
- **`pm check` labels a reported link as repairable** when its target exists, so
  the diagnostic points the user at `pm sync`.

Non-goals: removing a real directory to make room for a link (the
`link-replaced-by-dir` case stays manual); a standalone `pm link --repair`
command (repair lives in the reconcile/sync flow); repairing non-link drift
(unchanged).

## Capabilities

### New Capabilities
<!-- none — repair extends existing reconcile behavior -->

### Modified Capabilities
- `path-reconcile`: the requirement that `pm sync` never clones or restores a
  linked project is refined — sync now **repairs recoverable links** (recreate
  a missing symlink, correct a drifted one) when the recorded target exists,
  while still never cloning a link from a remote; unrecoverable links
  (target missing, or a real dir at the path) are reported, not mutated. The
  `pm check` diagnostic gains a "repairable" signal.

## Impact

- **Code:** `src/reconcile.rs` (a `repair_one`/plan helper that recreates the
  symlink; classify already distinguishes the cases); `src/commands/sync.rs`
  (build a repair plan from linked-project conflicts, fold into the plan
  print / confirmation / dry-run / non-interactive guards); `src/commands/check.rs`
  (annotate repairable broken/drift links).
- **Manifest:** none (uses the existing `Project.link`).
- **Platform:** symlink creation remains Unix-gated (same as `pm link`).
- **Docs:** README "Linked Projects" + drift-reconciliation notes: `pm sync`
  now repairs recoverable links.
