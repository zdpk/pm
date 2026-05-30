## Why

Under the v4 strict path convention, the only way to register an existing
local directory is `pm add`, which **physically moves** it into
`base_root/<workspace>/<dir>`. There is no way to manage a git repository
that must stay where it lives (a shared checkout, a mount, a path other
tools depend on). Users who want this today are forced to either move the
repo or leave it unmanaged.

A symlink at the convention path solves this without weakening v4: pm and
the shell still see the project at `base_root/<ws>/<dir>`, and the
filesystem redirects to the real location. The blocker is that
`path-reconcile` deliberately skips symlinks, so a linked project is
invisible to the disk scan and gets **misclassified as an orphan** — which
makes `pm check` report it as missing and `pm sync` try to re-clone over
it.

## What Changes

- **New `pm link <path> [--name --workspace --tags --note]` command.** Creates
  a symlink at `base_root/<ws>/<dir>` pointing at an existing **git** repo at
  `<path>`, and registers the project in the manifest. The external target is
  never moved or copied. Aborts if the convention path is already occupied or
  if `<path>` is not a git repo.
- **Manifest marks linked projects.** A new optional `link` field on `Project`
  records the canonical external target path. Its presence is what tells
  reconcile "this is a deliberate symlink, not a missing directory." Absent =
  ordinary managed project (unchanged on-disk behavior).
- **`path-reconcile` becomes link-aware** (MODIFIED):
  - `scan_base_root` records a project-level symlink as a `DiskEntry` (with a
    symlink/target marker) instead of skipping it.
  - `classify` cross-references the manifest `link` field against the observed
    symlink, producing link-specific diagnostics instead of false orphans:
    *broken link* (symlink missing), *link target drift* (points elsewhere),
    *link replaced by real dir*.
  - `pm sync` **excludes linked projects from the clone/restore plan** — a
    linked project is machine-local and is never restored from a remote. Its
    only "restore" action is recreating the symlink.
- **`pm rm` of a linked project removes only the symlink**, never the external
  target. **`pm mv` relocates the symlink** to the new convention path and
  updates the manifest; the external target stays put (consistent with D9).
- **Unix-first.** Symlink creation is gated to Unix in v0.5.x; on non-Unix
  `pm link` returns a clear "not supported" error, mirroring the existing
  `pm logs`/`pm stop` gating.

Non-goals (explicitly out of scope): linking non-git directories;
cross-machine portability of links (a linked project simply does not exist on
another machine — `pm sync` there leaves it absent); auto-adopting
manually-created symlinks via `pm sync --adopt`.

## Capabilities

### New Capabilities
- `project-link`: Linking an existing external git repo into the convention
  path via a symlink — the `pm link` command, the manifest `link` field, and
  the link-specific lifecycle rules for `pm rm` / `pm mv` (never touch the
  external target).

### Modified Capabilities
- `path-reconcile`: Scan recognizes project-level symlinks; classify gains
  link-aware diagnostics (broken link / target drift / replaced-by-real-dir)
  in place of orphan misclassification; `pm sync` excludes linked projects
  from the restore plan.

## Impact

- **Code:** new `src/commands/link.rs`; `src/cli.rs` (`Link` subcommand);
  `src/main.rs` (dispatch); `src/models.rs` (`Project.link` field);
  `src/reconcile.rs` (`scan_base_root` symlink handling, `DiskEntry` marker,
  `classify` link diagnostics, new `ConflictKind` variants);
  `src/commands/sync.rs` (exclude links from restore plan);
  `src/commands/check.rs` (render link diagnostics);
  `src/commands/remove.rs` and `src/commands/workspace.rs` (link-safe rm/mv);
  `src/error.rs` (new error variants).
- **Manifest:** additive optional field; no version bump required (still v4,
  `link` is `skip_serializing_if = "Option::is_none"`).
- **Docs:** README `pm link` section + reconcile mental-model update.
- **Platform:** symlink creation Unix-only in this change.
