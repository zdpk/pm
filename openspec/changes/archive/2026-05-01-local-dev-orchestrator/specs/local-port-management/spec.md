## MODIFIED Requirements

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
