## 1. Shared symlink helper (R5)

- [x] 1.1 Moved the Unix-gated `create_symlink(target, link)` from `src/commands/link.rs` to `src/path.rs` as `pub(crate)`; `link.rs` now calls `crate::path::create_symlink`. The `#[cfg(unix)]`/`#[cfg(not(unix))]` split lives in one place.

## 2. Reconcile repair helper (R3, R4, R6)

- [x] 2.1 Added `repairable_target(project) -> Option<PathBuf>` to `src/reconcile.rs`: returns the recorded link target iff the project is linked AND that target exists on disk (R4).
- [x] 2.2 Added `repair_one(target, convention) -> anyhow::Result<()>` (path-based, decoupled from `Project` so the owned sync plan can call it): refuses if a non-symlink occupies the path; removes a stale symlink node (drift); then `create_symlink`. Touches only the pointer (R3).

## 3. `pm sync` integration (R1, R2)

- [x] 3.1 In `src/commands/sync.rs`, build a `RepairTask` plan from `report.conflicts` whose kind is `BrokenLink`/`LinkTargetDrift` and `repairable_target` is `Some` (scoped by `--workspace`).
- [x] 3.2 Folded repairs into `print_plan` (`~ create|relink <convention> → <target>`) and the `nothing_to_do` / `--dry-run` checks so they preview without mutating.
- [x] 3.3 Apply repairs in the default flow under a batch confirmation (`--yes` bypass) and the non-interactive guard (reuse `PmError::NonInteractiveRestore`); linked projects remain excluded from clone/restore.
- [x] 3.4 Repairable links are skipped in the read-only conflict print (shown in the plan instead); unrecoverable linked conflicts (target missing, `LinkReplacedByDir`) still surface there and are never in the repair plan.

## 4. `pm check` annotation (R4)

- [x] 4.1 In `src/commands/check.rs`, a `BrokenLink`/`LinkTargetDrift` now appends a "repairable — run `pm sync`" hint iff `repairable_target` is `Some`; a missing-target broken link still shows "unavailable on this machine". Read-only.

## 5. Tests

- [x] 5.1 `reconcile` unit test `repairable_target_requires_existing_target`: `Some` for an existing target, `None` for a missing target or a non-linked project.
- [x] 5.2 `sync` integration tests (Unix): `sync_repairs_broken_link_when_target_exists`, `sync_corrects_drifted_link`, `sync_does_not_repair_link_replaced_by_dir`; target-missing case covered by the existing `sync_never_clones_a_linked_project`.
- [x] 5.3 `sync_dry_run_does_not_repair`: dry-run recreates no symlink.
- [x] 5.4 The repairable-vs-not predicate is covered by 5.1 (`repairable_target`), which is exactly the signal `check` annotates.

## 6. Docs

- [x] 6.1 README "Linked Projects": `pm sync` now repairs recoverable links (recreate missing / correct drifted), only touches the symlink, never clones/deletes; real-dir conflict and missing target stay manual; `pm check` marks repairable links.

## 7. Verify

- [x] 7.1 `cargo build` 0 errors; `cargo test` 187 pass (+5); `openspec validate link-repair` valid; clippy adds no new warnings (the 5 in touched files are all pre-existing: `collapse_path`, `AdoptCandidate`, the RemoteMismatch block, `adopt_one`, `run_restores`).
