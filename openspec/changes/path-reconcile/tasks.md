## 1. Scanner + classifier (`src/reconcile.rs`)

- [ ] 1.1 Create `src/reconcile.rs` and wire it from `main.rs`.
- [ ] 1.2 Define `DiskEntry { workspace: String, dir: String, path: PathBuf, is_git: bool, remote: Option<String> }`.
- [ ] 1.3 Implement `scan_base_root(config: &Config) -> Vec<DiskEntry>`: walk two levels of `expand_path(config.base_root)`, skip symlinks and non-directories, populate `is_git` via `git::is_git_repo`, populate `remote` via `git::get_remote_url`.
- [ ] 1.4 Define `ReconcileReport { orphans: Vec<&Project>, untracked_git: Vec<DiskEntry>, untracked_non_git: Vec<DiskEntry>, conflicts: Vec<ConflictEntry> }`.
- [ ] 1.5 Implement `classify(manifest: &Manifest, disk: &[DiskEntry]) -> ReconcileReport`: orphans are manifest projects without a matching disk entry; untracked entries are disk dirs not in the manifest; conflicts are existing entries that disagree with the manifest (non-git at expected path, remote mismatch).
- [ ] 1.6 Skip system workspaces (`.`-prefix) when collecting untracked entries; their contents are tracked-but-trashed.
- [ ] 1.7 Unit tests: pure classifier with hand-built `Manifest` + `Vec<DiskEntry>` covering all four buckets.

## 2. `pm check` integration (read-only)

- [ ] 2.1 Update `src/commands/check.rs` to call `scan_base_root` + `classify` and print all four sections in addition to today's per-project loop.
- [ ] 2.2 New status labels: `untracked` (git) and `untracked-non-git` (informational), distinct from the existing `missing-restorable`, `missing-unrestorable`, and conflict labels.
- [ ] 2.3 Ensure `pm check` continues to make zero filesystem and manifest changes.
- [ ] 2.4 Tests: untracked git dir surfaced, untracked non-git dir labeled differently, `.trash` contents not surfaced as untracked.

## 3. `pm sync` adopt mode

- [ ] 3.1 Add `--adopt` flag to `pm sync` in `src/cli.rs`; thread it through `commands/sync.rs::run`.
- [ ] 3.2 Implement `reconcile::adopt_one(...) -> Result<AdoptOutcome>`: validate inferred name (`validate_project_dir` + project-name regex), verify the workspace exists in the manifest, check for name collisions, derive `remote`/`repo_slug`, construct a `Project`, return it without mutating yet.
- [ ] 3.3 In `commands/sync.rs`, when `--adopt`, iterate the `untracked_git` set and call `adopt_one`. Confirm each item interactively (`prompt_yes_no`) unless `--yes`.
- [ ] 3.4 Apply approved adopts to the manifest in a single `save_state` at the end.
- [ ] 3.5 Tests for adopt: registers a git dir with origin (TestEnv + `init_git_repo` + `git remote add`), skips name collisions, skips invalid names, skips missing-workspace entries.

## 4. `pm sync` prune mode

- [ ] 4.1 Add `--prune` flag to `pm sync` in `src/cli.rs`; thread it through `commands/sync.rs::run`.
- [ ] 4.2 Implement `reconcile::prune_one(manifest: &mut Manifest, history: &mut HistoryData, project_name: &str, config: &Config) -> Result<PruneOutcome>`: locate project, append `HistoryAction::Unregistered` snapshot (with `repo_slug`, `dir`, `remote`, computed `path`), remove from manifest.
- [ ] 4.3 In `commands/sync.rs`, when `--prune`, iterate the `orphans` set and confirm each item interactively (unless `--yes`) before calling `prune_one`.
- [ ] 4.4 Save manifest and history in one `save_state` + `save_history` pass at the end.
- [ ] 4.5 Tests for prune: removes orphan + writes history; per-item confirmation honored (mocked via test that exercises `--yes`); plain `pm sync` does not prune.

## 5. Dry-run + non-interactive guards

- [ ] 5.1 Add `--dry-run` flag to `pm sync`.
- [ ] 5.2 When `--dry-run`, print the full plan (restores, adopts, prunes) and return early without mutating.
- [ ] 5.3 When `--adopt` or `--prune` runs without a TTY and without `--yes`, return `PmError::NonInteractiveRestore` (or a new sibling) referencing `--yes`.
- [ ] 5.4 Tests: dry-run touches nothing; non-interactive without `--yes` errors out; `--yes` applies without prompting.

## 6. Default behavior preserved

- [ ] 6.1 Verify `pm sync` (no flags) keeps the existing restore-only behavior — neither adopts nor prunes.
- [ ] 6.2 Regression test: plain `pm sync` with both untracked dirs and orphan entries present touches only the orphan-with-remote restore path.

## 7. Docs + verification

- [ ] 7.1 README: update `pm check` and `pm sync` sections; add a short "drift workflow" example.
- [ ] 7.2 Update `pm sync` help text to describe the new flags.
- [ ] 7.3 `cargo build`, `cargo clippy`, `cargo test`.
- [ ] 7.4 `openspec validate path-reconcile` and confirm all scenarios are covered.
