# local-dev-orchestrator Specification

## Purpose
TBD - created by archiving change local-dev-orchestrator. Update Purpose after archive.
## Requirements
### Requirement: `.proj.yaml` services schema
The `.proj.yaml` file SHALL accept an optional top-level `services:` map where each key is a service identifier (e.g., `front`, `back`) and each value is an object with optional fields `dir` (default: `.`), `dev_cmd`, `port_kind`, `framework`, and `path`. Missing `dev_cmd` and `port_kind` SHALL be inferred from the project (or service-level) `framework` value via a built-in defaults table. A `.proj.yaml` without a `services:` section SHALL behave as in v0.3.0 (no orchestrator features).

#### Scenario: services section enables orchestrator features
- **GIVEN** a project with `.proj.yaml` containing `services: { front: {} }`
- **WHEN** `pm run` is invoked with no arguments
- **THEN** the orchestrator activates and starts the `front` service

#### Scenario: missing services section preserves v0.3.0 behavior
- **GIVEN** a project whose `.proj.yaml` has no `services:` section
- **WHEN** `pm run -- pnpm dev` is invoked
- **THEN** the legacy v0.3.0 grammar runs `pnpm dev` with env vars injected
- **AND** no daemon is spawned

#### Scenario: framework defaults are applied
- **GIVEN** a service `front: { framework: nextjs }` with no `dev_cmd` or `port_kind`
- **WHEN** the orchestrator resolves the service definition
- **THEN** `dev_cmd` resolves to `pnpm next dev --turbopack`
- **AND** `port_kind` resolves to `frontend`

### Requirement: `pm run` orchestrated service startup
When invoked in a project whose `.proj.yaml` defines `services:`, `pm run` SHALL ensure shared infrastructure (Postgres, Redis) is running, ensure the project database exists, ensure the daemon is running, register hostname routes, and spawn the configured service commands. With no positional arguments, all services in the project SHALL start; with a service identifier (e.g., `pm run front`), only that service SHALL start.

#### Scenario: pm run with no args starts all services
- **GIVEN** `.proj.yaml` with services `front` and `back`
- **WHEN** `pm run` is invoked
- **THEN** both `front` and `back` are spawned
- **AND** routes for `front.<project>.<workspace>.localhost` and `back.<project>.<workspace>.localhost` are registered

#### Scenario: pm run with a service identifier
- **GIVEN** `.proj.yaml` with services `front` and `back`
- **WHEN** `pm run front` is invoked
- **THEN** only `front` is spawned
- **AND** the route for `front.<project>.<workspace>.localhost` is registered

#### Scenario: pm run with service and project identifiers
- **GIVEN** project `work/api` defines services `front` and `back`
- **WHEN** `pm run back api` is invoked from outside the project directory
- **THEN** only `back` of `work/api` is spawned

#### Scenario: legacy `--` mode still works
- **GIVEN** any project (with or without services)
- **WHEN** `pm run myproj -- echo hello` is invoked
- **THEN** the v0.3.0 behavior runs `echo hello` with env vars injected
- **AND** no service-mode logic activates

#### Scenario: token disambiguation prefers service over project
- **GIVEN** a project whose `.proj.yaml` services include `front`, AND another project named `front` exists in the manifest
- **WHEN** `pm run front` is invoked from the first project's directory
- **THEN** the orchestrator interprets `front` as a service identifier, not a project name

### Requirement: `*.localhost` hostname routing
The system SHALL register hostname routes of the form `<service>.<project>.<workspace>.localhost` for each running service and route HTTP requests on the daemon's listening port to the per-service local port based on the `Host` header. When the workspace is `default`, the system SHALL also register the shorter alias `<service>.<project>.localhost`.

#### Scenario: subdomain route is registered
- **GIVEN** the workspace is `work`, project is `api`, and service is `back` listening on `127.0.0.1:26918`
- **WHEN** the orchestrator starts the service
- **THEN** `routes.json` contains an entry `back.api.work.localhost` → `127.0.0.1:26918`

#### Scenario: default workspace alias
- **GIVEN** the workspace is `default`, project is `blog`, and service is `front` listening on `127.0.0.1:13042`
- **WHEN** the orchestrator starts the service
- **THEN** `routes.json` contains both `front.blog.default.localhost` and `front.blog.localhost` mapping to `127.0.0.1:13042`

#### Scenario: routing by Host header
- **GIVEN** the daemon is running on `127.0.0.1:7100` with a route for `front.api.work.localhost` → `127.0.0.1:13042`
- **WHEN** an HTTP request arrives at the daemon with `Host: front.api.work.localhost:7100`
- **THEN** the daemon proxies the request to `127.0.0.1:13042`

#### Scenario: unknown hostname returns 404
- **GIVEN** the daemon is running with no route for `unknown.localhost`
- **WHEN** an HTTP request arrives with `Host: unknown.localhost:7100`
- **THEN** the daemon responds with HTTP 404

### Requirement: Daemon auto-spawn (gpg-agent pattern)
On any orchestrator-mode `pm run` invocation, the system SHALL detect whether the daemon is alive (via PID file + control plane health check) and spawn it as a detached background process if not. The daemon SHALL share the same `pm` binary, entered via the hidden `pm __daemon` subcommand. The CLI invocation that triggered the spawn SHALL exit normally after registering its routes; the daemon SHALL persist independently.

#### Scenario: cold start spawns daemon
- **GIVEN** no `pm` daemon is running and `daemon.pid` is absent or stale
- **WHEN** `pm run` is invoked in an orchestrator-enabled project
- **THEN** a detached `pm __daemon` process is spawned
- **AND** `~/.config/pm/daemon.pid` is written with the new PID
- **AND** the CLI waits until the control plane responds to `/health` before exiting

#### Scenario: warm start reuses daemon
- **GIVEN** a `pm` daemon is already running (PID alive and `/health` responsive)
- **WHEN** `pm run` is invoked in an orchestrator-enabled project
- **THEN** no new daemon process is spawned
- **AND** routes are added by writing to `routes.json`

#### Scenario: stale PID file is recovered
- **GIVEN** `daemon.pid` exists but the PID is dead and the control plane port is free
- **WHEN** `pm run` is invoked
- **THEN** the stale PID file is overwritten and a new daemon is spawned

### Requirement: Daemon control plane
The daemon SHALL expose a small HTTP control plane (default `127.0.0.1:7101`, separate from the proxy port) supporting: `GET /health`, `POST /reload` (force routes.json reload), `POST /stop` (graceful shutdown), and `GET /status` (running services and routes). All endpoints SHALL bind to loopback only.

#### Scenario: pm proxy status reports daemon state
- **WHEN** the user runs `pm proxy status`
- **THEN** the command queries `/status` and prints PID, uptime, proxy port, and active route count

#### Scenario: pm proxy stop terminates daemon
- **WHEN** the user runs `pm proxy stop`
- **THEN** the CLI sends `POST /stop`
- **AND** the daemon performs graceful shutdown (closes listeners, flushes logs)
- **AND** subsequent `pm proxy status` reports "not running"

### Requirement: Docker auto-start of shared infrastructure
On orchestrator-mode `pm run`, the system SHALL ensure a shared Postgres container named `pm-local-db` (volume `pm-local-volume`, image configurable via `config.json`, default `postgres:16`) and a shared Redis container `pm-local-redis` (volume `pm-local-redis-volume`, default image `redis:7`) are running, unless port `5432`/`6379` is already bound by an external process. Containers SHALL persist after `pm run` exits and SHALL be stopped only by explicit `pm db stop`.

#### Scenario: containers auto-start when port is free
- **GIVEN** ports 5432 and 6379 are not in use
- **AND** Docker is installed and running
- **WHEN** `pm run` is invoked in an orchestrator-enabled project
- **THEN** containers `pm-local-db` and `pm-local-redis` are started (or created and started if absent)
- **AND** the volumes `pm-local-volume` and `pm-local-redis-volume` are reused across runs

#### Scenario: external Postgres is respected
- **GIVEN** an external process is bound to `127.0.0.1:5432`
- **WHEN** `pm run` is invoked
- **THEN** the system does NOT attempt to start `pm-local-db`
- **AND** continues to use the external Postgres

#### Scenario: containers persist after pm run exits
- **GIVEN** `pm run` was invoked and started `pm-local-db`
- **WHEN** the user stops their service (Ctrl+C) and `pm run` exits
- **THEN** `pm-local-db` is still running
- **AND** can be observed via `docker ps`

#### Scenario: pm db stop terminates containers
- **WHEN** the user runs `pm db stop`
- **THEN** containers `pm-local-db` and `pm-local-redis` are stopped (but volumes preserved)

#### Scenario: Docker missing is handled gracefully
- **GIVEN** Docker is not installed
- **WHEN** `pm run` is invoked in an orchestrator-enabled project
- **THEN** the command fails with a clear error message identifying Docker as the missing dependency
- **AND** suggests setting `dev.auto_start_docker: false` in `config.json` to disable auto-start

#### Scenario: auto-start disabled in config
- **GIVEN** `config.json` contains `dev.auto_start_docker: false`
- **WHEN** `pm run` is invoked
- **THEN** no Docker commands are executed
- **AND** the system assumes Postgres/Redis are available (or fails when services try to connect)

### Requirement: Automatic database creation
After ensuring Postgres is reachable on `127.0.0.1:5432`, the system SHALL check whether a database named `<workspace>_<project>` exists and SHALL create it if absent. The creation SHALL only proceed when the connection target is `127.0.0.1` or `localhost` (production hosts SHALL NOT trigger automatic CREATE DATABASE).

#### Scenario: missing database is created
- **GIVEN** Postgres is reachable and database `work_api` does not exist
- **WHEN** `pm run` is invoked for project `work/api`
- **THEN** the system executes `CREATE DATABASE "work_api"`
- **AND** the database appears in `\l` output

#### Scenario: existing database is reused
- **GIVEN** database `work_api` already exists
- **WHEN** `pm run` is invoked for project `work/api`
- **THEN** no `CREATE DATABASE` is executed
- **AND** the existing database is used

#### Scenario: non-loopback target skips creation
- **GIVEN** the user has overridden Postgres host to a remote address
- **WHEN** `pm run` is invoked
- **THEN** the system does NOT execute `CREATE DATABASE` automatically
- **AND** logs a message that auto-creation is disabled for non-loopback hosts

### Requirement: `pm logs` and `pm stop` commands
The system SHALL provide `pm logs [service] [project]` to tail per-service log files stored at `~/.config/pm/logs/<workspace>_<project>_<service>.log`, and `pm stop [service] [project]` to terminate one or all running services. With no positional arguments, both commands SHALL operate on all services of the current project.

#### Scenario: pm logs follows a single service
- **GIVEN** service `back` of project `work/api` is running and writing to its log file
- **WHEN** the user runs `pm logs back`
- **THEN** the command tails the log file in follow mode
- **AND** displays new lines as they are appended

#### Scenario: pm stop without args stops all services in project
- **GIVEN** services `front` and `back` of `work/api` are running
- **WHEN** the user runs `pm stop` from inside the project
- **THEN** both services receive SIGTERM (or platform equivalent)
- **AND** are removed from `routes.json`

#### Scenario: pm stop with service name
- **GIVEN** services `front` and `back` are running
- **WHEN** the user runs `pm stop front`
- **THEN** only `front` is terminated
- **AND** `back` continues running

### Requirement: Log file rotation
Each service log file at `~/.config/pm/logs/<workspace>_<project>_<service>.log` SHALL be rotated when it exceeds 10 MiB. The system SHALL keep at most 3 rotated files (`.log`, `.log.1`, `.log.2`, `.log.3`) and SHALL discard older rotations.

#### Scenario: rotation triggers at size threshold
- **GIVEN** a service log file has reached 10 MiB
- **WHEN** the next write would exceed the threshold
- **THEN** the current file is renamed to `.log.1`, prior `.log.1` to `.log.2`, etc.
- **AND** writes continue to a fresh `.log`

#### Scenario: oldest rotation is discarded
- **GIVEN** rotations `.log`, `.log.1`, `.log.2`, `.log.3` exist
- **WHEN** rotation triggers again
- **THEN** the previous `.log.3` is deleted before new rotation

### Requirement: Daemon listens on a non-privileged port
The daemon SHALL listen on `127.0.0.1:7100` by default for the proxy and `127.0.0.1:7101` for the control plane. Both ports SHALL be configurable via `config.json` keys `dev.proxy_port` and `dev.control_port`. Binding to privileged ports (80/443) is OUT OF SCOPE for this change.

#### Scenario: default proxy port is 7100
- **WHEN** the daemon starts with default configuration
- **THEN** it binds to `127.0.0.1:7100`
- **AND** hostname URLs reach services via `http://<host>.localhost:7100`

#### Scenario: configurable proxy port
- **GIVEN** `config.json` has `dev.proxy_port: 8080`
- **WHEN** the daemon starts
- **THEN** it binds to `127.0.0.1:8080` instead

### Requirement: Next.js convention — pnpm and Turbopack
For Next.js services, the system SHALL use `pnpm` as the package manager and Turbopack as the dev bundler by convention. When the framework is `nextjs` and the user has not provided a `dev_cmd`, the orchestrator SHALL resolve the dev command to `pnpm next dev --turbopack`. The bundled Next.js config templates (`configs/typescript/nextjs/`) SHALL include a `.npmrc` that enables pnpm-friendly defaults (`engine-strict=true`, `auto-install-peers=true`).

#### Scenario: nextjs default dev command uses pnpm and turbopack
- **GIVEN** `.proj.yaml` declares `services: { front: { framework: nextjs } }` with no `dev_cmd`
- **WHEN** `pm run front` is invoked
- **THEN** the spawned command is `pnpm next dev --turbopack`

#### Scenario: user override is respected
- **GIVEN** `.proj.yaml` declares `services: { front: { framework: nextjs, dev_cmd: "pnpm dev" } }`
- **WHEN** `pm run front` is invoked
- **THEN** the spawned command is `pnpm dev` (not the framework default)

#### Scenario: pm proj init -f nextjs writes .npmrc
- **GIVEN** the user runs `pm proj init -l ts -f nextjs` in a fresh directory
- **WHEN** the command applies bundled Next.js config files
- **THEN** the project directory contains `.npmrc` with at minimum `engine-strict=true` and `auto-install-peers=true`

#### Scenario: pm proj init warns if package-lock.json or yarn.lock exists
- **GIVEN** the user runs `pm proj init -l ts -f nextjs` in a directory that already contains `package-lock.json` (npm) or `yarn.lock`
- **WHEN** the command runs
- **THEN** stderr includes a warning recommending removal of the lock file and use of pnpm
- **AND** the command does not abort (warning only)

### Requirement: BREAKING migration notice for v0.3.0 users
On the first `pm run` after upgrading to v0.4.0, if a database matching the v0.3.0 pattern (`<workspace>_<project>_local`) is found in the local Postgres, the system SHALL emit a one-time stderr notice describing the new naming convention and suggesting a manual migration command. The notice SHALL NOT block execution.

#### Scenario: legacy database triggers notice
- **GIVEN** Postgres contains both `work_api_local` (legacy) and (will create) `work_api`
- **WHEN** `pm run` is invoked for `work/api` for the first time on v0.4.0
- **THEN** stderr contains a message identifying `work_api_local` and recommending `pg_dump work_api_local | psql work_api`
- **AND** the new `work_api` database is created and used

#### Scenario: no legacy database, no notice
- **GIVEN** Postgres has no `<workspace>_<project>_local` databases
- **WHEN** `pm run` is invoked
- **THEN** no migration notice is emitted

