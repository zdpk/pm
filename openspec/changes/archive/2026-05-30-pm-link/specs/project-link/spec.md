## ADDED Requirements

### Requirement: `pm link` registers an external git repo via a symlink

The `pm link <path>` command SHALL register an existing directory as a project
by creating a symlink at the convention path `<base_root>/<workspace>/<dir>`
that points at the canonicalized `<path>`, and by adding a corresponding entry
to the manifest. The command SHALL NOT move, copy, or modify the contents of
`<path>`. The workspace defaults to the current workspace and the project name
/ dir default to the basename of `<path>`, each overridable by flags
(`--workspace`, `--name`); `--tags` and `--note` SHALL be accepted as for
`pm add`.

#### Scenario: Link creates a symlink and registers the project
- **GIVEN** a git repository exists at `/ext/widgets` and the current workspace is `acme`
- **WHEN** the user runs `pm link /ext/widgets`
- **THEN** a symlink is created at `<base_root>/acme/widgets` pointing at the canonical path of `/ext/widgets`
- **AND** the manifest gains a project named `widgets` in workspace `acme` with `dir = "widgets"` and a recorded link target
- **AND** the contents of `/ext/widgets` are unchanged and not moved

#### Scenario: Link honors explicit name and workspace
- **GIVEN** a git repository exists at `/ext/widgets` and a workspace `labs` exists
- **WHEN** the user runs `pm link /ext/widgets --workspace labs --name gizmo`
- **THEN** the symlink is created at `<base_root>/labs/gizmo`
- **AND** the manifest entry has name `gizmo`, workspace `labs`, and `dir = "gizmo"`

### Requirement: Link target must be a git repository

`pm link` SHALL require that the resolved `<path>` is an existing git
repository. If `<path>` does not exist or is not a git repository, the command
SHALL abort with a clear error and SHALL NOT create a symlink or modify the
manifest.

#### Scenario: Non-git target is rejected
- **GIVEN** a directory `/ext/notes` that is not a git repository
- **WHEN** the user runs `pm link /ext/notes`
- **THEN** the command fails with an error stating the target is not a git repository
- **AND** no symlink is created and the manifest is unchanged

#### Scenario: Missing target is rejected
- **GIVEN** no filesystem entry exists at `/ext/ghost`
- **WHEN** the user runs `pm link /ext/ghost`
- **THEN** the command fails with an error that the path does not exist
- **AND** the manifest is unchanged

### Requirement: Link aborts when the convention path is occupied

`pm link` SHALL abort if anything already exists at the target convention path
`<base_root>/<workspace>/<dir>` (a real directory, a file, or an existing
symlink) or if the manifest already contains a project with the chosen name.
The abort SHALL leave both the filesystem and the manifest unchanged.

#### Scenario: Occupied convention path aborts the link
- **GIVEN** a directory already exists at `<base_root>/acme/widgets`
- **WHEN** the user runs `pm link /ext/widgets` resolving to that path
- **THEN** the command fails reporting the path is occupied
- **AND** no symlink replaces the existing entry and the manifest is unchanged

#### Scenario: Name collision aborts the link
- **GIVEN** the manifest already contains a project named `widgets`
- **WHEN** the user runs `pm link /ext/widgets`
- **THEN** the command fails reporting a name collision
- **AND** the manifest and filesystem are unchanged

### Requirement: Manifest records the canonical link target

A linked project's manifest entry SHALL carry the canonical absolute path of
its target. The presence of this link target SHALL be the authoritative signal
that the project is a deliberate symlink rather than a managed (moved/cloned)
project. A project without a link target SHALL retain its existing managed
behavior unchanged.

#### Scenario: Link target is stored canonically
- **GIVEN** the user links `/ext/../ext/widgets` (a non-canonical path) into `acme`
- **WHEN** the link succeeds
- **THEN** the manifest entry stores the canonicalized target `/ext/widgets`

#### Scenario: Managed projects carry no link target
- **GIVEN** a project added via `pm add` (clone or move)
- **WHEN** its manifest entry is inspected
- **THEN** it has no link target and is treated as a managed project

### Requirement: `pm rm` of a linked project removes only the symlink

When removing a project that has a link target, `pm rm` SHALL delete only the
symlink node at the convention path and SHALL NEVER delete, recurse into, or
otherwise modify the external target. The `--recursive` flag SHALL NOT cause
deletion of the target's contents for a linked project.

#### Scenario: Removing a link preserves the external target
- **GIVEN** a linked project `widgets` whose target `/ext/widgets` contains files
- **WHEN** the user runs `pm rm widgets`
- **THEN** the symlink at `<base_root>/acme/widgets` is removed
- **AND** the manifest entry is removed
- **AND** the files under `/ext/widgets` still exist and are unchanged

#### Scenario: Recursive removal of a link does not touch the target
- **GIVEN** a linked project `widgets` whose target `/ext/widgets` contains files
- **WHEN** the user runs `pm rm widgets --recursive`
- **THEN** only the symlink is removed and `/ext/widgets` and its contents survive

### Requirement: `pm mv` of a linked project relocates the symlink only

`pm mv` SHALL relocate a linked project by moving only the symlink node to the
new convention path and updating the project's `workspace`/`dir` in the
manifest. The link target SHALL be preserved unchanged, and the external
directory SHALL NOT be moved.

#### Scenario: Moving a link relocates the pointer, not the target
- **GIVEN** a linked project `widgets` in workspace `acme` targeting `/ext/widgets`
- **WHEN** the user moves it to workspace `labs`
- **THEN** the symlink now lives at `<base_root>/labs/widgets` and still points at `/ext/widgets`
- **AND** the manifest entry has workspace `labs` with the same link target
- **AND** `/ext/widgets` is not moved

### Requirement: `pm link` is supported only on Unix platforms

`pm link` SHALL create symlinks only on Unix platforms. On non-Unix platforms
the command SHALL fail with a clear "not supported on this platform" error and
SHALL NOT modify the manifest.

#### Scenario: Link is rejected on a non-Unix platform
- **GIVEN** pm is running on a non-Unix platform
- **WHEN** the user runs `pm link /ext/widgets`
- **THEN** the command fails with a platform-not-supported error
- **AND** the manifest is unchanged
