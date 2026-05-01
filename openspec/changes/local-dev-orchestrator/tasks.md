## 1. 의존성 및 Config 스키마

- [x] 1.1 `Cargo.toml` 에 sync `postgres` 추가 (Stage 1 자동 DB 생성용). `hyper`/`hyper-util`/`tokio-postgres`/`tokio` 는 Stage 2 데몬 작업 시 추가
- [x] 1.2 `src/models.rs` 에 `DevConfig { auto_start_docker, proxy_port, control_port, postgres_image, redis_image }` 추가, `Config` 에 `#[serde(default)] pub dev: DevConfig`
- [x] 1.3 `Default for DevConfig` 구현 (auto_start_docker=true, proxy_port=7100, control_port=7101, images: "postgres:16", "redis:7")
- [x] 1.4 단위 테스트: 기존 config.json 이 dev 섹션 없이 로드되어도 default 적용 (config_loads_with_default_dev_section_when_missing) + override 동작 (config_dev_section_overrides_apply)

## 2. `.proj.yaml` services 스키마

- [x] 2.1 `src/project.rs` 의 `ProjConfig` 에 `services: HashMap<String, ServiceDef>` 필드 추가
- [x] 2.2 `ServiceDef { dir, dev_cmd, port_kind, framework, path }` 모두 Optional 로 정의
- [x] 2.3 framework→defaults 추론 함수 `resolve_service_defaults(&ServiceDef, project_framework: Option<&str>) -> ResolvedService` 구현 (D9 의 표 그대로)
- [x] 2.4 nextjs 의 dev_cmd 기본값을 `"pnpm next dev --turbopack"` 으로 고정
- [x] 2.5 `.proj.yaml` parsing: services 섹션 부재 시 빈 HashMap, 호환성 보장
- [x] 2.6 단위 테스트 9개: framework 별 기본값, 사용자 override 우선, service-level framework 우선, dir 기본값/오버라이드, services 부재 시 v0.3.0 호환, path 필드 파싱 (Phase 2 reserved)

## 3. Docker 컨테이너 lifecycle

- [x] 3.1 신규 `src/commands/db.rs` 모듈
- [x] 3.2 `docker_available() -> bool` (외부 `docker` CLI 호출 가능 여부 체크)
- [x] 3.3 `ensure_postgres(image: &str)` — 컨테이너 존재 확인 → 없으면 `docker run -d --name pm-local-db -p 5432:5432 ...`, 있고 stopped 면 `docker start`
- [x] 3.4 `ensure_redis(image: &str)` — 동일 패턴, `pm-local-redis` + `pm-local-redis-volume`
- [x] 3.5 `is_port_available(5432)` 가 false 면 외부 인스턴스 가정, 컨테이너 skip + 로그 출력
- [x] 3.6 `pm db status/start/stop` CLI 분기 (`Commands::Db(DbCommand)`)
- [x] 3.7 Docker 미설치 / 미실행 시 친절한 에러: "Docker is required for `pm run`. Install Docker or set `dev.auto_start_docker: false`"
- [x] 3.8 단위 테스트 2개: ContainerState enum 동등성, error message 형식 (mock 추상화는 Stage 2 의 통합 테스트로 이연)

## 4. 자동 DB 생성

- [x] 4.1 sync `postgres` 클라이언트로 `127.0.0.1:5432` 연결 헬퍼 (`admin_connection_string`). tokio-postgres 는 Stage 2 에서 데몬용으로 추가
- [x] 4.2 `database_exists(client, name) -> bool` (`SELECT 1 FROM pg_database WHERE datname = $1`)
- [x] 4.3 `ensure_database_on_loopback(client, host, name)` — 미존재 시 `CREATE DATABASE "{name}"` (이중 큰따옴표 escape 가드)
- [x] 4.4 connection target 가드: `is_loopback_host(host)` 가 false 면 ensure_database skip + stderr 안내
- [x] 4.5 `local_database_name(workspace, project)` 의 `_local` 서픽스 제거 (BREAKING)
- [x] 4.6 `legacy_v03_databases()` + `emit_v03_migration_notice()` 헬퍼 추가 (Stage 2 에서 orchestrator 가 호출)
- [x] 4.7 기존 `commands/run.rs` 단위 테스트 갱신 (`work_my_app` 등 `_local` 없이 기대) + no-suffix 검증 테스트 추가

## 5. 데몬 본체 (`pm __daemon`)

- [x] 5.1 `src/cli.rs` 에 숨김 서브커맨드 `Commands::Daemon` (`#[command(name = "__daemon", hide = true)]`) + `ProxyCommand` 추가
- [x] 5.2 신규 `src/commands/proxy/{mod,daemon,reverse,control}.rs` — 데몬 모듈 분리
- [x] 5.3 `tokio::runtime::Builder` 멀티스레드 런타임, `daemon.pid` 파일 쓰기, 종료 시 정리
- [x] 5.4 `hyper` http1 으로 `127.0.0.1:{proxy_port}` listener, `serve_connection.with_upgrades()` 로 WebSocket 호환
- [x] 5.5 `Host` 헤더 → `RoutesCache` lookup, 없으면 404
- [x] 5.6 `routes.json` mtime 캐싱 (`refresh_if_changed` 매 요청 시 `metadata.modified()` 비교)
- [x] 5.7 graceful shutdown: `tokio::signal` 으로 SIGTERM/SIGINT 처리, `Notify` 로 양 listener 동시 종료
- [x] 5.8 데몬 stdout/stderr 를 `~/.config/pm/logs/daemon.log` 로 redirect (`spawn_detached` 의 `Stdio::from`)
- [x] 5.9 `#[cfg(unix)]` 로 게이트, non-Unix 빌드에서는 `pm proxy`/`pm __daemon` 모두 안내 에러

## 6. Control plane

- [x] 6.1 `127.0.0.1:{control_port}` 에 별도 hyper http1 listener
- [x] 6.2 `GET /health` → `200 OK { pid }`
- [x] 6.3 `GET /status` → `StatusBody { pid, uptime_sec, proxy_port, control_port, routes_count }`
- [x] 6.4 `POST /reload` → routes.json 은 매 요청마다 mtime 체크라 사실상 no-op, 호환성 위해 200 응답
- [x] 6.5 `POST /stop` → `Notify::notify_waiters()` 로 graceful shutdown trigger
- [x] 6.6 `/spawn` 은 Phase 2 (Stage 3) — 현재는 CLI 가 직접 서비스 spawn 후 routes.json 등록

## 7. CLI 측 데몬 ensure 로직

- [x] 7.1 `daemon::check_alive() -> Result<Option<u32>>` — daemon.pid + `nix::sys::signal::kill(pid, None)` + control plane `/health` 200 모두 통과 시 Some(pid)
- [x] 7.2 `daemon::spawn_detached()` — `current_exe()` + `__daemon` 인자, Unix `pre_exec` 로 `nix::unistd::setsid()`, stdio null/log
- [x] 7.3 `wait_until_ready` — exponential backoff (25→250ms), 최대 5초
- [x] 7.4 stale daemon.pid 처리: PID 가 죽었거나 control plane 무응답이면 파일 삭제 + None 반환

## 8. routes.json 관리

- [x] 8.1 `RouteEntry { hostname, upstream_port, project_key, service_key }` + `RoutesData { version, entries }` 모델 (`src/routes.rs`)
- [x] 8.2 `load_routes()` / `save_routes()` — tmp 파일 + atomic `fs::rename`
- [x] 8.3 atomic rename 으로 torn write 방지 (advisory `flock` 은 차후 — atomic rename 만으로 충분히 안전)
- [x] 8.4 `register_service(workspace, project, service, port)` — canonical hostname + (workspace == default 시) 단축 alias 동시 등록, 기존 `(project_key, service_key)` 항목 idempotent 교체
- [x] 8.5 `unregister_project()` / `unregister_service()` — Stage 3 의 `pm stop` 에서 호출 예정

## 9. Service 기동 (`pm run` orchestrator 모드)

- [x] 9.1 `commands/run.rs` 의 grammar disambiguation: `--` 있으면 v0.3.0 legacy 모드, 없고 services 정의 있으면 orchestrator 모드
- [x] 9.2 첫 인자가 `.proj.yaml` services 키면 service identifier, 아니면 project name 으로 해석 (`first_service_token`)
- [x] 9.3 orchestrator 진입 시: ensure_postgres → ensure_redis → ensure_database → ensure_daemon → 각 service spawn → register_routes
- [x] 9.4 dev_cmd spawn: `current_dir = service.dir`, env vars (build_port_env + service-specific port) 주입, stdout/stderr 를 service log file 로 redirect
- [x] 9.5 spawned service 의 PID/port/started_at/log_path/dev_cmd 를 `~/.config/pm/services.json` 에 저장
- [x] 9.6 SIGINT 격리: spawned service 는 `setsid()` 로 새 session 이라 부모 셸 Ctrl+C 영향 안 받음. 명시적 `pm stop` 으로만 종료
- [x] 9.7 `pm run` 인자 0 → 모든 service, `pm run <svc>` → 한 service 만 (`pick_services` 분기)
- [x] 9.8 `pm run <svc> <project>` → 다른 project 의 service 가능 (`resolve_project_from_positional`)

## 10. `pm logs` / `pm stop`

- [x] 10.1 `Commands::Logs { service, project }` CLI 추가
- [x] 10.2 `pm logs <service>` — `tail -f` 동등 구현 (100ms polling, file rotation 감지 시 reopen)
- [x] 10.3 `Commands::Stop { service, project }` CLI 추가
- [x] 10.4 `pm stop` (인자 0) → 현재 project 의 모든 service SIGTERM (2초 대기 후 SIGKILL fallback)
- [x] 10.5 `pm stop <svc>` → 해당 service 만
- [x] 10.6 종료 후 services.json 갱신 + routes.json 항목 제거

## 11. 로그 파일 rotation

- [x] 11.1 `src/log_rotation.rs` 신규 모듈 — `rotate_if_needed`/`rotate` 함수
- [x] 11.2 10MiB 초과 시 spawn 시점에 `.log → .log.1 → .log.2 → .log.3`, .log.4 는 삭제. v0.4.0 은 spawn-시점 rotation 만 (online rotation 은 후속)
- [x] 11.3 단위 테스트 4개: 임계 미달 noop, 초과 시 rotation, 4번 연속 rotation 후 .log.1~3 만 남음, 부재 파일 noop

## 12. `pm proxy` 명령

- [x] 12.1 `Commands::Proxy(ProxyCommand)` — Status / Start / Stop (Stage 2 완료)
- [x] 12.2 `pm proxy status` → control plane `/status` 호출, PID/uptime/proxy_port/control_port/routes_count 출력
- [x] 12.3 `pm proxy stop` → `/stop` 호출, daemon.pid 삭제 대기 (최대 2초)
- [x] 12.4 `pm proxy start --foreground` → 데몬을 foreground 로 실행 (디버깅용)

## 13. Next.js 컨벤션 enforcement

- [x] 13.1 `configs/ts/nextjs/.npmrc` 신규 추가 (`engine-strict=true`, `auto-install-peers=true`, `strict-peer-dependencies=false`). `configs/typescript/` → `configs/ts/` 디렉토리 rename (사전 manifest.yaml id mismatch 버그 수정)
- [x] 13.2 `configs/ts/nextjs/manifest.yaml` 에 `.npmrc` 항목 (strategy: managed) 추가
- [x] 13.3 `pm project init -f nextjs` 가 lockfile 검사: `package-lock.json` / `yarn.lock` / `bun.lockb` 존재 시 stderr 경고 (`warn_on_competing_lockfiles`)
- [x] 13.4 `pm project init` 시 `--no-services` 플래그가 없으면 `.project.yaml` 에 framework 별 default service 자동 추가 (`default_services_for_framework`)
- [x] 13.5 단위 테스트 4개: nextjs/axum/unknown/None framework 별 default services + manual E2E verify lockfile 경고 출력 + .npmrc 적용 확인
- [x] 13.6 (drive-by) `config_repo_head` 가 git repo 가 아닌 bundled configs 에서도 동작하도록 `Repository::discover` + "bundled" fallback

## 14. Windows / 비-Unix 가드

- [x] 14.1 `pm run` orchestrator 분기에 `#[cfg(not(unix))]` 에러 메시지 추가 (commands/run.rs)
- [x] 14.2 daemon 진입점 (`commands/proxy/mod.rs`) 도 동일 가드. logs/stop 명령도 main.rs dispatch 에서 cfg-not(unix) 안내 에러
- [x] 14.3 stateless 명령 (`pm run -- <cmd>`, `pm ports`, `pm ws`, `pm db status`) 은 cfg gate 없음. CI release.yml 은 현재 Linux/macOS 만 빌드, Windows 타겟은 v0.4.0 범위 외 (차후 별도 change)

## 15. 통합 검증

- [x] 15.1 `cargo build` (debug + release) 통과 — 로컬 macOS, CI 는 v0.4.0 tag push 시 검증
- [x] 15.2 `cargo clippy --all-targets` — 27 warnings, baseline 동일 (신규 introduced 0)
- [x] 15.3 `cargo test` — 107 passed, 0 failed (Stage 1~4 누적 +25 신규)
- [x] 15.4 수동 E2E: `.project.yaml` 에 services 정의 → `pm run` → routes.json + services.json 정상 등록, hostname 출력 확인 (Stage 3)
- [x] 15.5 수동 E2E: `pm run back` 단독 기동, `pm proxy status`, `pm stop`, `pm proxy stop` (Stage 3)
- [x] 15.6 Docker 없는 환경: `require_docker()` → 친절한 에러 ("Docker is required ... or set dev.auto_start_docker: false")
- [x] 15.7 수동 E2E: 외부 Postgres 5432 점유 시 `ContainerState::ExternalInUse` → "skipped pm-local-db (external port owner detected)" 출력 (Stage 3)
- [x] 15.8 v0.3.0 호환 검증: `.project.yaml` 없는 프로젝트에서 `pm run -- echo HELLO` 정상 출력
- [x] 15.9 BREAKING: legacy DB 발견 시 `emit_v03_migration_notice()` 가 stderr 안내 출력
## 16. 문서화

- [x] 16.1 `README.md` Local Port Management 섹션 갱신: BREAKING DB name 반영
- [x] 16.2 신규 "Local Dev Orchestrator (v0.4.0)" 섹션: services schema, `pm run`/`pm logs`/`pm stop`/`pm db`/`pm proxy` 사용법, hostname routing, Docker auto-start, grammar disambiguation 표
- [x] 16.3 Next.js 컨벤션 (pnpm + Turbopack + .npmrc) 명시
- [x] 16.4 v0.3.0 → v0.4.0 마이그레이션 가이드 (DB 이름 변경, `pg_dump | psql` 예시)
- [x] 16.5 비-Unix 사용자용 안내 ("Unix only in v0.4.0", Windows = Phase 2)
- [x] 16.6 path-based routing Phase 2 (README 의 services schema 설명 + design.md Resolved Decisions 참조)

## 17. 버전 bump 및 릴리스

- [x] 17.1 Cargo.toml version 0.3.0 → 0.4.0
- [x] 17.2 git commit + tag v0.4.0
- [x] 17.3 GitHub Actions release workflow 정상 동작 확인 (3 타겟 빌드)
- [x] 17.4 GitHub Release notes 에 BREAKING 명시 (release workflow `generate_release_notes: true` + commit messages 의 BREAKING 표기)
