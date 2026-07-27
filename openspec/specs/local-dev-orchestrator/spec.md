# local-dev-orchestrator Specification

## Purpose
Define the opt-in local development orchestrator: PM-managed service processes,
dynamic local ports, shared hostname routing, shared infrastructure, and the
read-only page that shows every application currently managed by PM.
## Requirements
### Requirement: `.project.yaml` services schema
The `.project.yaml` file SHALL accept an optional top-level `services:` map where each key is a service identifier (e.g., `front`, `back`) and each value is an object with optional fields `dir` (default: `.`), `dev_cmd`, `port_kind`, `framework`, and `path`. Missing `dev_cmd` and `port_kind` SHALL be inferred from the project (or service-level) `framework` value via a built-in defaults table. A `.project.yaml` without a `services:` section SHALL behave as in v0.3.0 (no orchestrator features).

#### Scenario: services section enables orchestrator features
- **GIVEN** a project with `.project.yaml` containing `services: { front: {} }`
- **WHEN** `pm run` is invoked with no arguments
- **THEN** the orchestrator activates and starts the `front` service

#### Scenario: missing services section preserves v0.3.0 behavior
- **GIVEN** a project whose `.project.yaml` has no `services:` section
- **WHEN** `pm run -- pnpm dev` is invoked
- **THEN** the legacy v0.3.0 grammar runs `pnpm dev` with env vars injected
- **AND** no daemon is spawned

#### Scenario: framework defaults are applied
- **GIVEN** a service `front: { framework: nextjs }` with no `dev_cmd` or `port_kind`
- **WHEN** the orchestrator resolves the service definition
- **THEN** `dev_cmd` resolves to `pnpm next dev --turbopack`
- **AND** `port_kind` resolves to `frontend`

### Requirement: `pm run` orchestrated service startup
When invoked in a project whose `.project.yaml` defines `services:`, `pm run` SHALL ensure shared infrastructure (Postgres, Redis) is running, ensure the project database exists, ensure the daemon is running, register hostname routes, and spawn the configured service commands. With no positional arguments, all services in the project SHALL start; with a service identifier (e.g., `pm run front`), only that service SHALL start.

#### Scenario: pm run with no args starts all services
- **GIVEN** `.project.yaml` with services `front` and `back`
- **WHEN** `pm run` is invoked
- **THEN** both `front` and `back` are spawned
- **AND** routes for `front.<project>.<workspace>.localhost` and `back.<project>.<workspace>.localhost` are registered

#### Scenario: pm run with a service identifier
- **GIVEN** `.project.yaml` with services `front` and `back`
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
- **GIVEN** a project whose `.project.yaml` services include `front`, AND another project named `front` exists in the manifest
- **WHEN** `pm run front` is invoked from the first project's directory
- **THEN** the orchestrator interprets `front` as a service identifier, not a project name

### Requirement: Portless-style routing mechanism
The system SHALL run one reverse-proxy listener per PM configuration directory,
shared by every application started through orchestrator-mode `pm run`. PM SHALL
select an available per-service upstream port before spawning the service,
persist the assignment, and inject that port through the service's configured
port-kind environment variable. The service application or its configuration
SHALL consume that variable and bind to the assigned port. PM executes
`dev_cmd` directly without shell-variable interpolation, so a framework that
does not natively consume the injected variable MUST be configured by the
project to do so. PM SHALL NOT rely on discovering a port chosen independently
by the child process.

After a successful spawn, PM SHALL record the process in `services.json` and
project it into one or more hostname-to-port entries in `routes.json`. The
daemon SHALL accept each browser request on `dev.proxy_port`, remove the port
suffix from the HTTP `Host` header, look up the hostname in the runtime route
table, and forward the HTTP request to `127.0.0.1:<upstream_port>`.

This is application-layer HTTP reverse proxying, not DNS-based port assignment,
kernel NAT, or a general-purpose TCP/UDP port mapper. TLS and HTTP protocol
upgrade behavior are separate concerns. PostgreSQL, Redis, and other raw TCP
services MUST NOT be dispatched through this hostname route table; they SHALL
continue to use their shared/direct ports and PM-injected connection settings.

#### Scenario: PM chooses the port before starting an application
- **GIVEN** service `back` has no existing port assignment
- **WHEN** `pm run back` starts the service
- **THEN** PM selects a free port from the service's configured port-kind range
- **AND** the selected port is persisted before it is exposed as a route
- **AND** the child receives the selected port through its configured environment variable
- **AND** the service application or project configuration uses that variable as its listen port
- **AND** PM does not scan the child after startup to guess which port it chose

#### Scenario: all PM applications share one proxy
- **GIVEN** `front` and `back` belong to different projects and use different upstream ports
- **WHEN** both are started with `pm run`
- **THEN** both hostname routes point at the same listener on `dev.proxy_port`
- **AND** PM does not start one reverse proxy per service or per project

#### Scenario: HTTP Host selects the upstream
- **GIVEN** `routes.json` maps `back.api.work.localhost` to `127.0.0.1:26918`
- **WHEN** the shared proxy receives an HTTP request with `Host: back.api.work.localhost:7100`
- **THEN** it forwards that request to `127.0.0.1:26918`
- **AND** the browser-visible hostname remains `back.api.work.localhost`

#### Scenario: raw TCP is not hostname-routed
- **GIVEN** shared PostgreSQL listens on `127.0.0.1:5432`
- **WHEN** a PM-managed web application needs a database connection
- **THEN** PM injects the direct PostgreSQL connection settings
- **AND** PM does not add PostgreSQL to the HTTP hostname route table

### Requirement: `*.localhost` hostname routing
The system SHALL register hostname routes of the form `<service>.<project>.<workspace>.localhost` for each running service and route HTTP requests on the daemon's listening port to the per-service local port based on the `Host` header. When the workspace is `default`, the system SHALL also register the shorter alias `<service>.<project>.localhost`.

The `.localhost` suffix SHALL be used because compliant resolvers map it to the
local loopback host without an `/etc/hosts` entry. That resolution supplies only
an IP address; it does not select an upstream application port. Clients still
connect to the shared proxy's `dev.proxy_port`, and the proxy performs the
hostname-to-port lookup.

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

#### Scenario: localhost DNS does not replace the proxy
- **GIVEN** `front.api.work.localhost` resolves to a loopback address
- **WHEN** a browser opens `http://front.api.work.localhost:7100`
- **THEN** the browser connects to the shared proxy on port 7100
- **AND** the proxy, rather than DNS, chooses the service's upstream port

### Requirement: Service and route lifecycle reconciliation
`services.json` SHALL be the authoritative registry of PM-managed processes.
PID liveness SHALL determine whether a recorded process is currently running.
`routes.json` SHALL be a derived URL projection of those services, not an
independent application registry. Route registration SHALL be
idempotent per `(workspace, project, service)`: re-registering a service SHALL
remove all prior canonical and alias routes for that service before writing its
current routes.

PM SHALL remove the service record and every route belonging to that service
when `pm stop` succeeds. Re-running `pm run` for a service whose recorded PID is
still alive SHALL re-register its route projection, repairing a missing or stale
route without spawning a duplicate process. Before replacing a dead recorded
process, PM SHALL remove its prior route projection so a failed replacement
spawn does not leave that stale route behind. Read-only state consumers such as
the dashboard MUST NOT rewrite either registry as a side effect.

#### Scenario: service restart replaces all aliases
- **GIVEN** a service has a canonical hostname and a default-workspace alias pointing to port 13042
- **WHEN** PM re-registers the same service on port 13043
- **THEN** every route for that `(workspace, project, service)` points to port 13043
- **AND** no stale alias remains on port 13042

#### Scenario: explicit stop removes process and routes
- **GIVEN** `work/api/back` is recorded in `services.json` and has one or more entries in `routes.json`
- **WHEN** `pm stop back api` successfully stops it
- **THEN** its service record is removed
- **AND** all of its canonical and alias route entries are removed

#### Scenario: live process repairs a missing route
- **GIVEN** a service PID in `services.json` is alive but its route projection is missing
- **WHEN** the same service is targeted by `pm run`
- **THEN** PM re-registers the service route using its recorded port
- **AND** PM does not spawn a duplicate process

#### Scenario: dead process is not treated as running
- **GIVEN** a service PID remains in `services.json` after the process exits unexpectedly
- **WHEN** a read-only consumer reads the application state
- **THEN** the dead service is not reported as running
- **AND** the read does not mutate `services.json` or `routes.json`

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

### Requirement: Read-only PM applications dashboard
The shared proxy SHALL serve an HTML dashboard at
`http://pm.localhost:<proxy_port>/`. For convenience, requests to `/` whose
host is `localhost` or `127.0.0.1` on the same proxy port SHALL serve the same
page. These dashboard hostnames are reserved and SHALL be handled before normal
application route lookup.

The dashboard UI SHALL be implemented as a shadcn/ui React application whose
compiled static assets are embedded in the PM binary and served by the same
loopback proxy. It SHALL NOT require a second runtime server or stable port.
The client SHALL obtain its data from the read-only `GET /api/dashboard`
endpoint. That response SHALL use an explicitly versioned, dashboard-specific
JSON schema and SHALL NOT serialize the raw process registries.

The dashboard SHALL show every live process started and tracked by PM across
all workspaces and projects. It SHALL use `services.json` for detached
orchestrator services, `foreground-processes.json` for live legacy commands
started with `pm run -- <command>`, `ports.json` to classify service port kinds,
and `routes.json` only to project application URLs. A service with multiple
hostname aliases SHALL appear once, without duplicate application cards or
rows. Shared database containers and arbitrary processes that are not tracked
by either process registry SHALL NOT appear as dashboard applications.

At minimum, each entry SHALL display workspace, project, background/foreground
mode, PID, start time, and a service or executable name. Background services
SHALL also display their upstream port. Frontend services with a navigation
target SHALL render as application cards whose whole card is a single link,
preferring a matching Tailnet URL and falling back to the preferred local
hostname URL. Frontend services without a navigation target SHALL render as
unavailable application cards. A frontend with a known workspace/project/service
presentation SHALL display its versioned embedded flat risograph-style
illustration and concise product description in a restrained paper-and-ink
editorial treatment; other frontends SHALL retain a neutral fallback visual
without becoming hidden. Backend, infrastructure,
unclassified, and foreground processes SHALL render below the cards as
non-navigable summary rows without navigation addresses. Entries SHALL be
grouped or sorted deterministically. A live frontend service without a matching
route SHALL remain visible with routing shown as unavailable. A stale registry
entry MAY be shown in a separate attention state, but MUST NOT be counted or
linked as a running process. When no processes are tracked, the page SHALL show
an explicit empty state. Automatic refresh SHALL defer while an interactive
control has focus and SHALL expose a user-controlled pause/resume action.

The PM listener SHALL remain loopback-only and the dashboard SHALL be
read-only. An exact hostname listed in `dev.dashboard_hosts` MAY serve the
dashboard when a trusted local reverse proxy forwards that Host to the
loopback listener; arbitrary or wildcard external hosts SHALL NOT match. The
dashboard MAY discover Tailscale Serve loopback proxy mappings and show the
corresponding Tailnet URL on a matching process entry. A frontend card SHALL
prefer that Tailnet URL as its navigation target. It MUST NOT expose start, stop,
restart, route mutation, shell execution, environment values, database
credentials, log contents, or foreground command arguments. All
registry-derived strings SHALL be rendered as text and MUST NOT be injected as
HTML.

#### Scenario: dashboard lists applications across projects
- **GIVEN** `work/api/back` and `personal/blog/front` are live in `services.json`
- **WHEN** the user opens `http://pm.localhost:7100/`
- **THEN** the page contains one application entry for each service
- **AND** each entry identifies its workspace, project, service, PID, port, and start time
- **AND** the frontend is a clickable card while the backend is a non-clickable summary row

#### Scenario: dashboard deduplicates hostname aliases
- **GIVEN** `default/blog/front` has routes `front.blog.default.localhost` and `front.blog.localhost`
- **WHEN** the dashboard is rendered
- **THEN** it shows one `default/blog/front` application entry
- **AND** the card's preferred clickable URL uses `front.blog.localhost`

#### Scenario: direct loopback hosts serve the dashboard
- **WHEN** the user opens `http://localhost:7100/` or `http://127.0.0.1:7100/`
- **THEN** the same read-only applications dashboard is returned

#### Scenario: dashboard data API is versioned and read-only
- **WHEN** the client requests `GET /api/dashboard`
- **THEN** the response contains `schema_version: 1` and the projected dashboard entries
- **AND** it contains no environment values, credentials, logs, or command arguments
- **AND** `HEAD` returns the same metadata without a body
- **AND** mutation methods are rejected

#### Scenario: exact Tailnet dashboard host is allowed
- **GIVEN** `dev.dashboard_hosts` contains `devbox.example.ts.net`
- **AND** a local Tailnet reverse proxy forwards that Host to `127.0.0.1:7100`
- **WHEN** the user opens the configured Tailnet HTTPS URL
- **THEN** the same read-only dashboard is returned
- **AND** a different Tailnet hostname is handled as an unknown app route

#### Scenario: foreground command is visible without sensitive arguments
- **GIVEN** `pm run -- pnpm worker --token secret` has a live child process
- **WHEN** the dashboard is rendered
- **THEN** the process is labeled Foreground with its project, executable name, PID, and start time
- **AND** neither `--token` nor `secret` appears in the dashboard or registry

#### Scenario: Tailnet frontend links follow active Serve mappings
- **GIVEN** Tailscale Serve proxies a Tailnet HTTPS URL to a live PM frontend service's loopback port
- **WHEN** the dashboard is rendered
- **THEN** that frontend card navigates to the Tailnet HTTPS URL
- **AND** mappings to non-loopback targets are ignored

#### Scenario: route projection is missing
- **GIVEN** a live frontend service exists in `services.json` but has no matching entry in `routes.json`
- **WHEN** the dashboard is rendered
- **THEN** the service remains listed
- **AND** its URL is marked unavailable rather than inventing a route

#### Scenario: stale PID is not linked as a running app
- **GIVEN** a service record remains after its PID has exited
- **WHEN** the dashboard is rendered
- **THEN** the entry is marked as stopped or omitted from the running list
- **AND** it has no clickable application URL
- **AND** rendering the page does not modify either state file

#### Scenario: dashboard has no mutation actions
- **WHEN** the dashboard is rendered
- **THEN** it contains no controls that start, stop, restart, or otherwise mutate applications
- **AND** it does not expose environment values, credentials, or log contents

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
- **GIVEN** `.project.yaml` declares `services: { front: { framework: nextjs } }` with no `dev_cmd`
- **WHEN** `pm run front` is invoked
- **THEN** the spawned command is `pnpm next dev --turbopack`

#### Scenario: user override is respected
- **GIVEN** `.project.yaml` declares `services: { front: { framework: nextjs, dev_cmd: "pnpm dev" } }`
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
