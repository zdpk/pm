## MODIFIED Requirements

### Requirement: `pm sync` never clones or restores a linked project

`pm sync` SHALL never clone or restore a linked project's content from a remote — a link's bytes live only at its local target, so there is nothing to clone. `pm sync` SHALL, however, repair the symlink *pointer* when the recorded target exists: it recreates a missing symlink (`broken-link`) and replaces a drifted one (`link-drift`) so the convention path resolves to the recorded target. Repair SHALL touch only the symlink — never the external target and never a real directory. When the recorded target does not exist (including on another machine), or a real directory occupies the convention path (`link-replaced-by-dir`), `pm sync` SHALL report the condition and SHALL NOT mutate anything. Repairs SHALL follow sync's existing controls: shown under `--dry-run`, applied under the default confirmation (`--yes` bypasses), and refused in a non-interactive run without `--yes`.

#### Scenario: A linked project is never cloned from its remote
- **GIVEN** a linked project `widgets` with a recorded remote whose symlink and target are both missing
- **WHEN** the user runs `pm sync`
- **THEN** `widgets` is not cloned
- **AND** it is reported as a broken link rather than restored from the remote

#### Scenario: A broken link is repaired when its target exists
- **GIVEN** a linked project `widgets` recorded as `/ext/widgets` where `/ext/widgets` exists but the symlink at the convention path is missing
- **WHEN** the user runs `pm sync` and confirms (or passes `--yes`)
- **THEN** a symlink is created at the convention path pointing at `/ext/widgets`
- **AND** `widgets` is not cloned and `/ext/widgets` is unchanged

#### Scenario: A drifted link is corrected
- **GIVEN** a linked project `widgets` recorded as `/ext/widgets` (which exists) whose symlink currently resolves to `/other/widgets`
- **WHEN** the user runs `pm sync` and confirms
- **THEN** the symlink is replaced to point at `/ext/widgets`
- **AND** neither `/ext/widgets` nor `/other/widgets` is modified

#### Scenario: A link replaced by a real directory is not auto-repaired
- **GIVEN** a linked project `widgets` whose convention path is now a real directory
- **WHEN** the user runs `pm sync`
- **THEN** the condition is reported and the directory is left untouched
- **AND** no symlink is created

#### Scenario: A link unavailable on this machine is left alone
- **GIVEN** a manifest with linked project `widgets` whose recorded target does not exist on this machine and no symlink is present
- **WHEN** the user runs `pm sync` with no flags
- **THEN** `widgets` is neither cloned, repaired, nor removed
- **AND** `pm check` reports it as a link unavailable on this machine

#### Scenario: Repair is previewed by dry-run and refused non-interactively
- **GIVEN** a repairable broken link (recorded target exists, symlink missing)
- **WHEN** the user runs `pm sync --dry-run`
- **THEN** the planned repair is printed and no symlink is created
- **AND** running `pm sync` without `--yes` in a non-interactive context refuses with a non-interactive error

## ADDED Requirements

### Requirement: `pm check` marks repairable links

`pm check` SHALL annotate a reported broken or drifted link as *repairable* when its recorded target exists on disk, directing the user to `pm sync`; it SHALL NOT mark a link repairable when the recorded target is missing. The annotation SHALL be read-only and SHALL NOT mutate the manifest or filesystem.

#### Scenario: A broken link with an existing target is marked repairable
- **GIVEN** a linked project `widgets` whose recorded target exists but whose symlink is missing
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a broken link and marked repairable (pointing the user at `pm sync`)

#### Scenario: A broken link with a missing target is not marked repairable
- **GIVEN** a linked project `widgets` whose recorded target does not exist
- **WHEN** the user runs `pm check`
- **THEN** `widgets` is reported as a broken link and is not marked repairable
