## 1. Manifest schema

- [x] 1.1 Add `link: Option<String>` to `Project` in `src/models.rs` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; confirm existing v4 manifests round-trip unchanged (no version bump).
- [x] 1.2 Linked projects are built via `Project::new` + direct `project.link = Some(..)` assignment (the codebase idiom used for `remote`/`note`/`tags`); added `Project::is_linked()` helper. No separate constructor needed.

## 2. Reconcile: observe symlinks (L3)

- [x] 2.1 Add a `link_target: Option<PathBuf>` marker (plus `is_symlink: bool`) to `DiskEntry` in `src/reconcile.rs`.
- [x] 2.2 In `scan_base_root`, stop skipping project-level symlinks: when `file_type().is_symlink()`, resolve via `canonicalize`, record `link_target`, and compute `is_git`/`remote` from the resolved target (dangling target → `link_target=None`, `is_git=false`). Keep skipping workspace-level symlinks.

## 3. Reconcile: classify links (L4)

- [x] 3.1 Add `ConflictKind` variants: `BrokenLink`, `LinkTargetDrift`, `LinkReplacedByDir`, `UnexpectedSymlink` (kept as unit variants so `ConflictKind` stays `Copy`; drift's expected/found paths are computed at render time in `check`).
- [x] 3.2 In `classify`, branch on `project.link` via `classify_linked`: when `Some(target)`, compare against the observed entry per the design table (healthy / broken / drift / replaced); a linked project is NEVER an orphan.
- [x] 3.3 In `classify`, a managed project (`link = None`) whose disk entry is a symlink → `UnexpectedSymlink`; untracked symlink entries are skipped in the disk-driven pass (never `untracked_git`, never adopt-eligible).

## 4. `pm link` command (L1, L6, L8, L9)

- [x] 4.1 Add `Link { path, name, workspace, tags, note }` subcommand to `src/cli.rs` and dispatch in `src/main.rs`.
- [x] 4.2 Create `src/commands/link.rs`: validate name + dir, resolve+canonicalize `<path>`, require `is_git_repo` (else error), require workspace exists, default name/dir to basename.
- [x] 4.3 Abort when the convention path is occupied (`symlink_metadata().is_ok()` — real dir, file, or existing/dangling symlink) or the project name collides; leave fs + manifest untouched on any abort.
- [x] 4.4 Create the symlink at the convention path and add the manifest entry with the canonical `link` target; `create_symlink` gated with `#[cfg(unix)]`, returning `PmError::LinkNotSupported` otherwise (before any manifest save).
- [x] 4.5 Added error variants to `src/error.rs` (`NotAGitRepo`, `LinkPathOccupied`, `LinkNotSupported`); reused existing `PathNotFound`, `InvalidProjectName`, `ProjectExists`, `InvalidProjectDir`.

## 5. `pm sync` excludes links (L5)

- [x] 5.1 In `src/commands/sync.rs`, exclude any project with `link.is_some()` (`is_linked()`) from the restore plan, even with a remote; classify surfaces broken links as `BrokenLink`, never restored.
- [x] 5.2 Cross-machine behavior holds: a linked entry whose target/symlink is absent classifies as `BrokenLink` (not orphan), and default `pm sync` never prunes (prune is opt-in) — neither cloned nor removed. Covered by test 8.3.

## 6. `pm check` rendering (L4)

- [x] 6.1 In `src/commands/check.rs`, render the new link diagnostics (broken / target drift showing both paths / replaced-by-dir / unexpected symlink); healthy links render as `✓`. Read-only.

## 7. Link-safe `rm` and `mv` (L7)

- [x] 7.1 In `src/commands/remove.rs`, linked projects route to `remove_linked`: removes only the symlink node (`fs::remove_file` after a `symlink_metadata` check, never `remove_dir_all`); all flags collapse to symlink-only removal; refuses to delete a real dir on link drift; never touches the external target.
- [x] 7.2 In `src/commands/workspace.rs` (move path), `fs::rename` relocates the symlink node (existence check extended to catch dangling symlinks); only `workspace` is updated in the manifest so the `link` target is preserved; the external directory is never moved.

## 8. Tests

- [x] 8.1 `link.rs` integration tests (Unix-gated, via `TestEnv` + `init_git_repo`): link creates symlink + canonical manifest entry without moving target; honors `--name`/`--workspace`; non-git rejected; missing rejected; occupied aborts; name collision aborts. (6 tests)
- [x] 8.2 `reconcile` unit tests: healthy link not reported; broken link not orphan; dangling symlink broken; target drift; link-replaced-by-dir; unexpected symlink for managed project; untracked symlink never adopt-eligible. (7 tests)
- [x] 8.3 `sync` test: a linked project with a working local remote and no symlink is never cloned and never pruned by default sync.
- [x] 8.4 `remove` tests asserting the external target survives both `pm rm` and `pm rm -r -f` of a linked project. (2 tests)
- [x] 8.5 `mv` test: moving a linked project relocates the symlink, still resolves to the external target, preserves the `link` field, and does not move the external dir.

## 9. Docs

- [x] 9.1 README.md: added a `pm link` command-table row and a "Linked Projects" section covering link safety/lifecycle (rm/mv), machine-local semantics, and the `pm check` link diagnostics. (README-ko.md does not exist in this repo.)

## 10. Verify

- [x] 10.1 `cargo build` (0 errors) and `cargo test` (182 pass, +17 new) green; `openspec validate pm-link` valid. `cargo clippy`: this change introduces no new warnings (one `#[cfg(not(unix))]`-only dead-code variant annotated); pre-existing codebase clippy warnings (collapsible-if, etc.) are out of scope.
