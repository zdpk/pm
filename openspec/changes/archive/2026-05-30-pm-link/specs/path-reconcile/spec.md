## ADDED Requirements

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

`pm sync` SHALL exclude every project that has a link target from the
restore/clone plan, even when the project carries a remote and its symlink is
missing. A linked project's bytes live only at its local target; there is
nothing to clone. A broken link SHALL be reported, never auto-restored from a
remote.

#### Scenario: A linked project with a remote is never cloned
- **GIVEN** a linked project `widgets` with a recorded remote whose symlink is missing
- **WHEN** the user runs `pm sync`
- **THEN** `widgets` is not cloned
- **AND** it is reported as a broken link rather than restored

#### Scenario: A link unavailable on another machine is left alone
- **GIVEN** a manifest with linked project `widgets` whose target path does not exist on this machine and no symlink is present
- **WHEN** the user runs `pm sync` with no flags
- **THEN** `widgets` is neither cloned nor removed
- **AND** `pm check` reports it as a link unavailable on this machine
