# path-convention Specification

## Purpose

Define the deterministic mapping between `pm`'s manifest entries and their
on-disk locations. Every workspace and project must resolve to a single
predictable path computed from `base_root` and names alone, so that a manifest
is portable across machines and the filesystem layout is a function of
declared intent rather than per-machine history.

This spec also draws the source-of-truth boundary that the rest of `pm`
relies on: the manifest is the authority for *what* is managed (membership
and metadata); the filesystem is the authority for *whether* a directory
physically exists. Loading validates path-convention shape only and never
fails because a directory has been deleted out-of-band — drift between
declared and observed state is surfaced by reconcile tooling, not hidden.

## Requirements

### Requirement: Initialization materializes the default workspace root

For a fresh configuration, the system SHALL default `base_root` to `~/ws`,
persist that value in `config.json`, and create the root together with the
default manifest workspace directories. The persisted value MAY be edited
later by the user.

If `~/ws` is already occupied by a path that is not owned by the existing PM
configuration, initialization SHALL report the collision and ask whether to
use `~/.ws` instead. On confirmation, the system SHALL persist `~/.ws` as
`base_root` and materialize the workspace directories there. On rejection,
initialization SHALL abort before writing new configuration.

#### Scenario: Fresh init creates the default root
- **GIVEN** neither `~/ws` nor a PM configuration exists
- **WHEN** the user runs `pm init`
- **THEN** `config.json` records `base_root = "~/ws"`
- **AND** `~/ws/default` and `~/ws/.trash` exist as directories

#### Scenario: Existing default root offers the hidden fallback
- **GIVEN** no PM configuration exists
- **AND** `~/ws` is already occupied
- **WHEN** the user runs `pm init`
- **THEN** PM reports that the default root already exists
- **AND** asks whether to use `~/.ws`

#### Scenario: User accepts the hidden fallback
- **GIVEN** `~/ws` is already occupied and `~/.ws` is available
- **WHEN** the user accepts the fallback prompt
- **THEN** `config.json` records `base_root = "~/.ws"`
- **AND** `~/.ws/default` and `~/.ws/.trash` exist as directories

#### Scenario: User rejects the hidden fallback
- **GIVEN** `~/ws` is already occupied
- **WHEN** the user rejects the fallback prompt
- **THEN** initialization aborts without writing new configuration

### Requirement: Workspace directory location is determined by name

The system SHALL place every workspace directory at `<base_root>/<workspace_name>/`, where `base_root` is the single configurable root from global configuration. Workspaces SHALL NOT override their physical location through manifest fields or any other mechanism.

#### Scenario: Workspace path resolves from name alone
- **WHEN** the system computes the directory of a workspace named `acme`
- **THEN** it returns `<base_root>/acme/` regardless of any other manifest data

#### Scenario: System workspace follows the same rule
- **WHEN** the system resolves the path of a system workspace such as `.trash`
- **THEN** it returns `<base_root>/.trash/`

#### Scenario: No root override field exists on workspace
- **WHEN** a developer inspects the `Workspace` model
- **THEN** there is no `root` (or equivalent override) field for physical location

### Requirement: Project directory name must be a single safe component

The system SHALL require every `Project.dir` value to be a single directory component that is non-empty, contains no path separators (`/` or `\`), is not `.` or `..`, and does not start with an absolute-path marker. Violations SHALL be rejected with a clear error identifying the offending project and rule.

#### Scenario: Single-name directory is accepted
- **WHEN** a project is added with `dir = "frontend"`
- **THEN** the system accepts it

#### Scenario: Nested directory name is rejected
- **WHEN** a project is added or loaded with `dir = "web/frontend"`
- **THEN** the system rejects it with an error referencing the path-separator rule

#### Scenario: Parent traversal is rejected
- **WHEN** a project is added or loaded with `dir = ".."` or `dir = "../escape"`
- **THEN** the system rejects it with an error referencing the parent-traversal rule

#### Scenario: Absolute path is rejected
- **WHEN** a project is added or loaded with `dir = "/abs/path"` or `dir = "C:\\abs"`
- **THEN** the system rejects it with an error referencing the absolute-path rule

#### Scenario: Empty directory name is rejected
- **WHEN** a project is added or loaded with `dir = ""`
- **THEN** the system rejects it with an error referencing the empty-name rule

### Requirement: Project absolute path is deterministic

The system SHALL compute every project's absolute filesystem path as `<base_root>/<workspace_name>/<project.dir>` and SHALL NOT consult any other manifest field or environment variable for location.

#### Scenario: Project path is name composition
- **WHEN** the system resolves the path of project `web` in workspace `acme` with `base_root = ~/ws`
- **THEN** it returns `~/ws/acme/web`

#### Scenario: Same manifest yields same paths on every machine
- **WHEN** two machines load the same manifest with the same `base_root`
- **THEN** they compute identical absolute paths for every project

### Requirement: Manifest loading validates the path convention

The system SHALL validate every workspace and project entry against the path convention at manifest load time and SHALL fail loading with an error listing all violations rather than silently normalizing or accepting them.

#### Scenario: Legacy manifest with workspace root override fails to load
- **WHEN** the system loads a manifest containing a workspace `root` override field
- **THEN** loading fails with an error identifying the unsupported field

#### Scenario: Manifest with invalid project dir fails to load
- **WHEN** the system loads a manifest where any project has an invalid `dir`
- **THEN** loading fails with an error identifying the project and the violated rule

#### Scenario: Valid manifest loads successfully
- **WHEN** every workspace and project in the manifest satisfies the convention
- **THEN** loading succeeds

### Requirement: `pm add` materializes projects at the convention path

The `pm add` command SHALL ensure that after success the project's working copy exists at `<base_root>/<workspace>/<project.dir>`. When a source path is provided that is outside this location, the command SHALL move it into place; when a git URL is provided, the command SHALL clone directly into place. The command SHALL NOT register external paths as projects.

#### Scenario: Adding a git URL clones into the convention path
- **WHEN** the user runs `pm add` with a git URL in workspace `acme` with name `web`
- **THEN** the repository is cloned at `<base_root>/acme/web`

#### Scenario: Adding an existing external clone moves it into place
- **WHEN** the user runs `pm add` pointing to a git working copy at an external path
- **THEN** the working copy is moved to `<base_root>/<workspace>/<project>` before manifest update

#### Scenario: Existing destination aborts the add
- **WHEN** the destination `<base_root>/<workspace>/<project>` already exists
- **THEN** the command fails with an error and leaves the manifest unchanged

#### Scenario: Move failure leaves manifest unchanged
- **WHEN** moving an external working copy into place fails partway
- **THEN** the manifest is not updated and the error reports the partial state

### Requirement: Manifest schema version is bumped to v4

The manifest schema version SHALL be `4` for manifests that conform to the strict path convention. Manifests at version `3` or below SHALL be rejected at load time with a clear incompatibility error. No automatic migration is provided; the user is expected to recreate workspaces under the new convention.

#### Scenario: v4 manifest loads
- **WHEN** the system loads a manifest declaring schema version `4` that satisfies the convention
- **THEN** loading succeeds

#### Scenario: v3 manifest is rejected
- **WHEN** the system loads a manifest declaring schema version `3`
- **THEN** loading fails with an incompatibility message referencing the v4 convention and instructing the user to recreate workspaces

### Requirement: `pm mv` moves files and updates membership together

Because a project's path is `<base_root>/<workspace>/<project.dir>`, moving a project to another workspace SHALL update the manifest membership AND physically relocate the directory to the new convention path. The command SHALL NOT update the manifest without moving the files, and SHALL NOT move files without updating the manifest.

#### Scenario: Moving a project relocates its directory
- **WHEN** the user moves project `web` from workspace `acme` to workspace `labs`
- **THEN** the directory is moved from `<base_root>/acme/web` to `<base_root>/labs/web` and the manifest records workspace `labs`

#### Scenario: Existing destination aborts the move
- **WHEN** the destination `<base_root>/<target_workspace>/<dir>` already exists
- **THEN** the command fails and leaves both the filesystem and the manifest unchanged

#### Scenario: Move failure leaves manifest unchanged
- **WHEN** the physical move fails partway
- **THEN** the manifest is not updated and the error reports the partial state

### Requirement: Missing project directory is reported, not silently handled

Manifest is the source of truth for project membership and metadata; the filesystem is authoritative for physical presence. Manifest loading SHALL validate path-convention shape only and SHALL NOT fail because a project directory is absent. When a command needs a project's working copy and the directory is absent, the system SHALL report the absence with guidance to restore it rather than silently creating or ignoring it.

#### Scenario: A missing directory does not break manifest loading
- **WHEN** a manifest entry's directory has been deleted outside `pm`
- **THEN** loading still succeeds and other commands continue to work

#### Scenario: Accessing a missing project reports clearly
- **WHEN** the user switches to or runs a project whose directory is absent
- **THEN** the command fails with a clear message directing the user to restore it (e.g. `pm sync`) and does not silently recreate it

### Requirement: Current-project detection uses the convention

When determining which project the current working directory belongs to, the system SHALL check that the working directory is within `<base_root>` and then look up the matching workspace and project by directory components rather than scanning all project paths.

#### Scenario: cwd inside a project resolves directly
- **WHEN** the current working directory is `<base_root>/acme/web/src`
- **THEN** the system reports project `web` in workspace `acme` without scanning unrelated projects

#### Scenario: cwd outside base_root returns no current project
- **WHEN** the current working directory is outside `<base_root>`
- **THEN** the system reports no current project
