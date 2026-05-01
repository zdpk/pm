## Why

현재 `pm run` 은 사용자가 명시적으로 래퍼를 거쳐야만 환경 변수가 주입되어 IDE·dotenv 라이브러리·docker-compose 등 정적 도구와 마찰이 크다. 또한 monolith 아키텍처에서 front/back 을 동시에 띄우거나, 로컬 Postgres/Redis 컨테이너를 매 프로젝트마다 수동으로 세팅하는 반복 작업도 사용자 부담이다. portless (vercel-labs) 가 검증한 "hostname-as-identity + 데몬 라우팅 + 자동 spawn" 패턴을 pm 의 기존 stateless 명령들과 공존시키는 형태로 도입해, `pm run` 한 번으로 dev 환경 전체가 일관되게 기동되는 워크플로를 만든다.

## What Changes

### 신규 명령

- `pm run` (현재 project 의 모든 service 기동) — 기존 `pm run [project] -- <cmd>` grammar 와 디스앰비규에이션 규칙으로 공존
- `pm run <service>` / `pm run <service> <project>` — 특정 service 만 기동 (`front`, `back` 등 `.proj.yaml` 의 services 키)
- `pm logs [service] [project]` — 실행 중 service 의 로그 tail
- `pm stop [service] [project]` — service 종료 (인자 없으면 현재 project 의 모두)
- `pm db status` / `pm db start` / `pm db stop` — 공용 Postgres·Redis 컨테이너 lifecycle
- `pm proxy status` / `pm proxy start --foreground` / `pm proxy stop` — 데몬 명시적 제어

### 데몬 + hostname 라우팅

- 첫 `pm run` 호출 시 같은 pm 바이너리가 `pm __daemon` 모드로 자기 자신을 detached spawn (gpg-agent 패턴)
- 데몬은 `127.0.0.1:80`/`:443` 에서 reverse proxy 로 동작, `~/.config/pm/routes.json` 을 매 요청마다 읽어 hostname 기반 라우팅
- 도메인 패턴: `<service>.<project>.<workspace>.localhost` (예: `front.api.work.localhost`)
- `*.localhost` 는 RFC 6761 + 모든 주요 OS 의 자동 해석에 의존 (hosts/DNS 변경 불요)
- 라우팅 모델 A (기본): service 별 subdomain. 모델 B (opt-in): single-origin path-based, `.proj.yaml` 의 service 별 `path:` 옵션으로 활성화

### Docker auto-start

- 컨테이너 `pm-local-db` (Postgres) / `pm-local-redis` (Redis) 를 `pm run` 진입 시 자동 기동
- 볼륨: `pm-local-volume` (Postgres) / `pm-local-redis-volume` (Redis), 명명 표기 kebab-case
- 5432/6379 가 이미 listening 이면 외부 인스턴스로 가정하고 컨테이너 기동 skip
- `pm run` 종료 시 컨테이너는 살아있음, 명시적 `pm db stop` 으로만 종료
- Docker 미설치 시 친절한 안내 + `auto_start: false` 로 끌 수 있음 (`config.json`)

### 자동 DB 생성

- `pm run` 진입 시 프로젝트 DB 가 없으면 `CREATE DATABASE` 자동 실행
- DB 이름: `<workspace>_<project>` (snake_case, hyphen → underscore 정규화). v0.3.0 의 `_local` 서픽스 제거.

### `.proj.yaml` 스키마 확장

- `services:` 섹션 신설 — 각 service 의 `dir`, `dev_cmd`, `port_kind`, 선택적 `path` 정의
- framework 기반 기본값 (`nextjs` → `dev_cmd: pnpm next dev --turbopack, port_kind: frontend` 등)

### Next.js 컨벤션: pnpm + Turbopack

- Next.js service 의 dev_cmd 기본값을 `pnpm next dev --turbopack` 으로 고정 — npm/yarn 대신 pnpm, webpack 대신 Turbopack 강제
- `configs/typescript/nextjs/` 번들 템플릿에 `.npmrc` 추가 (`engine-strict=true`, `auto-install-peers=true`)
- `pm proj init -l ts -f nextjs` 가 `package-lock.json`/`yarn.lock` 존재 시 stderr 경고

### Breaking changes

- **BREAKING**: `local_database_name(workspace, project)` 가 더 이상 `_local` 을 붙이지 않음. v0.3.0 사용자가 만든 로컬 DB (`work_api_local`) 는 고립되며, 첫 `pm run` 시 새 이름 (`work_api`) 의 빈 DB 가 생성된다. 데이터 이전은 사용자 책임 (수동 `pg_dump`/`pg_restore` 또는 `RENAME DATABASE`).
- **BREAKING**: `pm run` grammar 확장 — `.proj.yaml` 이 있고 첫 인자가 services 키와 일치하면 service 기동으로 해석. `.proj.yaml` 없거나 `--` 가 있으면 기존 grammar 유지하므로 v0.3.0 사용자 회귀 0.

## Capabilities

### New Capabilities
- `local-dev-orchestrator`: `pm run` 의 service 기동, 데몬 lifecycle, hostname 라우팅, Docker auto-start, 자동 DB 생성, service 정의 스키마를 포괄.

### Modified Capabilities
- `local-port-management`: `local_database_name` 의 `_local` 서픽스 제거 (Requirement "Local database name normalization" 의 모든 Scenario 갱신). DATABASE_URL 형식도 그에 따라 변경.

## Impact

### Code
- `src/cli.rs` — `Commands::Run` grammar 확장, 신규 `Logs`/`Stop`/`Db`/`Proxy` 서브커맨드
- `src/commands/run.rs` — service 기동 분기, 데몬 ensure, routes.json 갱신, dev cmd spawn, 기존 `--` cmd 모드 보존
- 신규 `src/commands/dev/mod.rs` (또는 `commands/services.rs`) — service 기동/중지/로그
- 신규 `src/commands/proxy.rs` — `pm __daemon` 진입점, reverse proxy 본체, `pm proxy status/stop`
- 신규 `src/commands/db.rs` — Docker 컨테이너 lifecycle, `CREATE DATABASE` 자동
- `src/project.rs` — `ProjConfig` 의 `services: Vec<ServiceDef>` 추가, framework 기본값 추론, `dev_cmd` resolve
- `src/commands/run.rs` 의 `local_database_name` — `_local` 제거 (BREAKING)

### Dependencies (신규)
- `hyper` 또는 `tower-http` — reverse proxy
- `tokio-postgres` — DB 존재 확인 + CREATE DATABASE
- `bollard` 또는 외부 `docker` CLI invoke — 컨테이너 lifecycle (외부 CLI 가 의존성 가벼움)
- 데몬용 비동기 런타임 `tokio` (이미 transitive 가능)

### Config / 파일 시스템
- `~/.config/pm/routes.json` — 데몬용 hostname → port 매핑 (CLI 가 쓰고 데몬이 읽음)
- `~/.config/pm/daemon.pid` — 데몬 PID 파일
- `~/.config/pm/daemon.sock` (선택) — control plane IPC
- `~/.config/pm/logs/<workspace>_<project>_<service>.log` — service 로그 저장
- `config.json` 에 신규 옵션 `dev: { auto_start_docker: bool, postgres_image: String, redis_image: String }`

### Network
- 데몬이 `127.0.0.1:80` 바인딩 — Linux 에서는 `CAP_NET_BIND_SERVICE` 또는 root 필요. 회피 옵션: 비특권 포트 (`:7100` 등) 기본, `--privileged` 시 80 사용.
- `*.localhost` 자동 해석은 macOS/Linux/Windows 모두 지원 (확인 필요: 일부 Linux 배포는 `nss-myhostname` 의존)

### Documentation
- `README.md` / 한국어판 — Local Dev Orchestrator 섹션 신규
- v0.3.0 사용자용 마이그레이션 가이드 (DB 이름 변경, `pm run` 새 동작)

### Versioning
- v0.4.0 — minor bump (pre-1.0 의 BREAKING 은 minor 로 처리)
