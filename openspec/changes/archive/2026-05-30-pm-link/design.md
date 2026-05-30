## Context

v4 made `project_path = base_root/<workspace>/<dir>` a deterministic function
(strict-path-convention, decisions D1–D9). `pm add` registers a local
directory by **physically moving** it into that path. `path-reconcile` then
diffs the manifest (desired state) against a two-level filesystem scan
(observed state), and `scan_base_root` **skips all symlinks** to avoid
recursion and cross-filesystem surprises (`src/reconcile.rs:83,104`).

This change adds `pm link <path>`: register an existing **git** repo that
must stay where it lives, by placing a symlink at the convention path. The
central problem is that a symlinked project is currently invisible to the
scan, so the manifest-driven pass in `classify` (`src/reconcile.rs:151`) finds
no disk entry and flags it as an **orphan** — which makes `pm check` report it
missing and `pm sync` attempt a re-clone over it.

Constraints inherited from v4: the convention path must stay deterministic
(no per-project path override — that was removed in v4); the manifest is the
source of truth for membership/metadata, the filesystem for physical
presence; `pm sync` defaults must remain backward compatible (v0.5.0).

## Goals / Non-Goals

**Goals:**
- Register an external git repo at the convention path without moving it.
- Make `path-reconcile` distinguish a deliberate link from a missing dir, so
  `pm check`/`pm sync` stop misreporting and never clone over a link.
- Guarantee link operations never mutate the external target (`rm`, `mv`).
- Preserve the v4 convention: pm and the shell still resolve the project at
  `base_root/<ws>/<dir>`.

**Non-Goals:**
- Linking non-git directories (git-only this change).
- Cross-machine portability of a link — a linked project simply does not
  exist on another machine; `pm sync` there leaves it absent and never clones.
- Auto-adopting manually-created symlinks via `pm sync --adopt`.
- Windows symlink creation (Unix-first; see L8).

## Decisions

### L1 — Symlink at the convention path, not a manifest path-override

The link is realized as a filesystem symlink `base_root/<ws>/<dir> →
<target>`, **not** by storing an alternate path in the manifest and teaching
`project_path()` to honor it.

- **Why:** v4 deliberately *removed* `Workspace.root` and nested `Project.dir`
  overrides to make paths deterministic. Reintroducing a path override would
  undo that and force every path consumer (`state.rs`, shell integration,
  `detect_current_project`) to special-case linked projects. A symlink keeps
  the convention exact — only the filesystem redirects.
- **Alternative considered:** `Project.path_override: Option<String>`. Rejected
  — reverses a core v4 decision and spreads conditional logic across the
  codebase.

### L2 — `Project.link: Option<String>` is the desired-state marker

A new optional field on `Project` stores the canonical external target. Its
**presence** is what declares "this project is intentionally a symlink."
`None` = ordinary managed project (unchanged behavior).

- **Why:** reconcile keys its link-vs-orphan decision off intent declared in
  the manifest (desired state), not off what happens to be on disk. This is
  the same desired/observed split D8 established.
- **Alternative considered:** infer "linked" purely from a symlink existing on
  disk. Rejected — conflates a hand-made symlink with a pm-managed link, and
  loses the intent entirely on a machine where the symlink doesn't exist.
- Serialized with `skip_serializing_if = "Option::is_none"`, so existing v4
  manifests are unaffected and **no version bump is needed**.

### L3 — `scan_base_root` records project-level symlinks instead of skipping

At the **project** (second) level, a symlink entry is no longer skipped: it is
read via `read_link`/`canonicalize`, and recorded as a `DiskEntry` carrying a
new marker (`link_target: Option<PathBuf>`) plus the resolved `is_git`/`remote`
of the *target*. Workspace-level (first-level) symlinks are still skipped.

- **Why:** the observed state must include links for `classify` to reconcile
  them. Restricting the change to the second level keeps the workspace tier's
  recursion/cross-fs guarantees intact.
- **Note:** `DirEntry::metadata()` does not traverse symlinks, so
  `file_type().is_symlink()` already detects the link correctly; only the
  `continue` branch changes.
- **Alternative considered:** follow every symlink at both levels. Rejected —
  reintroduces the recursion/cross-fs risk the original skip avoided.

### L4 — Link-aware classification

`classify` cross-references the manifest `link` field against the observed
symlink. The cross-product:

```
manifest.link=Some(T)  +  symlink → canonicalize == T          → ✅ healthy (no report)
manifest.link=Some(T)  +  no entry / not a symlink, real dir   → ⚠️ LinkReplacedByDir (conflict)
manifest.link=Some(T)  +  symlink → canonicalize != T          → 🔗 LinkTargetDrift (conflict)
manifest.link=Some(T)  +  symlink present but target missing   → 🔗 BrokenLink (conflict)
manifest.link=Some(T)  +  nothing at path                      → 🔗 BrokenLink (conflict, not orphan)
manifest.link=None     +  symlink at path                      → ⚠️ UnexpectedSymlink (conflict)
manifest.link=None     +  (existing rules unchanged)           → orphan / untracked / conflict
```

New `ConflictKind` variants: `BrokenLink`, `LinkTargetDrift`,
`LinkReplacedByDir`, `UnexpectedSymlink`. A linked project is **never** an
orphan and **never** an adopt candidate.

- **Why:** replaces today's single false "orphan" outcome with precise,
  actionable diagnostics, each mapping to a distinct user fix.

### L5 — `pm sync` excludes linked projects from the restore plan

The restore plan (`src/commands/sync.rs:49`) iterates orphans-with-remote.
Since a linked project is never an orphan (L4), it falls out of restore
automatically — but we also explicitly skip any project with `link.is_some()`
when building restores, so a stray remote can never trigger a clone over a
link.

- A `BrokenLink` is reported by `pm check` and (optionally) re-symlinked by a
  future `pm link --repair`; it is **never** auto-cloned. There is no remote to
  restore *from* for a link — the bytes live only at the local target.
- **Cross-machine:** on another machine the target path does not exist and no
  symlink is present, so the entry classifies as `BrokenLink`. `pm sync`
  leaves it untouched (no clone, no prune unless `--prune`); `pm check`
  reports it as a link unavailable on this machine. This is the portability
  semantics the proposal chose (links are machine-local).

### L6 — git-only link target

`pm link <path>` requires `is_git_repo(canonicalize(path))`; otherwise it
aborts before touching the filesystem or manifest.

- **Why:** consistent with `pm sync --adopt`, which requires `is_git_repo`. pm
  is git-based; a non-git link has no remote/status semantics to manage.

### L7 — Link-safe `rm` and `mv`

- **`pm rm <linked>`:** removes the symlink node only (`fs::remove_file` on the
  link path — never `remove_dir_all`, which would delete through into the
  target), then removes the manifest entry. The external target is untouched.
  The `--recursive` flag is a no-op for links (nothing under the link to
  recurse into from pm's perspective).
- **`pm mv <linked>`:** moves the **symlink** to the new convention path
  (`fs::rename` of the link node relocates the link itself, not the target)
  and updates `workspace`/`dir` in the manifest. `link` target is preserved.
- **Why:** the defining safety property of a link is that pm manages the
  pointer, never the pointee. rm/mv must enforce this even though managed
  projects delete/relocate real bytes (D9).

### L8 — Symlink creation is Unix-only this change

`pm link` is gated with `#[cfg(unix)]`; on non-Unix it returns a clear
"not supported on this platform" error, mirroring `pm logs`/`pm stop`.

- **Why:** Windows symlinks require elevated privilege or developer mode;
  out of scope for v0.5.x.
- **Note:** the `link` manifest field is platform-neutral (a string), so a
  manifest authored on Unix still *loads* on Windows — only `pm link` creation
  and symlink *resolution* in scan are Unix-gated. On non-Unix, a linked entry
  classifies as `BrokenLink` (target/symlink unavailable), never a crash.

### L9 — Store the canonicalized absolute target

`Project.link` stores `canonicalize(<path>)` captured at link time.

- **Why:** drift detection (L4) compares `canonicalize(readlink(convention))`
  against the stored value; both must be in the same canonical form for the
  comparison to be reliable. A canonical absolute path is also a stable
  identity independent of the user's cwd at link time.
- **Trade-off:** canonicalization resolves intermediate symlinks, so the
  stored target is the real path, not the as-typed one. Acceptable — it's the
  physically meaningful location.

## Risks / Trade-offs

- **[Manifest carries a machine-local absolute path]** → Non-portable by
  nature. Mitigation: documented as a non-goal (L5); on other machines the
  entry is inert and reported as an unavailable link, never auto-acted-upon.
- **[Target deleted out-of-band]** → `BrokenLink`. Mitigation: `pm check`
  reports it; no destructive auto-action; user runs `pm rm` or re-links.
- **[Link points back inside `base_root`]** → A user could link a project to a
  path under `base_root`. Mitigation: scan follows a project-level symlink
  exactly one hop and never recurses, so no scan loop; the configuration is
  unusual but not harmful. Not blocked.
- **[Symlink vs target stat confusion]** → Must use `symlink_metadata` to
  detect the link node and a separate stat of the resolved target to check
  existence/git-ness. Mitigation: encode this explicitly in `scan_base_root`.
- **[`rm` accidentally deleting through the link]** → Catastrophic data loss.
  Mitigation: L7 mandates `remove_file` on the link node, never a recursive
  delete; covered by a dedicated test asserting the target survives `pm rm`.

## Migration Plan

None. Purely additive: a new command, a new optional manifest field
(`skip_serializing_if`), new reconcile diagnostic categories. Existing v4
manifests load and behave identically; `pm check`/`pm sync` defaults are
unchanged for projects without a `link` field. No version bump.

## Open Questions

- Should a future `pm link --repair` (recreate a missing/drifted symlink when
  the target still exists) ship in this change or a follow-up? Leaning
  follow-up — this change establishes the model; repair is additive.
- Should `pm check` group link diagnostics under a dedicated "Links" section or
  inline them with other conflicts? Resolve during specs/implementation.
