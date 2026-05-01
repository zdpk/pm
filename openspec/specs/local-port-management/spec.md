# local-port-management Specification

## Purpose
TBD - created by archiving change shared-local-infra. Update Purpose after archive.
## Requirements
### Requirement: Per-project port allocation kinds
The system SHALL allocate ports per project for the following kinds only: `Frontend`, `Backend`, `Infra`. The kinds `Database` and `Redis` SHALL be classified as shared and MUST NOT be allocated per project.

#### Scenario: assign with default kinds allocates only Backend
- **WHEN** the user runs `pm ports assign my-project` without `--kind`
- **THEN** the system assigns a port for `Backend` only and prints the assignment
- **AND** no `Database` or `Redis` entries are created in `projects.<key>.services`

#### Scenario: assign rejects shared kinds
- **WHEN** the user runs `pm ports assign my-project --kind database`
- **THEN** the command exits with a non-zero status
- **AND** stderr includes a message directing the user to `pm ports shared`

#### Scenario: assign accepts non-shared kinds explicitly
- **WHEN** the user runs `pm ports assign my-project --kind frontend --kind infra`
- **THEN** the system assigns ports for `Frontend` and `Infra` from their configured ranges

### Requirement: Shared infrastructure ports
The system SHALL store a single global Postgres port and a single global Redis port in `PortsData.shared`. Defaults SHALL be `postgres_port = 5432` and `redis_port = 6379`.

#### Scenario: fresh ports.json contains shared defaults
- **WHEN** `pm ports list` runs and `~/.config/pm/ports.json` does not exist
- **THEN** the file is created with `version = 2` and `shared = { postgres_port: 5432, redis_port: 6379 }`

#### Scenario: shared ports survive project release
- **WHEN** the user runs `pm ports release my-project`
- **THEN** the project entry is removed
- **AND** `shared.postgres_port` and `shared.redis_port` retain their values

### Requirement: `pm ports shared` command
The system SHALL provide a `pm ports shared` subcommand that views and updates the shared Postgres/Redis ports. With no flags it prints the current values; with `--postgres <port>` or `--redis <port>` it updates the corresponding field and persists `ports.json`.

#### Scenario: read current shared ports
- **WHEN** the user runs `pm ports shared`
- **THEN** stdout includes both `postgres` and `redis` lines with their current ports

#### Scenario: update postgres port
- **WHEN** the user runs `pm ports shared --postgres 5433`
- **THEN** `shared.postgres_port` is persisted as `5433`
- **AND** `shared.redis_port` is unchanged

#### Scenario: update both ports
- **WHEN** the user runs `pm ports shared --postgres 5433 --redis 6380`
- **THEN** both values are persisted

#### Scenario: reject invalid port
- **WHEN** the user runs `pm ports shared --postgres 0`
- **THEN** the command exits non-zero and `ports.json` is unchanged

### Requirement: ports.json schema v2 with automatic migration
The system SHALL persist `~/.config/pm/ports.json` with `version = 2` and a `shared` field. On load, if the file has `version = 1` or omits `version`, the system SHALL migrate it: back up the original to `ports.json.bak.v1`, remove all per-project services whose `kind` is `Database` or `Redis`, inject default `shared`, set `version = 2`, and save.

#### Scenario: v1 file is migrated on load
- **GIVEN** `~/.config/pm/ports.json` exists with `version = 1` and a project containing a `db` service
- **WHEN** any `pm ports` command runs
- **THEN** the file is rewritten with `version = 2`, the `db` service is removed, and `shared` is present with defaults
- **AND** `ports.json.bak.v1` exists with the original content

#### Scenario: v2 file loads without modification
- **GIVEN** `~/.config/pm/ports.json` already has `version = 2`
- **WHEN** any `pm ports` command runs
- **THEN** the file content is unchanged on disk (aside from explicit user-requested writes)
- **AND** no backup file is created

### Requirement: Local database name normalization
The system SHALL derive the local Postgres database name as `{workspace}_{project}`, with characters outside `[a-z0-9_]` replaced by `_` and the entire string lowercased. The function MUST be deterministic and depend only on workspace and project names. The trailing `_local` suffix used in v0.3.0 is removed in v0.4.0 to match production database identifiers.

#### Scenario: hyphens become underscores
- **WHEN** workspace = `work` and project = `my-app`
- **THEN** the local database name is `work_my_app`

#### Scenario: case is normalized
- **WHEN** workspace = `Work` and project = `MyApp`
- **THEN** the local database name is `work_myapp`

#### Scenario: same project name in different workspaces yields different names
- **WHEN** workspaces `a` and `b` both contain a project named `api`
- **THEN** their database names are `a_api` and `b_api` respectively

#### Scenario: no _local suffix
- **WHEN** workspace = `work` and project = `api`
- **THEN** the local database name is `work_api` (no trailing `_local`)

### Requirement: `pm run` injects shared infrastructure environment variables
For each invocation of `pm run`, the system SHALL inject environment variables that point to the shared Postgres and Redis instances and to a project-scoped database name and Redis key prefix. The variables `LOCAL_POSTGRES_PORT`, `DATABASE_URL`, `LOCAL_REDIS_PORT`, `REDIS_URL`, and `REDIS_KEY_PREFIX` MUST be present whenever the project has any port allocation or shared infra is configured. The `DATABASE_URL` SHALL embed the database name as `{workspace}_{project}` (no `_local` suffix).

#### Scenario: postgres environment variables
- **GIVEN** `shared.postgres_port = 5432` and the project is `work/my-app`
- **WHEN** `pm run my-app -- env` is invoked
- **THEN** the spawned process environment includes `LOCAL_POSTGRES_PORT=5432`
- **AND** `DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/work_my_app`

#### Scenario: redis environment variables
- **GIVEN** `shared.redis_port = 6379` and the project is `work/my-app`
- **WHEN** `pm run my-app -- env` is invoked
- **THEN** the spawned process environment includes `LOCAL_REDIS_PORT=6379`
- **AND** `REDIS_URL=redis://127.0.0.1:6379`
- **AND** `REDIS_KEY_PREFIX=work:my-app`

#### Scenario: backend port still injected per project
- **GIVEN** the project has a `Backend` service assigned to port `20123`
- **WHEN** `pm run my-app -- env` is invoked
- **THEN** the process environment includes `APP_PORT=20123` and `APP_HOST=127.0.0.1`

### Requirement: `pm ports list` and `check` show shared section
The output of `pm ports list` and `pm ports check` SHALL include a dedicated section that displays the shared Postgres and Redis ports separately from per-project allocations.

#### Scenario: list output includes shared section header
- **WHEN** the user runs `pm ports list`
- **THEN** stdout contains a section labeled `SHARED` (or equivalent header) with `postgres` and `redis` rows
- **AND** the per-project table follows below

#### Scenario: check reports shared port availability
- **WHEN** the user runs `pm ports check`
- **AND** `shared.postgres_port` is currently bound by another process
- **THEN** the shared row for postgres reports a non-`free` status (e.g. `bound`)

