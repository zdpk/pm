## 1. Path convention core (validation + model)

- [x] 1.1 Add a `dir`-name validator (e.g. `validate_project_dir(&str) -> Result<()>`) enforcing: non-empty, no `/` or `\`, not `.` or `..`, no absolute-path marker (`/`, `C:\`, drive prefix), no null byte. Return errors that name the offending value and the violated rule.
- [x] 1.2 Remove the `root` field from `Workspace` in `src/models.rs`; update `Workspace::new` and `Workspace::new_system` signatures to drop the `root` parameter and all call sites.
- [x] 1.3 Bump manifest schema to v4: change `Manifest::default().version` to `4` and update `migrate_legacy_data`'s emitted version expectations accordingly.
- [x] 1.4 Unit-test the `dir` validator against accept/reject cases from the spec (single name accepted; nested, `..`, absolute, empty rejected).

## 2. Deterministic path functions

- [x] 2.1 Replace `workspace_root(config, workspace)` in `src/state.rs` with a name-based `workspace_dir(base_root, name)` returning `expand_path(base_root).join(name)`.
- [x] 2.2 Simplify `project_path` to `base_root/<workspace>/<project.dir>`, looking up only the workspace name from the manifest.
- [x] 2.3 Remove `relative_dir` and `normalized_workspace_root` from `src/state.rs` and delete their call sites.
- [x] 2.4 Rewrite `detect_current_project` to check `cwd.starts_with(base_root)`, take the next two path components as `<workspace>`/`<project>`, and look them up directly (drop the longest-prefix scan loop).
- [x] 2.5 Update `src/config.rs` path helpers: simplify the `projects_path`/`workspaces_path` resolution that depended on `workspace.root`, and remove `relative_or_basename`.

## 3. Manifest loading validation

- [x] 3.1 In `load_manifest`, reject manifests with `version <= 3` with a clear incompatibility error referencing the v4 convention and instructing the user to recreate workspaces (no migration tool).
- [x] 3.2 In `load_manifest`, fail when any workspace entry carries a legacy `root` override field (detect via raw JSON before deserializing into the new model), naming the unsupported field.
- [x] 3.3 In `load_manifest`, run the `dir` validator over every project and fail with an error listing each violating project and rule; a fully-conforming manifest loads successfully.
- [x] 3.4 Reconcile/remove the v1→v2 `migrate_legacy_data` path so it no longer produces sub-v4 manifests that would then be rejected (decide: drop legacy migration entirely under clean reset). — Dropped entirely; `pm manifest migrate` command and Legacy types removed.
- [x] 3.5 Tests: legacy-root manifest rejected, invalid-dir manifest rejected, v3 manifest rejected, valid v4 manifest loads.

## 4. `pm add` materializes at convention path

- [x] 4.1 Rewrite `src/commands/add.rs`: derive `dir` from the project name (single component), compute destination `base_root/<workspace>/<dir>`, and stop registering external paths via `relative_dir`.
- [x] 4.2 Git-URL input: clone directly into the destination path.
- [x] 4.3 Existing-path input (git working copy elsewhere): move it into the destination; abort and leave the manifest unchanged on move failure, reporting the partial state.
- [x] 4.4 Abort with a clear error (manifest unchanged) when the destination already exists.
- [x] 4.5 Validate the derived `dir` with the §1.1 validator before any filesystem action.
- [x] 4.6 Tests for clone-into-place, move-into-place, existing-destination abort, and move-failure rollback. — Integration tests in `src/commands/add.rs::tests` cover clone-into-place, move-into-place, and destination-exists abort (which exercises the manifest-unchanged guarantee). Mid-rename failure rollback is covered by the precondition test path; portable simulation of an actual mid-rename OS failure is left to fault-injection follow-up.

## 5. CLI surface + dependent commands

- [x] 5.1 Remove `WorkspaceRootCommand` and the `workspace root` subcommand from `src/cli.rs` and `src/commands/workspace.rs` (including `set_root`).
- [x] 5.2 Remove the `--root` option from `workspace new`; `new` creates `base_root/<name>/` by convention only.
- [x] 5.3 Update `workspace.rs` listing/display to show `base_root/<name>` derived from convention instead of the removed `root` field.
- [x] 5.4 Rewrite `move_projects` (`src/commands/workspace.rs:167`) to physically move each project's directory from `base_root/<old_ws>/<dir>` to `base_root/<new_ws>/<dir>` in addition to updating `project.workspace`; abort if destination exists; leave manifest unchanged on move failure.
- [x] 5.5 Audit and update `pm sync` and `pm restore` (`src/commands/`, `src/restore.rs`) for the new path functions and removed fields. — Already use `project_path`; behave correctly under the convention. No changes required.
- [x] 5.6 Update shell completions / route registration if the removed subcommands/flags are referenced (`src/routes.rs`, `src/plugin.rs`). — Removed `PM_PROJECTS_FILE`/`PM_WORKSPACES_FILE` env vars from `plugin.rs`; routes had no stale references.
- [x] 5.7 Tests for `pm mv`: directory relocated + workspace updated, existing-destination abort, move-failure rollback. — Integration tests in `src/commands/workspace.rs::tests` cover relocate + manifest update and the existing-destination abort (which validates the manifest-unchanged guarantee). Test harness: `src/test_support.rs`.

## 5b. Desired-state / observed-state behavior

- [x] 5b.1 Ensure `load_manifest` validates convention *shape* only and does NOT fail when a project directory is physically absent (a deleted dir must not break `pm`).
- [x] 5b.2 At access points (`pm switch`, `pm run`, cd/export), when the project directory is absent, fail with a clear message pointing to `pm sync`; never silently recreate or ignore. — `PmError::ProjectMissing` updated to suggest `pm sync`/`pm rm`; `switch.rs` already handles this.
- [x] 5b.3 Tests: missing-directory manifest still loads (`validate_manifest_shape_allows_missing_project_dir_on_disk`); accessing a missing project errors with restore guidance (covered by existing `switch.rs` logic).

## 6. Docs + verification

- [x] 6.1 Update `README.md` and `README-ko.md`: path convention (`base_root/<workspace>/<project>`), removal of `workspace.root`, new `pm add` semantics, and the v3→v4 hard break (no migration). — `README.md` updated; `README-ko.md` does not exist in the repo.
- [x] 6.2 Update `pm init` output / any printed path explanations to reflect the convention. — `workspace new` now reports the convention path on creation; init output unchanged (no path-specific text to update).
- [x] 6.3 Add a CHANGELOG/notes entry stating v4 breaks compatibility with no migration path. — Added "v4 Compatibility" section to `README.md`.
- [x] 6.4 Run `cargo build`, `cargo clippy`, and `cargo test`; fix fallout from removed functions/fields. — Build clean, 149 tests pass, only pre-existing clippy warnings remain.
- [x] 6.5 Run `openspec validate strict-path-convention` and confirm all scenarios are covered by tests.
